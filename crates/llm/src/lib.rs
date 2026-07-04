//! Language-model access: the `LlmBackend` trait, Ollama client, per-task
//! model routing, the dictation-cleanup prompt, and the output guard.
//!
//! Design doc §3: the hot-path cleanup call runs on *every* utterance with a
//! small resident model; quality-sensitive transforms route to a larger model.
//! §2 stage 8: LLM output is never trusted blindly — the guard falls back to
//! the deterministic transcript whenever the model misbehaves.

mod backend;
mod cleanup;
mod guard;
mod ollama;
mod router;

pub use backend::{ChatMessage, FakeLlm, LlmBackend, LlmError};
pub use cleanup::{CleanupContext, CleanupResult, CleanupService};
pub use guard::sanitize;
pub use ollama::OllamaBackend;
pub use router::{model_for, TaskKind};
