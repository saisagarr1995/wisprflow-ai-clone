use std::collections::HashSet;

use crate::parse::Chord;
use crate::HotkeyEvent;

/// Pure chord state machine: feed every key transition on the system, get
/// `Pressed` when the chord goes fully down and `Released` when it breaks.
/// OS autorepeat (repeated key-downs while held) is ignored by construction.
pub struct ChordTracker {
    chord: Chord,
    down: HashSet<u32>,
    active: bool,
}

impl ChordTracker {
    pub fn new(chord: Chord) -> Self {
        Self { chord, down: HashSet::new(), active: false }
    }

    /// `pressed=true` for key-down, `false` for key-up.
    pub fn on_key(&mut self, vk_code: u32, pressed: bool) -> Option<HotkeyEvent> {
        if pressed {
            // Autorepeat: already-down key changes nothing.
            if !self.down.insert(vk_code) {
                return None;
            }
            if !self.active && self.chord_satisfied() {
                self.active = true;
                return Some(HotkeyEvent::Pressed);
            }
        } else {
            self.down.remove(&vk_code);
            if self.active && !self.chord_satisfied() {
                self.active = false;
                return Some(HotkeyEvent::Released);
            }
        }
        None
    }

    fn chord_satisfied(&self) -> bool {
        self.chord.keys.iter().all(|m| self.down.iter().any(|&k| m.matches(k)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse_chord, vk};

    fn tracker(s: &str) -> ChordTracker {
        ChordTracker::new(parse_chord(s).unwrap())
    }

    #[test]
    fn modifier_only_hold_and_release() {
        let mut t = tracker("RCtrl");
        assert_eq!(t.on_key(vk::RCTRL, true), Some(HotkeyEvent::Pressed));
        assert_eq!(t.on_key(vk::RCTRL, false), Some(HotkeyEvent::Released));
    }

    #[test]
    fn autorepeat_fires_pressed_once() {
        let mut t = tracker("RCtrl");
        assert_eq!(t.on_key(vk::RCTRL, true), Some(HotkeyEvent::Pressed));
        for _ in 0..20 {
            assert_eq!(t.on_key(vk::RCTRL, true), None); // autorepeat spam
        }
        assert_eq!(t.on_key(vk::RCTRL, false), Some(HotkeyEvent::Released));
    }

    #[test]
    fn multi_key_chord_requires_all_and_breaks_on_any_release() {
        let mut t = tracker("Ctrl+Shift+Space");
        assert_eq!(t.on_key(vk::LCTRL, true), None);
        assert_eq!(t.on_key(vk::LSHIFT, true), None);
        assert_eq!(t.on_key(vk::SPACE, true), Some(HotkeyEvent::Pressed));
        // Releasing any chord member breaks it.
        assert_eq!(t.on_key(vk::LSHIFT, false), Some(HotkeyEvent::Released));
        // Releasing the rest emits nothing further.
        assert_eq!(t.on_key(vk::SPACE, false), None);
        assert_eq!(t.on_key(vk::LCTRL, false), None);
    }

    #[test]
    fn generic_modifier_matches_either_side() {
        let mut t = tracker("Ctrl+D");
        assert_eq!(t.on_key(vk::RCTRL, true), None);
        assert_eq!(t.on_key(b'D' as u32, true), Some(HotkeyEvent::Pressed));
        assert_eq!(t.on_key(vk::RCTRL, false), Some(HotkeyEvent::Released));
    }

    #[test]
    fn unrelated_keys_do_not_disturb_chord() {
        let mut t = tracker("RCtrl");
        assert_eq!(t.on_key(b'A' as u32, true), None);
        assert_eq!(t.on_key(vk::RCTRL, true), Some(HotkeyEvent::Pressed));
        assert_eq!(t.on_key(b'A' as u32, false), None);
        assert_eq!(t.on_key(b'B' as u32, true), None);
        assert_eq!(t.on_key(vk::RCTRL, false), Some(HotkeyEvent::Released));
    }

    #[test]
    fn no_pressed_while_already_active_via_extra_keys() {
        let mut t = tracker("Ctrl+D");
        t.on_key(vk::LCTRL, true);
        assert_eq!(t.on_key(b'D' as u32, true), Some(HotkeyEvent::Pressed));
        // Pressing the other ctrl while active must not re-fire.
        assert_eq!(t.on_key(vk::RCTRL, true), None);
        // Releasing one ctrl keeps the chord satisfied via the other.
        assert_eq!(t.on_key(vk::LCTRL, false), None);
        assert_eq!(t.on_key(vk::RCTRL, false), Some(HotkeyEvent::Released));
    }
}
