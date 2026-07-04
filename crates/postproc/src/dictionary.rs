//! Personal-dictionary fuzzy replacement: fix ASR near-misses of the user's
//! terms ("cube control" → "kubectl") deterministically, before the LLM.

use strsim::jaro_winkler;

/// A dictionary term: the canonical `word` plus an optional spoken `hint`
/// ("kubectl", hint "cube control"). Both are matched against the transcript.
#[derive(Debug, Clone)]
pub struct Term {
    pub word: String,
    pub hint: Option<String>,
}

const SIMILARITY_THRESHOLD: f64 = 0.90;

/// Replace fuzzy matches of dictionary terms in `text`. Sliding-window match
/// over whitespace tokens; window size follows each pattern's word count.
/// Punctuation attached to window-edge tokens is preserved.
pub fn apply(text: &str, terms: &[Term]) -> String {
    if terms.is_empty() || text.is_empty() {
        return text.to_string();
    }
    let mut tokens: Vec<String> = text.split_whitespace().map(str::to_string).collect();

    for term in terms {
        // The canonical word and the hint are both spoken forms to detect.
        let mut patterns: Vec<String> = vec![term.word.to_lowercase()];
        if let Some(hint) = &term.hint {
            patterns.push(hint.to_lowercase());
        }
        for pattern in patterns {
            // Compare space-squashed forms so "post hog" (two tokens) can
            // match the single word "posthog", and vice versa.
            let squashed: String = pattern.chars().filter(|c| c.is_alphanumeric()).collect();
            if squashed.len() < 3 {
                continue; // too short to fuzzy-match safely
            }
            let word_count = pattern.split_whitespace().count().max(1);
            for window_len in 1..=word_count + 1 {
                if tokens.len() < window_len {
                    break;
                }
                let mut i = 0;
                while i + window_len <= tokens.len() {
                    let candidate_squashed: String = tokens[i..i + window_len]
                        .iter()
                        .flat_map(|t| t.chars())
                        .filter(|c| c.is_alphanumeric())
                        .map(|c| c.to_ascii_lowercase())
                        .collect();
                    // Cheap pre-filter: Jaro-Winkler's prefix bonus makes
                    // "post" ≈ "posthog" and "kubectlapply" ≈ "kubectl" score
                    // high, so require comparable lengths before scoring.
                    let (lo, hi) = {
                        let (a, b) = (candidate_squashed.len(), squashed.len());
                        if a < b {
                            (a, b)
                        } else {
                            (b, a)
                        }
                    };
                    if lo < 3 || (lo as f64) / (hi as f64) < 0.8 {
                        i += 1;
                        continue;
                    }
                    let (candidate, prefix, suffix) = normalize_window(&tokens[i..i + window_len]);
                    if candidate != term.word
                        && jaro_winkler(&candidate_squashed, &squashed) >= SIMILARITY_THRESHOLD
                    {
                        let replacement = format!("{prefix}{}{suffix}", term.word);
                        tokens.splice(i..i + window_len, [replacement]);
                    }
                    i += 1;
                }
            }
        }
    }
    tokens.join(" ")
}

/// Join a token window, splitting off leading/trailing punctuation so the
/// replacement can keep it ("(cube control)," → prefix "(", suffix ",").
fn normalize_window(window: &[String]) -> (String, String, String) {
    let joined = window.join(" ");
    let start = joined.find(|c: char| c.is_alphanumeric()).unwrap_or(0);
    let end = joined
        .rfind(|c: char| c.is_alphanumeric())
        .map(|i| i + joined[i..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(joined.len());
    (
        joined[start..end].to_string(),
        joined[..start].to_string(),
        joined[end..].to_string(),
    )
}

/// The bias text passed to the STT engine's initial prompt.
pub fn bias_prompt(terms: &[Term]) -> Option<String> {
    if terms.is_empty() {
        return None;
    }
    Some(
        terms
            .iter()
            .map(|t| t.word.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(word: &str, hint: Option<&str>) -> Term {
        Term {
            word: word.into(),
            hint: hint.map(String::from),
        }
    }

    #[test]
    fn replaces_hint_phrase_with_canonical_word() {
        let terms = [term("kubectl", Some("cube control"))];
        assert_eq!(
            apply("run cube control apply now", &terms),
            "run kubectl apply now"
        );
    }

    #[test]
    fn replaces_near_miss_of_the_word_itself() {
        let terms = [term("PostHog", None)];
        assert_eq!(
            apply("we track it in post hog today", &terms),
            "we track it in PostHog today"
        );
        assert_eq!(
            apply("we track it in posthog today", &terms),
            "we track it in PostHog today"
        );
    }

    #[test]
    fn preserves_punctuation_around_replacement() {
        let terms = [term("kubectl", Some("cube control"))];
        assert_eq!(
            apply("then run cube control, please", &terms),
            "then run kubectl, please"
        );
    }

    #[test]
    fn leaves_dissimilar_text_alone() {
        let terms = [term("kubectl", Some("cube control"))];
        let text = "the cube is under control here";
        // "cube is" / "is under" etc. must not match "cube control".
        assert_eq!(apply(text, &terms), text);
    }

    #[test]
    fn already_canonical_is_untouched() {
        let terms = [term("kubectl", None)];
        assert_eq!(
            apply("kubectl apply -f x.yaml", &terms),
            "kubectl apply -f x.yaml"
        );
    }

    #[test]
    fn bias_prompt_joins_words() {
        let terms = [term("kubectl", None), term("PostHog", None)];
        assert_eq!(bias_prompt(&terms).unwrap(), "kubectl, PostHog");
        assert!(bias_prompt(&[]).is_none());
    }

    #[test]
    fn is_fast_enough_for_large_dictionaries() {
        let terms: Vec<Term> = (0..500)
            .map(|i| term(&format!("uniqueterm{i}"), None))
            .collect();
        let text = "this is a perfectly ordinary sentence with a dozen or so words in it today";
        let started = std::time::Instant::now();
        let _ = apply(text, &terms);
        // Generous bound for debug builds; release is ~10x faster. The point
        // is to catch accidental O(n²) blowups, not to benchmark precisely.
        assert!(
            started.elapsed().as_millis() < 250,
            "500-term dictionary took {:?}",
            started.elapsed()
        );
    }
}
