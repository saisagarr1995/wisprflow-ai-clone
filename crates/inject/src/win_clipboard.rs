use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_V,
};

use crate::{InjectError, Injector};

/// Clipboard-swap paste: save clipboard text → set our text → synthesize
/// Ctrl-V → wait `restore_delay` → restore. Only text content is preserved
/// across the swap in v1; rich formats are a known limitation (design doc §8).
pub struct ClipboardPasteInjector {
    pub restore_delay: Duration,
}

impl ClipboardPasteInjector {
    pub fn new(restore_delay_ms: u64) -> Self {
        Self {
            restore_delay: Duration::from_millis(restore_delay_ms),
        }
    }
}

impl Injector for ClipboardPasteInjector {
    fn inject(&mut self, text: &str) -> Result<(), InjectError> {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| InjectError::Clipboard(e.to_string()))?;
        // Non-text clipboard content (images, files) is not preserved in v1.
        let saved = clipboard.get_text().ok();

        clipboard
            .set_text(text)
            .map_err(|e| InjectError::Clipboard(e.to_string()))?;
        send_ctrl_v()?;

        // Give the target app time to read the clipboard before restoring.
        std::thread::sleep(self.restore_delay);
        if let Some(prev) = saved {
            let _ = clipboard.set_text(prev);
        }
        Ok(())
    }
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_ctrl_v() -> Result<(), InjectError> {
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VK_V, false),
        key_input(VK_V, true),
        key_input(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(InjectError::Synthesis(format!(
            "SendInput injected {sent}/{} events",
            inputs.len()
        )));
    }
    Ok(())
}
