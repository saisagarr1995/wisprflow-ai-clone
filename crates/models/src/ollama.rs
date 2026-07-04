use serde::Deserialize;

use crate::ModelError;

#[derive(Debug, Clone)]
pub struct OllamaStatus {
    /// Models present locally (as reported by /api/tags), e.g. "qwen3:4b-instruct".
    pub available: Vec<String>,
    /// Requested models that are missing and need `ollama pull`.
    pub missing: Vec<String>,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

/// Query a local Ollama for its model list and diff against `required`.
/// A missing tag is matched leniently: "qwen3:4b" counts as present if any
/// installed name starts with it (Ollama appends default tags like ":latest").
pub fn check_ollama(base_url: &str, required: &[&str]) -> Result<OllamaStatus, ModelError> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let resp = reqwest::blocking::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| ModelError::OllamaUnreachable {
            url: url.clone(),
            reason: e.to_string(),
        })?;

    let tags: TagsResponse = resp.json().map_err(|e| ModelError::OllamaUnreachable {
        url,
        reason: format!("bad response: {e}"),
    })?;

    let available: Vec<String> = tags.models.into_iter().map(|m| m.name).collect();
    let missing = required
        .iter()
        .filter(|req| {
            !available.iter().any(|have| {
                have == *req
                    || have.starts_with(&format!("{req}:"))
                    || req.starts_with(&format!("{have}:"))
            })
        })
        .map(|s| s.to_string())
        .collect();

    Ok(OllamaStatus { available, missing })
}
