//! Output guard (design doc §2 stage 8): LLMs are "phenomenal at recall, very
//! low precision" — so every cleanup response is sanitized and, if it smells
//! wrong, rejected in favor of the deterministic transcript.

#[derive(Debug, PartialEq, Eq)]
pub enum GuardReject {
    /// Output length wildly off vs. input (>2.5× or <0.3×).
    LengthRatio,
    /// Output still contains meta-chatter after stripping.
    MetaText,
    /// Output empty after sanitizing.
    Empty,
}

const META_PREFIXES: &[&str] = &[
    "here is",
    "here's",
    "sure,",
    "sure!",
    "certainly",
    "cleaned text:",
    "cleaned:",
    "output:",
    "the cleaned",
    "as an ai",
    "i'm sorry",
    "i cannot",
];

/// Strip wrappers the model loves to add, then validate. `Ok` is the text to
/// inject; `Err` means fall back to the original transcript.
pub fn sanitize(llm_output: &str, original: &str) -> Result<String, GuardReject> {
    // Strip <think> blocks (reasoning models), then code fences.
    let mut owned = llm_output.trim().to_string();
    if let (Some(start), Some(end)) = (owned.find("<think>"), owned.find("</think>")) {
        if start < end {
            owned = format!("{}{}", &owned[..start], &owned[end + "</think>".len()..]);
        }
    }
    let text = owned
        .trim()
        .trim_start_matches("```text")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    // Strip surrounding quotes the model sometimes adds around the whole text.
    let text = if text.len() > 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\u{201c}') && text.ends_with('\u{201d}')))
    {
        &text[1..text.len() - 1]
    } else {
        text
    };

    // Drop a leading meta line ("Here is the cleaned text:") if one slipped in.
    let text = match text.split_once('\n') {
        Some((first, rest))
            if META_PREFIXES
                .iter()
                .any(|p| first.trim().to_lowercase().starts_with(p)) =>
        {
            rest.trim()
        }
        _ => text,
    };

    if text.is_empty() {
        return Err(GuardReject::Empty);
    }
    let lower = text.to_lowercase();
    if META_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return Err(GuardReject::MetaText);
    }

    let in_len = original.chars().count().max(1);
    let out_len = text.chars().count();
    // Short utterances legitimately shrink a lot ("um, yes" → "Yes."), so the
    // lower bound only applies once there's enough input to mean something.
    if out_len > in_len * 5 / 2 || (in_len > 40 && out_len < in_len * 3 / 10) {
        return Err(GuardReject::LengthRatio);
    }

    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_clean_output_through() {
        assert_eq!(
            sanitize(
                "We should ship on Monday.",
                "um we should ship on friday no wait monday"
            ),
            Ok("We should ship on Monday.".into())
        );
    }

    #[test]
    fn strips_fences_quotes_and_think_blocks() {
        assert_eq!(sanitize("```\nHello.\n```", "hello"), Ok("Hello.".into()));
        assert_eq!(
            sanitize("\"Hello there.\"", "hello there"),
            Ok("Hello there.".into())
        );
        assert_eq!(
            sanitize("<think>user wants cleanup</think>Hello.", "hello"),
            Ok("Hello.".into())
        );
    }

    #[test]
    fn strips_leading_meta_line_but_rejects_pure_meta() {
        assert_eq!(
            sanitize(
                "Here is the cleaned text:\nShip it Monday.",
                "um ship it monday"
            ),
            Ok("Ship it Monday.".into())
        );
        assert_eq!(
            sanitize(
                "I'm sorry, I cannot help with that.",
                "hello world how are you"
            ),
            Err(GuardReject::MetaText)
        );
    }

    #[test]
    fn rejects_wild_length_changes() {
        let original = "please write down the number four";
        let bloated = "four ".repeat(60);
        assert_eq!(sanitize(&bloated, original), Err(GuardReject::LengthRatio));

        let long_original =
            "this is a fairly long sentence with plenty of words that should survive cleanup mostly intact";
        assert_eq!(sanitize("ok", long_original), Err(GuardReject::LengthRatio));
    }

    #[test]
    fn allows_big_shrink_on_short_utterances() {
        assert_eq!(sanitize("Yes.", "um uh yes"), Ok("Yes.".into()));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(sanitize("   ", "hello"), Err(GuardReject::Empty));
    }
}
