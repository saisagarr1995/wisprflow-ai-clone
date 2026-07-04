//! Text injection into the focused application.
//!
//! Strategy per design doc §8: clipboard-swap + Ctrl-V paste is the primary
//! Windows method (instant for any length, full Unicode); synthetic
//! keystrokes and per-app strategy tables arrive with the coding/terminal
//! work (Prompt 22). The clipboard is saved and restored around the paste,
//! with a configurable delay so slow targets read the data first.

mod fake;
#[cfg(windows)]
mod win_clipboard;
#[cfg(windows)]
mod win_keystroke;

pub use fake::FakeInjector;
#[cfg(windows)]
pub use win_clipboard::ClipboardPasteInjector;
#[cfg(windows)]
pub use win_keystroke::{sanitize_for_terminal, KeystrokeInjector};

#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("input synthesis failed: {0}")]
    Synthesis(String),
}

/// Inject text at the caret of the currently focused application.
pub trait Injector: Send {
    fn inject(&mut self, text: &str) -> Result<(), InjectError>;
}
