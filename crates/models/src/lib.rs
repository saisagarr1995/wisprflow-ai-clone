//! Model management: STT/VAD model registry, checksummed + resumable
//! downloads into the app data dir, and Ollama model presence checks.
//!
//! LLM weights are Ollama's job (`ollama pull`); this crate only downloads
//! STT/VAD artifacts and *verifies* Ollama has what the config needs.

mod download;
mod ollama;
mod registry;

pub use download::{ensure, ensure_entry, DownloadProgress};
pub use ollama::{check_ollama, OllamaStatus};
pub use registry::{registry, ModelEntry, ModelKind};

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("unknown model id '{0}'")]
    UnknownModel(String),
    #[error("download failed: {0}")]
    Download(String),
    #[error("checksum mismatch for {file}: expected {expected}, got {actual}")]
    ChecksumMismatch { file: String, expected: String, actual: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ollama unreachable at {url}: {reason}")]
    OllamaUnreachable { url: String, reason: String },
}

/// `%APPDATA%/openflow/models` (Windows) etc. Overridable for tests via
/// the `OPENFLOW_DATA_DIR` environment variable.
pub fn models_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("OPENFLOW_DATA_DIR") {
        return PathBuf::from(dir).join("models");
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openflow")
        .join("models")
}
