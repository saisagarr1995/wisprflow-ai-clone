//! Session orchestration: the single-threaded event loop and state machine
//! that owns a dictation session end to end
//! (idle → recording → transcribing → injecting).
//!
//! Everything real-time-adjacent (hotkey hook, audio callbacks) posts events
//! into the orchestrator's channel; the loop is the only code that touches
//! session state, which is what keeps hotkey/audio/config races structurally
//! impossible (design doc §5).

mod orchestrator;

pub use orchestrator::{Orchestrator, OrchestratorHandle, SessionEvent, Status};
