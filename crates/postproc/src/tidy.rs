//! Deterministic transcript tidying (design doc §2 stage 5): the fast,
//! always-on subset of what the LLM cleanup does — stutter/filler removal,
//! sentence capitalization, and terminal punctuation. Costs microseconds and
//! no RAM, so dictation reads Wispr-clean even when Ollama is off or the
//! machine can't afford an LLM pass.
//!
//! Applied to prose dictation only; coding/terminal profiles stay literal.

/// Fillers dropped wherever they appear (matched on the token with
/// punctuation stripped, case-insensitive). Deliberately conservative:
/// only non-words, never possibly-intentional words like "well" or "like".
const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "erm", "ehm"];

/// Question openers for the terminal-punctuation heuristic.
const QUESTION_STARTS: &[&str] = &[
    "who",
    "what",
    "when",
    "where",
    "why",
    "how",
    "which",
    "whose",
    "is",
    "are",
    "was",
    "were",
    "am",
    "do",
    "does",
    "did",
    "can",
    "could",
    "will",
    "would",
    "should",
    "shall",
    "have",
    "has",
    "had",
    "isn't",
    "aren't",
    "don't",
    "doesn't",
    "didn't",
    "won't",
    "can't",
    "couldn't",
    "wouldn't",
    "shouldn't",
];

/// Tidy a final prose transcript: drop fillers, collapse immediate word and
/// bigram repeats ("the the", "I think I think"), capitalize sentence starts
/// and the pronoun "I", and close the text with "." or "?" (question chosen
/// by opener heuristic) if whisper left it unpunctuated.
pub fn tidy_transcript(text: &str) -> String {
    let mut tokens: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    if tokens.is_empty() {
        return String::new();
    }

    tokens = drop_fillers(tokens);
    tokens = collapse_repeats(tokens);
    if tokens.is_empty() {
        return String::new();
    }

    fix_pronoun_i(&mut tokens);
    terminal_punctuation(&mut tokens);
    capitalize_sentences(&mut tokens);

    tokens.join(" ")
}

/// Token with leading/trailing punctuation stripped, lowercased — the
/// identity used for repeat/filler comparison.
fn core(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
        .to_lowercase()
}

fn is_terminal(token: &str) -> bool {
    token.ends_with(['.', '!', '?', '…', ':', ';'])
}

fn drop_fillers(tokens: Vec<String>) -> Vec<String> {
    let kept: Vec<String> = tokens
        .iter()
        .filter(|t| !FILLERS.contains(&core(t).as_str()))
        .cloned()
        .collect();
    // Never tidy the whole utterance away — "Um." alone stays as spoken.
    if kept.is_empty() {
        tokens
    } else {
        kept
    }
}

/// Collapse immediate repeats, longest phrase first (up to 3 words), so
/// "I think I think we" and "no no no" both come out clean. The *later*
/// occurrence is kept: it carries the punctuation that continues the
/// sentence. Never collapses across a sentence boundary ("Yes. Yes." is two
/// deliberate sentences, not a stutter).
fn collapse_repeats(tokens: Vec<String>) -> Vec<String> {
    let is_repeat_at = |i: usize, n: usize| {
        i + 2 * n <= tokens.len()
            && tokens[i..i + n]
                .iter()
                .zip(&tokens[i + n..i + 2 * n])
                .all(|(a, b)| core(a) == core(b) && !core(a).is_empty())
            // A terminal mark inside the first copy means a real sentence
            // end ("Yes. Yes."), not a stutter.
            && !tokens[i..i + n].iter().any(|t| is_terminal(t))
    };
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        // Drop the first copy and re-examine from the second, so triple+
        // repeats keep collapsing.
        match (1..=3usize).rev().find(|&n| is_repeat_at(i, n)) {
            Some(n) => i += n,
            None => {
                out.push(tokens[i].clone());
                i += 1;
            }
        }
    }
    out
}

fn fix_pronoun_i(tokens: &mut [String]) {
    for t in tokens.iter_mut() {
        let c = core(t);
        if c == "i" || c == "i'm" || c == "i'll" || c == "i've" || c == "i'd" {
            if let Some(pos) = t.find(['i', 'I']) {
                t.replace_range(pos..pos + 1, "I");
            }
        }
    }
}

/// If the utterance ends without terminal punctuation (a trailing comma
/// counts as none), close it with "?" when the last sentence opens like a
/// question, otherwise ".".
fn terminal_punctuation(tokens: &mut [String]) {
    let last = tokens.len() - 1;
    if is_terminal(&tokens[last]) {
        return;
    }
    let trimmed = tokens[last].trim_end_matches([',', '-', '—']).to_string();
    if trimmed.is_empty() {
        return;
    }
    // First token of the sentence the last word belongs to.
    let sentence_start = (0..tokens.len() - 1)
        .rev()
        .find(|&i| is_terminal(&tokens[i]))
        .map_or(0, |i| i + 1);
    let mark = if QUESTION_STARTS.contains(&core(&tokens[sentence_start]).as_str()) {
        '?'
    } else {
        '.'
    };
    tokens[last] = format!("{trimmed}{mark}");
}

fn capitalize_sentences(tokens: &mut [String]) {
    let mut at_start = true;
    for t in tokens.iter_mut() {
        if at_start {
            if let Some((idx, c)) = t.char_indices().find(|(_, c)| c.is_alphabetic()) {
                let upper = c.to_uppercase().to_string();
                t.replace_range(idx..idx + c.len_utf8(), &upper);
            }
        }
        at_start = is_terminal(t);
    }
}

#[cfg(test)]
mod tests {
    use super::tidy_transcript;

    #[test]
    fn collapses_immediate_word_repeats() {
        assert_eq!(
            tidy_transcript("the the report is is ready"),
            "The report is ready."
        );
    }

    #[test]
    fn collapses_phrase_repeats_and_triples() {
        assert_eq!(
            tidy_transcript("I think I think we should go go go now"),
            "I think we should go now."
        );
    }

    #[test]
    fn keeps_deliberate_repeated_sentences() {
        assert_eq!(tidy_transcript("Yes. Yes."), "Yes. Yes.");
    }

    #[test]
    fn drops_fillers_but_never_everything() {
        assert_eq!(
            tidy_transcript("um, so the meeting uh moved to Friday"),
            "So the meeting moved to Friday."
        );
        assert_eq!(tidy_transcript("Um."), "Um.");
    }

    #[test]
    fn adds_question_mark_for_interrogative_openers() {
        assert_eq!(
            tidy_transcript("what time does the meeting start"),
            "What time does the meeting start?"
        );
        assert_eq!(
            tidy_transcript("I sent it. can you check your inbox"),
            "I sent it. Can you check your inbox?"
        );
    }

    #[test]
    fn adds_period_and_capitalizes_statements() {
        assert_eq!(
            tidy_transcript("send the report today"),
            "Send the report today."
        );
        assert_eq!(
            tidy_transcript("it works, i tested it,"),
            "It works, I tested it."
        );
    }

    #[test]
    fn capitalizes_i_forms() {
        assert_eq!(
            tidy_transcript("i think i'm ready and i'll join"),
            "I think I'm ready and I'll join."
        );
    }

    #[test]
    fn leaves_already_clean_text_alone() {
        assert_eq!(
            tidy_transcript("The quarterly numbers look good."),
            "The quarterly numbers look good."
        );
        assert_eq!(tidy_transcript(""), "");
    }
}
