//! Coding-mode casing engine (design doc §9): deterministic conversion of
//! spoken phrases into identifiers, paths, symbols, and CLI flags. Runs only
//! when the Coding/Terminal profile is active, before any LLM involvement.

/// Casing styles reachable by voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStyle {
    Camel,
    Pascal,
    Snake,
    Kebab,
    ScreamingSnake,
    Dot,
}

/// Detect a trailing or leading casing command and apply it to the rest.
/// "get user by id camel case" / "camel case get user by id" → `getUserById`.
/// Returns `None` when the utterance isn't a casing command.
pub fn apply_casing_command(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    if words.len() < 3 {
        return None;
    }

    // Longest suffixes first: "screaming snake case" must win over "snake case".
    const COMMANDS: &[(&[&str], CaseStyle)] = &[
        (&["screaming", "snake", "case"], CaseStyle::ScreamingSnake),
        (&["camel", "case"], CaseStyle::Camel),
        (&["pascal", "case"], CaseStyle::Pascal),
        (&["snake", "case"], CaseStyle::Snake),
        (&["kebab", "case"], CaseStyle::Kebab),
        (&["dot", "case"], CaseStyle::Dot),
    ];

    for (suffix, style) in COMMANDS {
        if words.len() > suffix.len() && words.ends_with(suffix) {
            let phrase = &words[..words.len() - suffix.len()];
            return Some(apply_case(phrase, *style));
        }
        if words.len() > suffix.len() && words.starts_with(suffix) {
            let phrase = &words[suffix.len()..];
            return Some(apply_case(phrase, *style));
        }
    }
    None
}

pub fn apply_case(words: &[&str], style: CaseStyle) -> String {
    let clean: Vec<String> = words
        .iter()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();
    match style {
        CaseStyle::Camel => {
            let mut out = String::new();
            for (i, w) in clean.iter().enumerate() {
                if i == 0 {
                    out.push_str(w);
                } else {
                    out.push_str(&capitalize(w));
                }
            }
            out
        }
        CaseStyle::Pascal => clean.iter().map(|w| capitalize(w)).collect(),
        CaseStyle::Snake => clean.join("_"),
        CaseStyle::Kebab => clean.join("-"),
        CaseStyle::ScreamingSnake => clean.join("_").to_uppercase(),
        CaseStyle::Dot => clean.join("."),
    }
}

fn capitalize(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Spoken separators and symbols → characters. Applied in Coding/Terminal
/// profiles: "src slash components slash button dot tsx" →
/// `src/components/button.tsx`; "git commit dash m" → `git commit -m`.
pub fn apply_coding_grammar(text: &str) -> String {
    // Multi-word symbols first (longest match wins).
    const MULTI: &[(&[&str], &str)] = &[
        (&["dash", "dash"], "--"),
        (&["fat", "arrow"], "=>"),
        (&["triple", "equals"], "==="),
        (&["double", "equals"], "=="),
        (&["not", "equals"], "!="),
        (&["and", "and"], "&&"),
        (&["or", "or"], "||"),
        (&["open", "brace"], "{"),
        (&["close", "brace"], "}"),
        (&["open", "bracket"], "["),
        (&["close", "bracket"], "]"),
        (&["open", "paren"], "("),
        (&["close", "paren"], ")"),
    ];
    // Joiners glue their neighbors: "button dot tsx" → "button.tsx".
    // Note: "dash" is NOT here — in shells it means a flag ("commit dash m" →
    // "commit -m"), handled separately below. Use "kebab case" for hyphenation.
    const JOINERS: &[(&str, &str)] = &[
        ("slash", "/"),
        ("dot", "."),
        ("underscore", "_"),
        ("colon", ":"),
    ];
    // Standalone symbols keep surrounding spaces: "a pipe b" → "a | b".
    const STANDALONE: &[(&str, &str)] = &[
        ("pipe", "|"),
        ("ampersand", "&"),
        ("backtick", "`"),
        ("semicolon", ";"),
        ("arrow", "->"),
        ("equals", "="),
        ("plus", "+"),
        ("asterisk", "*"),
        ("tilde", "~"),
        ("at-sign", "@"),
    ];

    let words: Vec<String> = text.split_whitespace().map(|w| w.to_string()).collect();
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    'outer: while i < words.len() {
        let lower = words[i].to_lowercase();
        let bare = lower.trim_matches(|c: char| !c.is_alphanumeric());

        // Multi-word sequences.
        for (pattern, replacement) in MULTI {
            if i + pattern.len() <= words.len()
                && pattern.iter().enumerate().all(|(k, p)| {
                    words[i + k]
                        .to_lowercase()
                        .trim_matches(|c: char| !c.is_alphanumeric())
                        == *p
                })
            {
                // "dash dash verbose" glues onto the next word.
                if *replacement == "--" && i + pattern.len() < words.len() {
                    let next = words[i + pattern.len()].to_lowercase();
                    out.push(format!("--{next}"));
                    i += pattern.len() + 1;
                } else {
                    out.push(replacement.to_string());
                    i += pattern.len();
                }
                continue 'outer;
            }
        }
        // "dash x" → "-x" as its own token (a shell flag).
        if bare == "dash" {
            if let Some(next) = words.get(i + 1) {
                out.push(format!("-{}", next.to_lowercase()));
                i += 2;
                continue;
            }
        }
        // Joiners: merge previous + separator + next into one token.
        if let Some((_, sep)) = JOINERS.iter().find(|(name, _)| bare == *name) {
            if let (Some(prev), Some(next)) = (out.last().cloned(), words.get(i + 1)) {
                let merged = format!("{prev}{sep}{}", next.to_lowercase());
                *out.last_mut().expect("checked above") = merged;
                i += 2;
                continue;
            }
        }
        if let Some((_, sym)) = STANDALONE.iter().find(|(name, _)| bare == *name) {
            out.push(sym.to_string());
            i += 1;
            continue;
        }
        out.push(words[i].clone());
        i += 1;
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casing_commands_table() {
        let cases = [
            ("get user by id camel case", "getUserById"),
            ("get user by id, camel case", "getUserById"),
            ("camel case get user by id", "getUserById"),
            ("fetch users pascal case", "FetchUsers"),
            ("user account id snake case", "user_account_id"),
            ("primary button style kebab case", "primary-button-style"),
            ("max retry count screaming snake case", "MAX_RETRY_COUNT"),
            ("config server port dot case", "config.server.port"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                apply_casing_command(input).as_deref(),
                Some(expected),
                "input: {input}"
            );
        }
    }

    #[test]
    fn non_commands_pass_through() {
        assert_eq!(apply_casing_command("just a normal sentence"), None);
        assert_eq!(apply_casing_command("camel case"), None); // no phrase
        assert_eq!(apply_casing_command("in any case we proceed"), None);
    }

    #[test]
    fn paths_from_spoken_separators() {
        assert_eq!(
            apply_coding_grammar("src slash components slash button dot tsx"),
            "src/components/button.tsx"
        );
        assert_eq!(apply_coding_grammar("main dot rs"), "main.rs");
        assert_eq!(
            apply_coding_grammar("my file underscore name dot py"),
            "my file_name.py"
        );
    }

    #[test]
    fn terminal_flags_and_operators() {
        assert_eq!(apply_coding_grammar("git commit dash m"), "git commit -m");
        assert_eq!(
            apply_coding_grammar("cargo test dash dash verbose"),
            "cargo test --verbose"
        );
        assert_eq!(apply_coding_grammar("ls pipe grep foo"), "ls | grep foo");
        assert_eq!(
            apply_coding_grammar("make build and and make test"),
            "make build && make test"
        );
    }

    #[test]
    fn symbols() {
        assert_eq!(apply_coding_grammar("a fat arrow b"), "a => b");
        assert_eq!(apply_coding_grammar("x double equals y"), "x == y");
        assert_eq!(apply_coding_grammar("open brace close brace"), "{ }");
        assert_eq!(apply_coding_grammar("a arrow b"), "a -> b");
    }

    #[test]
    fn ordinary_words_untouched() {
        assert_eq!(
            apply_coding_grammar("please update the readme file"),
            "please update the readme file"
        );
    }
}
