use serde::Deserialize;
use serde_json::json;

use crate::backend::{ChatMessage, LlmBackend, LlmError};

/// Ollama `/api/chat` client. Non-streaming: dictation cleanup outputs are
/// short (tens of tokens), so a single response keeps the pipeline simple;
/// `keep_alive: -1` pins models in memory per design doc §11.
pub struct OllamaBackend {
    base_url: String,
    client: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

impl OllamaBackend {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
        }
    }

    pub fn reachable(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

impl LlmBackend for OllamaBackend {
    fn chat(&self, model: &str, messages: &[ChatMessage], json: bool) -> Result<String, LlmError> {
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": false,
            // Keep the model warm during an active session but let Ollama free
            // its RAM after ~10 min idle (important on memory-limited machines).
            // It reloads on the next dictation.
            "keep_alive": "10m",
            // Disable "thinking" for models that support it (qwen3, etc.):
            // on the hot path it multiplies latency for no benefit here
            // (design doc §3). Ignored by models without a thinking mode.
            "think": false,
            "options": { "temperature": 0.1 },
        });
        if json {
            body["format"] = json!("json");
        }

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    LlmError::Unavailable(e.to_string())
                } else {
                    LlmError::Request(e.to_string())
                }
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(LlmError::Request(format!("{status}: {text}")));
        }
        let parsed: ChatResponse = resp
            .json()
            .map_err(|e| LlmError::BadResponse(e.to_string()))?;
        Ok(parsed.message.content)
    }

    fn prewarm(&self, model: &str) {
        // A one-token generate loads the model; errors are non-fatal.
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": "hi"}],
            "stream": false,
            "keep_alive": "10m",
            "think": false,
            "options": { "num_predict": 1 },
        });
        let started = std::time::Instant::now();
        match self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
        {
            Ok(r) if r.status().is_success() => {
                tracing::info!(
                    model,
                    ms = started.elapsed().as_millis() as u64,
                    "llm prewarmed"
                )
            }
            Ok(r) => tracing::warn!(model, status = %r.status(), "llm prewarm rejected"),
            Err(e) => tracing::warn!(model, error = %e, "llm prewarm failed"),
        }
    }
}
