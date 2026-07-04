//! Context awareness (design doc §7): who has focus, what kind of app it is,
//! what tone the cleanup LLM should use, and (best-effort) nearby text via
//! platform accessibility APIs.

mod profiles;
#[cfg(windows)]
mod win_focus;
#[cfg(windows)]
mod win_uia;

pub use profiles::{classify, tone_for, AppProfile};
#[cfg(windows)]
pub use win_focus::WindowsContext;

/// Identity of the focused application at dictation start.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppInfo {
    /// Lowercased executable name, e.g. "slack.exe".
    pub process: String,
    /// Foreground window title.
    pub title: String,
}

/// Text captured from the focused control (design doc §7 layer 3).
#[derive(Debug, Clone, Default)]
pub struct TextContext {
    /// Up to ~200 chars before the caret / end of the field.
    pub preceding_text: Option<String>,
    /// Currently selected text, if any (operand for voice commands).
    pub selection: Option<String>,
}

/// Platform focus provider; `Fake` for tests and unsupported platforms.
pub trait ContextProvider: Send {
    fn focused_app(&self) -> Option<AppInfo>;
    /// Best-effort; must return quickly (callers give it ~150 ms).
    fn text_context(&self) -> TextContext;
}

/// Test/unsupported-platform provider.
#[derive(Default, Clone)]
pub struct FakeContext {
    pub app: Option<AppInfo>,
    pub text: TextContext,
}

impl ContextProvider for FakeContext {
    fn focused_app(&self) -> Option<AppInfo> {
        self.app.clone()
    }
    fn text_context(&self) -> TextContext {
        self.text.clone()
    }
}

/// Extract likely proper nouns / identifiers from on-screen text to bias the
/// recognizer (design doc §7): capitalized words and camelCase or snake_case
/// identifiers, deduplicated, capped.
pub fn extract_entities(text: &str, cap: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        if s.len() >= 3 && !out.iter().any(|e| e == s) && out.len() < cap {
            out.push(s.to_string());
        }
    };

    for word in text.split(|c: char| c.is_whitespace() || ",.;:!?()[]{}\"'".contains(c)) {
        if word.len() < 3 {
            continue;
        }
        let has_upper = word.chars().any(|c| c.is_uppercase());
        let has_lower = word.chars().any(|c| c.is_lowercase());
        let starts_upper = word.chars().next().is_some_and(|c| c.is_uppercase());
        let internal_upper = word.chars().skip(1).any(|c| c.is_uppercase());
        // camelCase / PascalCase with internal capital, or snake_case ids.
        if (has_upper && has_lower && internal_upper) || word.contains('_') {
            push(word);
        } else if starts_upper && has_lower {
            // Plain capitalized word: likely a name.
            push(word);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_identifiers_and_names() {
        let text = "Tell fetchUsers to email Dr. Nayarti about the user_id column in PostHog.";
        let entities = extract_entities(text, 10);
        assert!(entities.contains(&"fetchUsers".to_string()));
        assert!(entities.contains(&"user_id".to_string()));
        assert!(entities.contains(&"PostHog".to_string()));
        assert!(entities.contains(&"Nayarti".to_string()));
        assert!(entities.len() <= 10);
    }

    #[test]
    fn dedupes_and_caps() {
        let text = "PostHog PostHog PostHog AlphaBeta GammaDelta EpsilonZeta";
        let entities = extract_entities(text, 2);
        assert_eq!(entities.len(), 2);
    }
}
