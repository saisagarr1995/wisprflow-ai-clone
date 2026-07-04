use openflow_settings::LlmConfig;

/// What kind of work a request is; each kind routes to a configured model
/// (design doc §3: small resident model for the hot path, larger on demand).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// Every-dictation cleanup: must be fast.
    Cleanup,
    /// Voice-command interpretation (structured output).
    Command,
    /// Quality rewrites (tone, summarize, email…).
    Rewrite,
    /// Coding-mode assistance.
    Coding,
    /// Free-form Q&A shown in the popover.
    Assistant,
}

pub fn model_for(task: TaskKind, config: &LlmConfig) -> String {
    match task {
        TaskKind::Cleanup | TaskKind::Command => config.cleanup_model.clone(),
        TaskKind::Rewrite | TaskKind::Assistant => config.rewrite_model.clone(),
        TaskKind::Coding => config
            .coding_model
            .clone()
            .unwrap_or_else(|| config.cleanup_model.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_by_task() {
        let config = LlmConfig::default();
        assert_eq!(model_for(TaskKind::Cleanup, &config), config.cleanup_model);
        assert_eq!(model_for(TaskKind::Rewrite, &config), config.rewrite_model);
        // Coding falls back to the cleanup model when unset.
        assert_eq!(model_for(TaskKind::Coding, &config), config.cleanup_model);
        let with_coding = LlmConfig {
            coding_model: Some("qwen3-coder:30b".into()),
            ..config
        };
        assert_eq!(model_for(TaskKind::Coding, &with_coding), "qwen3-coder:30b");
    }
}
