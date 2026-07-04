use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("llm unavailable: {0}")]
    Unavailable(String),
    #[error("llm request failed: {0}")]
    Request(String),
    #[error("llm returned malformed response: {0}")]
    BadResponse(String),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatMessage {
    pub role: &'static str, // "system" | "user" | "assistant"
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system",
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user",
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant",
            content: content.into(),
        }
    }
}

/// Blocking chat completion. Called from the orchestrator thread (dictation)
/// or worker threads (commands); short outputs make blocking acceptable and
/// keep the state machine simple.
pub trait LlmBackend: Send + Sync {
    /// `json` requests strict JSON output (Ollama `format: "json"`).
    fn chat(&self, model: &str, messages: &[ChatMessage], json: bool) -> Result<String, LlmError>;

    /// Load the model into memory so the first real call is fast.
    fn prewarm(&self, model: &str);
}

/// Scripted backend for tests.
#[derive(Clone, Default)]
pub struct FakeLlm {
    responses: Arc<Mutex<VecDeque<Result<String, String>>>>,
    pub calls: Arc<Mutex<Vec<(String, Vec<ChatMessage>)>>>,
}

impl FakeLlm {
    pub fn with_responses<S: Into<String>>(responses: impl IntoIterator<Item = S>) -> Self {
        let fake = Self::default();
        fake.responses
            .lock()
            .unwrap()
            .extend(responses.into_iter().map(|s| Ok(s.into())));
        fake
    }

    pub fn failing(message: &str) -> Self {
        let fake = Self::default();
        fake.responses
            .lock()
            .unwrap()
            .push_back(Err(message.to_string()));
        fake
    }
}

impl LlmBackend for FakeLlm {
    fn chat(&self, model: &str, messages: &[ChatMessage], _json: bool) -> Result<String, LlmError> {
        self.calls
            .lock()
            .unwrap()
            .push((model.to_string(), messages.to_vec()));
        match self.responses.lock().unwrap().pop_front() {
            Some(Ok(text)) => Ok(text),
            Some(Err(e)) => Err(LlmError::Request(e)),
            None => Ok("ok".into()),
        }
    }

    fn prewarm(&self, _model: &str) {}
}
