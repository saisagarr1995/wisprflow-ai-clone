//! Speech-to-text: the `SttEngine` trait and backends (whisper.cpp now,
//! Parakeet TDT via ONNX in Prompt 10) plus, later, streaming partial policies.
//!
//! Everything downstream consumes `Transcript`; everything upstream provides
//! 16 kHz mono f32 audio (the `openflow-audio` contract).

mod fake;
mod whisper;

pub use fake::FakeStt;
pub use whisper::WhisperEngine;

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("model load failed: {0}")]
    ModelLoad(String),
    #[error("transcription failed: {0}")]
    Inference(String),
}

/// Options for a single transcription pass.
#[derive(Debug, Clone, Default)]
pub struct TranscribeOpts {
    /// ISO 639-1 code ("en", "hi"); `None` = auto-detect.
    pub language: Option<String>,
    /// Bias text: personal-dictionary terms and on-screen proper nouns
    /// (design doc §2 stage 4, §7). Whisper consumes this as the decoder's
    /// initial prompt; other engines map it to hotword lists.
    pub initial_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    /// Detected (or forced) language, if the engine knows it.
    pub language: Option<String>,
}

/// A batch speech-to-text engine. `&mut` because engines keep scratch state;
/// engines are owned by the orchestrator and used from its single thread.
pub trait SttEngine: Send {
    fn transcribe(&mut self, audio: &[f32], opts: &TranscribeOpts) -> Result<Transcript, SttError>;

    /// Human-readable backend description ("whisper.cpp tiny (CPU)") for
    /// logs and the latency monitor.
    fn describe(&self) -> String;
}
