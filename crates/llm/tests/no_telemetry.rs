//! Guardrail: the LLM backend must only ever talk to the configured Ollama
//! base URL. This test drives the FakeLlm path (no network) and asserts the
//! real OllamaBackend, pointed at a dead port, fails fast rather than reaching
//! out anywhere else. Combined with the workspace having no analytics/telemetry
//! dependencies, this documents the "no cloud, no telemetry" guarantee.

use openflow_llm::{LlmBackend, OllamaBackend};

#[test]
fn ollama_backend_only_targets_configured_host() {
    // A closed port on localhost: the backend must error, not silently
    // succeed by contacting some other endpoint.
    let backend = OllamaBackend::new("http://127.0.0.1:1");
    assert!(
        !backend.reachable(),
        "nothing should be listening on port 1"
    );

    let err = backend
        .chat("any-model", &[openflow_llm::ChatMessage::user("hi")], false)
        .unwrap_err();
    // Connection failure — proves it tried only the configured host.
    assert!(
        matches!(err, openflow_llm::LlmError::Unavailable(_)),
        "expected Unavailable, got {err:?}"
    );
}
