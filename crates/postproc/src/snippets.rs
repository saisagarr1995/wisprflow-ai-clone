//! Voice-triggered snippets: if the whole utterance is a registered trigger
//! phrase (optionally prefixed with "insert"/"snippet"), the expansion is
//! injected verbatim and the LLM never runs (design doc §2 stage 5).

#[derive(Debug, Clone)]
pub struct SnippetDef {
    pub trigger: String,
    pub expansion: String,
}

/// Whole-utterance match, punctuation- and case-insensitive.
pub fn match_snippet<'a>(text: &str, snippets: &'a [SnippetDef]) -> Option<&'a str> {
    let normalized = normalize(text);
    let stripped = normalized
        .strip_prefix("insert ")
        .or_else(|| normalized.strip_prefix("snippet "))
        .unwrap_or(&normalized);
    snippets
        .iter()
        .find(|s| normalize(&s.trigger) == stripped)
        .map(|s| s.expansion.as_str())
}

fn normalize(text: &str) -> String {
    // Punctuation becomes a space (so "sign-off" == "sign off"), then
    // whitespace collapses.
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snips() -> Vec<SnippetDef> {
        vec![
            SnippetDef {
                trigger: "my address".into(),
                expansion: "42 Example Lane".into(),
            },
            SnippetDef {
                trigger: "email sign-off".into(),
                expansion: "Best,\nSagar".into(),
            },
        ]
    }

    #[test]
    fn matches_whole_utterance_ignoring_case_and_punctuation() {
        assert_eq!(
            match_snippet("My Address.", &snips()),
            Some("42 Example Lane")
        );
        assert_eq!(
            match_snippet("email sign off", &snips()),
            Some("Best,\nSagar")
        );
    }

    #[test]
    fn matches_with_insert_prefix() {
        assert_eq!(
            match_snippet("insert my address", &snips()),
            Some("42 Example Lane")
        );
        assert_eq!(
            match_snippet("snippet my address", &snips()),
            Some("42 Example Lane")
        );
    }

    #[test]
    fn partial_utterances_do_not_match() {
        assert_eq!(
            match_snippet("send it to my address please", &snips()),
            None
        );
        assert_eq!(match_snippet("my", &snips()), None);
    }
}
