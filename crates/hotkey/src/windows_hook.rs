//! Windows `WH_KEYBOARD_LL` hook. The hook runs on a dedicated thread with a
//! message pump; per Win32 rules the callback executes on that same thread, so
//! tracker + sender live in a thread-local. The callback does no work beyond
//! feeding the tracker and sending on an unbounded channel — slow LL hooks get
//! silently removed by the OS.

use std::cell::RefCell;
use std::sync::mpsc;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::parse::Chord;
use crate::tracker::ChordTracker;
use crate::{HotkeyError, HotkeyEvent};

thread_local! {
    static HOOK_STATE: RefCell<Option<(ChordTracker, mpsc::Sender<HotkeyEvent>)>> =
        const { RefCell::new(None) };
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        let transition = match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
            WM_KEYUP | WM_SYSKEYUP => Some(false),
            _ => None,
        };
        if let Some(pressed) = transition {
            HOOK_STATE.with(|s| {
                if let Some((tracker, tx)) = s.borrow_mut().as_mut() {
                    if let Some(event) = tracker.on_key(kb.vkCode, pressed) {
                        let _ = tx.send(event);
                    }
                }
            });
        }
    }
    // Never swallow: dictation hotkeys pass through to the focused app.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Owns the hook thread. Dropping uninstalls the hook and joins the thread.
pub struct HotkeyListener {
    thread_id: u32,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl HotkeyListener {
    /// Install the hook and stream chord events to `tx` until dropped.
    pub fn start(chord: Chord, tx: mpsc::Sender<HotkeyEvent>) -> Result<Self, HotkeyError> {
        let (ready_tx, ready_rx) = mpsc::channel();

        let thread = std::thread::Builder::new()
            .name("openflow-hotkey".into())
            .spawn(move || {
                HOOK_STATE.with(|s| *s.borrow_mut() = Some((ChordTracker::new(chord), tx)));
                let hook =
                    match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) } {
                        Ok(h) => h,
                        Err(e) => {
                            let _ = ready_tx.send(Err(HotkeyError::HookInstall(e.to_string())));
                            return;
                        }
                    };
                let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));

                let mut msg = MSG::default();
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                let _ = unsafe { UnhookWindowsHookEx(hook) };
                HOOK_STATE.with(|s| *s.borrow_mut() = None);
            })
            .map_err(|e| HotkeyError::HookInstall(e.to_string()))?;

        let thread_id = ready_rx
            .recv()
            .map_err(|_| HotkeyError::HookInstall("hook thread died during setup".into()))??;
        Ok(Self {
            thread_id,
            thread: Some(thread),
        })
    }
}

impl Drop for HotkeyListener {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
