//! Best-effort nearby-text capture via UI Automation (design doc §7 layer 3).
//! Works in editors, browsers, and most Electron apps; silently returns
//! nothing for terminals, games, and apps without a UIA text implementation.

use std::sync::mpsc;
use std::time::Duration;

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
    UIA_TextPatternId, UIA_ValuePatternId,
};

use crate::TextContext;

const PRECEDING_CHARS: usize = 200;

/// Capture with a hard timeout: UIA calls into the target app's UI thread,
/// which can hang — the pipeline must never wait on it (design doc §7).
pub fn capture_text_context(timeout_ms: u64) -> TextContext {
    let (tx, rx) = mpsc::channel();
    // Detached thread: if it hangs, it leaks until the target app unblocks —
    // harmless — while dictation proceeds without context.
    let _ = std::thread::Builder::new()
        .name("openflow-uia".into())
        .spawn(move || {
            let _ = tx.send(capture_inner());
        });
    rx.recv_timeout(Duration::from_millis(timeout_ms))
        .unwrap_or_default()
}

fn tail(text: &str) -> Option<String> {
    let trimmed = text.trim_end();
    if trimmed.trim().is_empty() {
        return None;
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let start = chars.len().saturating_sub(PRECEDING_CHARS);
    Some(chars[start..].iter().collect())
}

fn capture_inner() -> TextContext {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let result = capture_via_uia();
        CoUninitialize();
        result.unwrap_or_default()
    }
}

unsafe fn capture_via_uia() -> windows::core::Result<TextContext> {
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let element = unsafe { automation.GetFocusedElement()? };
    let mut ctx = TextContext::default();

    if let Ok(pattern) = unsafe { element.GetCurrentPattern(UIA_TextPatternId) } {
        if let Ok(text_pattern) = pattern.cast::<IUIAutomationTextPattern>() {
            if let Ok(selection) = unsafe { text_pattern.GetSelection() } {
                if unsafe { selection.Length() }? > 0 {
                    let range = unsafe { selection.GetElement(0) }?;
                    let sel = unsafe { range.GetText(4000) }?.to_string();
                    if !sel.trim().is_empty() {
                        ctx.selection = Some(sel);
                    }
                }
            }
            if let Ok(doc_range) = unsafe { text_pattern.DocumentRange() } {
                // Approximation: the field's tail is "text before the caret"
                // for the append-at-end case that dominates dictation.
                let text = unsafe { doc_range.GetText(4000) }?.to_string();
                ctx.preceding_text = tail(&text);
            }
        }
    }
    if ctx.preceding_text.is_none() {
        if let Ok(pattern) = unsafe { element.GetCurrentPattern(UIA_ValuePatternId) } {
            if let Ok(value_pattern) = pattern.cast::<IUIAutomationValuePattern>() {
                let value = unsafe { value_pattern.CurrentValue() }?.to_string();
                ctx.preceding_text = tail(&value);
            }
        }
    }
    Ok(ctx)
}
