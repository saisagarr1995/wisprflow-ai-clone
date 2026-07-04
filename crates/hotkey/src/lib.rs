//! Global hotkey listening via low-level keyboard hooks.
//!
//! Why not `RegisterHotKey` / the `global-hotkey` crate: push-to-talk needs
//! key-*up* events and modifier-only bindings ("hold Right Ctrl"), which those
//! APIs cannot express (design doc §1.2). The platform hook feeds raw key
//! transitions into a pure `ChordTracker`, which is where all semantics live —
//! and what tests exercise without any OS dependency.

mod parse;
mod tracker;
#[cfg(windows)]
mod windows_hook;

pub use parse::{parse_chord, Chord, KeyMatcher, ParseError};
pub use tracker::ChordTracker;
#[cfg(windows)]
pub use windows_hook::HotkeyListener;

/// The chord went fully down / stopped being fully down.
/// Hold-vs-toggle interpretation happens in the orchestrator, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    #[error("failed to install keyboard hook: {0}")]
    HookInstall(String),
}
