//! Synthetic Unicode keystrokes via `SendInput` (design doc §8): slower than
//! paste but works where paste is blocked, and it's the only safe method for
//! terminals — combined with the hard invariant that no newline is ever sent
//! (a pasted/typed Enter would *execute* whatever is on the prompt).

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
};

use crate::{InjectError, Injector};

/// Types text character-by-character. `terminal_safe` strips all newlines.
pub struct KeystrokeInjector {
    pub terminal_safe: bool,
}

impl KeystrokeInjector {
    pub fn new(terminal_safe: bool) -> Self {
        Self { terminal_safe }
    }
}

/// SAFETY INVARIANT (design doc §9/§22): text destined for a terminal must
/// never contain a newline — Enter executes commands. Newlines collapse to
/// single spaces.
pub fn sanitize_for_terminal(text: &str) -> String {
    // Preserve a trailing space only if the *original* text ended with one —
    // a stripped trailing newline must not become a space.
    let trailing = text.ends_with(' ');
    let replaced = text.replace("\r\n", " ").replace(['\n', '\r'], " ");
    let mut out = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if trailing && !out.is_empty() {
        out.push(' ');
    }
    out
}

impl Injector for KeystrokeInjector {
    fn inject(&mut self, text: &str) -> Result<(), InjectError> {
        let sanitized;
        let text = if self.terminal_safe {
            sanitized = sanitize_for_terminal(text);
            sanitized.as_str()
        } else {
            text
        };

        let units: Vec<u16> = text.encode_utf16().collect();
        // Batch in modest chunks; each UTF-16 unit gets a down+up pair.
        // Windows reassembles surrogate pairs from consecutive UNICODE events.
        for chunk in units.chunks(32) {
            let mut inputs = Vec::with_capacity(chunk.len() * 2);
            for &unit in chunk {
                for up in [false, true] {
                    inputs.push(INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: Default::default(),
                                wScan: unit,
                                dwFlags: if up {
                                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                                } else {
                                    KEYEVENTF_UNICODE
                                },
                                time: 0,
                                dwExtraInfo: 0,
                            },
                        },
                    });
                }
            }
            let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
            if sent != inputs.len() as u32 {
                return Err(InjectError::Synthesis(format!(
                    "SendInput typed {sent}/{} events",
                    inputs.len()
                )));
            }
            // Tiny pacing gap: some apps drop rapid unicode bursts (§8).
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_sanitizer_removes_every_newline_kind() {
        let text = "echo hello\r\nrm -rf /\ndone\r";
        let out = sanitize_for_terminal(text);
        assert!(
            !out.contains('\n') && !out.contains('\r'),
            "newline survived: {out:?}"
        );
        assert_eq!(out, "echo hello rm -rf / done");
    }

    #[test]
    fn preserves_single_trailing_space() {
        assert_eq!(sanitize_for_terminal("ls -la "), "ls -la ");
        assert_eq!(sanitize_for_terminal("ls -la"), "ls -la");
    }
}
