//! Timestamped key-tracker (evdev input layer, S8).
//!
//! The pure heart of the timing logic. The IO boundary owns the device and
//! supplies *timestamped* raw key events ([`RawKeyEvent`]); this tracks modifier
//! state, matches the two configured combos, measures each press→release
//! duration, and emits [`ButtonEvent`]s (`Down` on press, `Up{held}` on release)
//! for the [`crate::gesture`] mapper to turn into commands.
//!
//! Pure given timestamps: drive it with a fixed event sequence and the outputs
//! are deterministic, so the tap/hold/PTT/VAD timing is unit-testable without a
//! real device.

use std::time::Duration;

use crate::gesture::{Button, ButtonEvent};
use crate::key_combo::{KeyCombo, Modifiers, codes};

/// One raw key event from the device: a keycode, whether it went down (`true`)
/// or up (`false`), and the event timestamp. The timestamp need only be
/// consistent across a press/release pair — the tracker only takes differences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKeyEvent {
    pub code: u16,
    pub pressed: bool,
    pub time: Duration,
}

impl RawKeyEvent {
    pub fn down(code: u16, time: Duration) -> RawKeyEvent {
        RawKeyEvent { code, pressed: true, time }
    }
    pub fn up(code: u16, time: Duration) -> RawKeyEvent {
        RawKeyEvent { code, pressed: false, time }
    }
}

/// Tracks modifier state and per-button press timing, turning raw key events
/// into [`ButtonEvent`]s for the two configured combos.
#[derive(Debug, Clone)]
pub struct KeyTracker {
    start: KeyCombo,
    stop: KeyCombo,
    // Held modifier keys (left/right of each), so releasing one of a pair keeps
    // the modifier active while the other is still down.
    left_shift: bool,
    right_shift: bool,
    left_ctrl: bool,
    right_ctrl: bool,
    left_alt: bool,
    right_alt: bool,
    // When each button's main key went down (if currently down), to measure hold.
    start_down_at: Option<Duration>,
    stop_down_at: Option<Duration>,
}

impl KeyTracker {
    /// Build a tracker for the `start` and `stop` combos.
    pub fn new(start: KeyCombo, stop: KeyCombo) -> KeyTracker {
        KeyTracker {
            start,
            stop,
            left_shift: false,
            right_shift: false,
            left_ctrl: false,
            right_ctrl: false,
            left_alt: false,
            right_alt: false,
            start_down_at: None,
            stop_down_at: None,
        }
    }

    /// Current modifier state derived from held left/right modifier keys.
    fn modifiers(&self) -> Modifiers {
        Modifiers {
            shift: self.left_shift || self.right_shift,
            ctrl: self.left_ctrl || self.right_ctrl,
            alt: self.left_alt || self.right_alt,
        }
    }

    /// Update held-modifier state if `ev` is a modifier key; returns true if it
    /// was a modifier (and therefore not a combo trigger).
    fn track_modifier(&mut self, ev: RawKeyEvent) -> bool {
        let slot = match ev.code {
            codes::KEY_LEFTSHIFT => &mut self.left_shift,
            codes::KEY_RIGHTSHIFT => &mut self.right_shift,
            codes::KEY_LEFTCTRL => &mut self.left_ctrl,
            codes::KEY_RIGHTCTRL => &mut self.right_ctrl,
            codes::KEY_LEFTALT => &mut self.left_alt,
            codes::KEY_RIGHTALT => &mut self.right_alt,
            _ => return false,
        };
        *slot = ev.pressed;
        true
    }

    /// Feed one raw key event; returns a [`ButtonEvent`] if it opened or closed
    /// one of the configured buttons, else `None`.
    ///
    /// Press matches against the *current* modifier state (so `Shift+F10`
    /// requires Shift held at the down edge). Release is matched purely by which
    /// button's main key is currently down — the user may release Shift before
    /// the key, and the held duration is what reclassifies tap vs hold.
    pub fn feed(&mut self, ev: RawKeyEvent) -> Option<ButtonEvent> {
        if self.track_modifier(ev) {
            return None;
        }

        if ev.pressed {
            let held = self.modifiers();
            // A repeated down (key autorepeat) while already latched is ignored.
            if self.start_down_at.is_none() && self.start.matches(ev.code, held) {
                self.start_down_at = Some(ev.time);
                return Some(ButtonEvent::Down(Button::Start));
            }
            if self.stop_down_at.is_none() && self.stop.matches(ev.code, held) {
                self.stop_down_at = Some(ev.time);
                return Some(ButtonEvent::Down(Button::Stop));
            }
            None
        } else {
            // Release: resolve whichever button this key opened.
            if let Some(down) = self.start_down_at
                && ev.code == self.start.key
            {
                self.start_down_at = None;
                return Some(ButtonEvent::Up {
                    button: Button::Start,
                    held: ev.time.saturating_sub(down),
                });
            }
            if let Some(down) = self.stop_down_at
                && ev.code == self.stop.key
            {
                self.stop_down_at = None;
                return Some(ButtonEvent::Up {
                    button: Button::Stop,
                    held: ev.time.saturating_sub(down),
                });
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// A default Shift+F10 / Shift+F9 tracker, as the shipped config binds.
    fn tracker() -> KeyTracker {
        KeyTracker::new(
            KeyCombo::parse("Shift+F10").unwrap(),
            KeyCombo::parse("Shift+F9").unwrap(),
        )
    }

    #[test]
    fn shift_then_start_key_emits_a_down() {
        let mut t = tracker();
        // Shift is a modifier — no event of its own.
        assert_eq!(t.feed(RawKeyEvent::down(codes::KEY_LEFTSHIFT, ms(0))), None);
        assert_eq!(
            t.feed(RawKeyEvent::down(codes::KEY_F10, ms(10))),
            Some(ButtonEvent::Down(Button::Start)),
        );
    }

    #[test]
    fn a_full_press_release_emits_down_then_up_with_held_duration() {
        let mut t = tracker();
        t.feed(RawKeyEvent::down(codes::KEY_LEFTSHIFT, ms(0)));
        assert_eq!(
            t.feed(RawKeyEvent::down(codes::KEY_F10, ms(100))),
            Some(ButtonEvent::Down(Button::Start)),
        );
        assert_eq!(
            t.feed(RawKeyEvent::up(codes::KEY_F10, ms(180))),
            Some(ButtonEvent::Up { button: Button::Start, held: ms(80) }),
        );
    }

    #[test]
    fn start_key_without_shift_is_not_the_start_combo() {
        // No modifier held → Shift+F10 must not fire (avoids a bare-F10 misfire).
        let mut t = tracker();
        assert_eq!(t.feed(RawKeyEvent::down(codes::KEY_F10, ms(0))), None);
        assert_eq!(t.feed(RawKeyEvent::up(codes::KEY_F10, ms(50))), None);
    }

    #[test]
    fn releasing_shift_before_the_key_still_resolves_the_button() {
        // Push-to-talk: the held duration, not the modifier, classifies release.
        // Real users often lift Shift a beat before the main key.
        let mut t = tracker();
        t.feed(RawKeyEvent::down(codes::KEY_LEFTSHIFT, ms(0)));
        assert_eq!(
            t.feed(RawKeyEvent::down(codes::KEY_F10, ms(5))),
            Some(ButtonEvent::Down(Button::Start)),
        );
        // Shift up first — no button event.
        assert_eq!(t.feed(RawKeyEvent::up(codes::KEY_LEFTSHIFT, ms(400))), None);
        // Then the key up — still resolves Start, with the full hold.
        assert_eq!(
            t.feed(RawKeyEvent::up(codes::KEY_F10, ms(420))),
            Some(ButtonEvent::Up { button: Button::Start, held: ms(415) }),
        );
    }

    #[test]
    fn stop_combo_is_tracked_independently() {
        let mut t = tracker();
        t.feed(RawKeyEvent::down(codes::KEY_RIGHTSHIFT, ms(0)));
        assert_eq!(
            t.feed(RawKeyEvent::down(codes::KEY_F9, ms(10))),
            Some(ButtonEvent::Down(Button::Stop)),
        );
        assert_eq!(
            t.feed(RawKeyEvent::up(codes::KEY_F9, ms(600))),
            Some(ButtonEvent::Up { button: Button::Stop, held: ms(590) }),
        );
    }

    #[test]
    fn autorepeat_down_does_not_re_emit_while_held() {
        // evdev autorepeat (value 2) can arrive as extra downs; a second down
        // while already latched must not emit another Down.
        let mut t = tracker();
        t.feed(RawKeyEvent::down(codes::KEY_LEFTSHIFT, ms(0)));
        assert_eq!(
            t.feed(RawKeyEvent::down(codes::KEY_F10, ms(10))),
            Some(ButtonEvent::Down(Button::Start)),
        );
        assert_eq!(t.feed(RawKeyEvent::down(codes::KEY_F10, ms(40))), None);
        assert_eq!(t.feed(RawKeyEvent::down(codes::KEY_F10, ms(70))), None);
        // The eventual release still resolves once, measured from the FIRST down.
        assert_eq!(
            t.feed(RawKeyEvent::up(codes::KEY_F10, ms(110))),
            Some(ButtonEvent::Up { button: Button::Start, held: ms(100) }),
        );
    }

    #[test]
    fn an_unrelated_key_is_ignored_entirely() {
        // Security: only the configured combos produce events; other keys
        // (here 'A') pass through silently — never tracked, never emitted.
        let mut t = tracker();
        t.feed(RawKeyEvent::down(codes::KEY_LEFTSHIFT, ms(0)));
        assert_eq!(
            t.feed(RawKeyEvent::down(crate::key_combo::key_code("A").unwrap(), ms(10))),
            None,
        );
        assert_eq!(
            t.feed(RawKeyEvent::up(crate::key_combo::key_code("A").unwrap(), ms(60))),
            None,
        );
    }

    #[test]
    fn a_release_with_no_prior_press_is_ignored() {
        // A key-up for a key we never saw go down (e.g. held across daemon
        // start) must not synthesize a phantom Up.
        let mut t = tracker();
        t.feed(RawKeyEvent::down(codes::KEY_LEFTSHIFT, ms(0)));
        assert_eq!(t.feed(RawKeyEvent::up(codes::KEY_F10, ms(10))), None);
    }

    #[test]
    fn the_full_tap_sequence_drives_a_latch_then_a_stop_tap() {
        // End-to-end through the gesture mapper: a Start tap latches recording
        // (no stop), and a later Stop tap stops it. This is the headline flow.
        use crate::gesture::command_for;
        use crate::protocol::{Command, State};
        let threshold = ms(250);

        let mut t = tracker();
        let mut state = State::Idle;

        // Start tap: down begins recording, quick up latches (no command).
        t.feed(RawKeyEvent::down(codes::KEY_LEFTSHIFT, ms(0)));
        let down = t.feed(RawKeyEvent::down(codes::KEY_F10, ms(10))).unwrap();
        if let Some(c) = command_for(state, down, threshold) {
            assert_eq!(c, Command::Toggle);
            state = State::Recording;
        }
        let up = t.feed(RawKeyEvent::up(codes::KEY_F10, ms(90))).unwrap();
        assert_eq!(command_for(state, up, threshold), None, "tap latches");
        assert_eq!(state, State::Recording);

        // Stop tap: a quick Stop press/release stops the latched recording.
        t.feed(RawKeyEvent::down(codes::KEY_LEFTSHIFT, ms(1000)));
        t.feed(RawKeyEvent::down(codes::KEY_F9, ms(1010)));
        let stop_up = t.feed(RawKeyEvent::up(codes::KEY_F9, ms(1080))).unwrap();
        assert_eq!(command_for(state, stop_up, threshold), Some(Command::Toggle));
    }
}
