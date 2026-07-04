/// Windows virtual-key codes (used as the canonical key id on all platforms;
/// other platforms translate their native codes into this space).
pub mod vk {
    pub const LSHIFT: u32 = 0xA0;
    pub const RSHIFT: u32 = 0xA1;
    pub const LCTRL: u32 = 0xA2;
    pub const RCTRL: u32 = 0xA3;
    pub const LALT: u32 = 0xA4;
    pub const RALT: u32 = 0xA5;
    pub const LWIN: u32 = 0x5B;
    pub const RWIN: u32 = 0x5C;
    pub const SPACE: u32 = 0x20;
    pub const F1: u32 = 0x70; // F1..F24 are contiguous
}

/// One slot in a chord: an exact key, or "either side" of a modifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyMatcher {
    Exact(u32),
    AnyOf(&'static [u32]),
}

impl KeyMatcher {
    pub fn matches(&self, vk_code: u32) -> bool {
        match self {
            KeyMatcher::Exact(k) => *k == vk_code,
            KeyMatcher::AnyOf(ks) => ks.contains(&vk_code),
        }
    }
}

/// A parsed hotkey chord. All listed keys must be down simultaneously;
/// modifier-only chords (e.g. just `RCtrl`) are first-class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub keys: Vec<KeyMatcher>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("unknown key name '{0}'")]
    UnknownKey(String),
    #[error("empty chord")]
    Empty,
}

/// Parse strings like "RCtrl", "Ctrl+Shift+Space", "F9", "Alt+Z".
/// Case-insensitive; `+` separates keys.
pub fn parse_chord(s: &str) -> Result<Chord, ParseError> {
    let mut keys = Vec::new();
    for token in s.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        keys.push(parse_key(token)?);
    }
    if keys.is_empty() {
        return Err(ParseError::Empty);
    }
    Ok(Chord { keys })
}

fn parse_key(token: &str) -> Result<KeyMatcher, ParseError> {
    use vk::*;
    static CTRL_ANY: &[u32] = &[LCTRL, RCTRL];
    static SHIFT_ANY: &[u32] = &[LSHIFT, RSHIFT];
    static ALT_ANY: &[u32] = &[LALT, RALT];
    static WIN_ANY: &[u32] = &[LWIN, RWIN];

    let t = token.to_ascii_lowercase();
    let m = match t.as_str() {
        "ctrl" | "control" => KeyMatcher::AnyOf(CTRL_ANY),
        "lctrl" => KeyMatcher::Exact(LCTRL),
        "rctrl" => KeyMatcher::Exact(RCTRL),
        "shift" => KeyMatcher::AnyOf(SHIFT_ANY),
        "lshift" => KeyMatcher::Exact(LSHIFT),
        "rshift" => KeyMatcher::Exact(RSHIFT),
        "alt" => KeyMatcher::AnyOf(ALT_ANY),
        "lalt" => KeyMatcher::Exact(LALT),
        "ralt" | "altgr" => KeyMatcher::Exact(RALT),
        "win" | "super" | "meta" | "cmd" => KeyMatcher::AnyOf(WIN_ANY),
        "space" => KeyMatcher::Exact(SPACE),
        _ => {
            // F1..F24
            if let Some(n) = t.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
                if (1..=24).contains(&n) {
                    return Ok(KeyMatcher::Exact(F1 + n - 1));
                }
            }
            // Single letter or digit: VK codes match ASCII uppercase.
            let mut chars = t.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if c.is_ascii_alphanumeric() {
                    return Ok(KeyMatcher::Exact(c.to_ascii_uppercase() as u32));
                }
            }
            return Err(ParseError::UnknownKey(token.to_string()));
        }
    };
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_chords() {
        assert_eq!(
            parse_chord("RCtrl").unwrap().keys,
            vec![KeyMatcher::Exact(vk::RCTRL)]
        );
        assert_eq!(
            parse_chord("Ctrl+Shift+Space").unwrap().keys,
            vec![
                KeyMatcher::AnyOf(&[vk::LCTRL, vk::RCTRL]),
                KeyMatcher::AnyOf(&[vk::LSHIFT, vk::RSHIFT]),
                KeyMatcher::Exact(vk::SPACE),
            ]
        );
        assert_eq!(
            parse_chord("F9").unwrap().keys,
            vec![KeyMatcher::Exact(0x78)]
        );
        assert_eq!(parse_chord("Alt+Z").unwrap().keys.len(), 2);
        assert_eq!(parse_chord("alt + z").unwrap().keys.len(), 2);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            parse_chord("Bogus").unwrap_err(),
            ParseError::UnknownKey("Bogus".into())
        );
        assert_eq!(parse_chord("").unwrap_err(), ParseError::Empty);
        assert_eq!(
            parse_chord("F25").unwrap_err(),
            ParseError::UnknownKey("F25".into())
        );
    }
}
