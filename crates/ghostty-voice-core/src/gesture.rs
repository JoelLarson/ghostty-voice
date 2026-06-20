//! Button-gesture mapping (evdev input layer).
//!
//! Translates the two configured buttons (Start = Shift+F10, Stop = Shift+F9
//! by default) into existing daemon [`Command`]s, with tap-vs-hold semantics:
//!
//! - **Start down** begins recording immediately (record-on-press, so
//!   push-to-talk never clips the first words).
//! - **Start up** after the hold threshold = push-to-talk release → stop.
//!   A quick release (tap) leaves the recording latched.
//! - **Stop up** quick (tap) = stop a latched recording.
//! - **Stop up** held = start a VAD (auto-stop-on-silence) recording.
//!
//! Pure: the evdev boundary owns timing/Shift-tracking and feeds these events;
//! this just maps (state, event) → command. The resulting command flows
//! through the normal [`crate::machine`] transition.

use std::time::Duration;

use crate::protocol::{Command, State};

/// The two bound buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Start,
    Stop,
}

/// A button press or release; `held` is how long it was down (on release).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEvent {
    Down(Button),
    Up { button: Button, held: Duration },
}

/// The daemon command a gesture should issue, given current state, or `None`.
pub fn command_for(state: State, event: ButtonEvent, hold_threshold: Duration) -> Option<Command> {
    match event {
        // Start pressed: begin recording now, but only if idle.
        ButtonEvent::Down(Button::Start) => (state == State::Idle).then_some(Command::Toggle),
        // Stop press does nothing on its own; the gesture resolves on release.
        ButtonEvent::Down(Button::Stop) => None,

        // Start released: a held release is push-to-talk → stop; a tap latches.
        ButtonEvent::Up {
            button: Button::Start,
            held,
        } => (held >= hold_threshold && state == State::Recording).then_some(Command::Toggle),

        ButtonEvent::Up {
            button: Button::Stop,
            held,
        } => {
            if held < hold_threshold {
                // Tap = stop a latched recording.
                (state == State::Recording).then_some(Command::Toggle)
            } else {
                // Hold = start a VAD recording (only when idle).
                (state == State::Idle).then_some(Command::Vad)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLD: Duration = Duration::from_millis(250);
    const SHORT: Duration = Duration::from_millis(80);
    const LONG: Duration = Duration::from_millis(600);

    fn down(b: Button) -> ButtonEvent {
        ButtonEvent::Down(b)
    }
    fn up(b: Button, held: Duration) -> ButtonEvent {
        ButtonEvent::Up { button: b, held }
    }

    #[test]
    fn start_down_begins_recording_when_idle() {
        assert_eq!(
            command_for(State::Idle, down(Button::Start), THRESHOLD),
            Some(Command::Toggle),
        );
    }

    #[test]
    fn start_down_is_ignored_when_already_recording() {
        assert_eq!(
            command_for(State::Recording, down(Button::Start), THRESHOLD),
            None
        );
    }

    #[test]
    fn start_tap_release_latches_no_command() {
        assert_eq!(
            command_for(State::Recording, up(Button::Start, SHORT), THRESHOLD),
            None,
        );
    }

    #[test]
    fn start_hold_release_is_push_to_talk_stop() {
        assert_eq!(
            command_for(State::Recording, up(Button::Start, LONG), THRESHOLD),
            Some(Command::Toggle),
        );
    }

    #[test]
    fn stop_tap_stops_a_latched_recording() {
        assert_eq!(
            command_for(State::Recording, up(Button::Stop, SHORT), THRESHOLD),
            Some(Command::Toggle),
        );
    }

    #[test]
    fn stop_tap_is_ignored_when_idle() {
        assert_eq!(
            command_for(State::Idle, up(Button::Stop, SHORT), THRESHOLD),
            None
        );
    }

    #[test]
    fn stop_hold_starts_vad_when_idle() {
        assert_eq!(
            command_for(State::Idle, up(Button::Stop, LONG), THRESHOLD),
            Some(Command::Vad),
        );
    }

    #[test]
    fn stop_hold_is_ignored_while_recording() {
        assert_eq!(
            command_for(State::Recording, up(Button::Stop, LONG), THRESHOLD),
            None
        );
    }

    #[test]
    fn stop_down_is_a_noop() {
        assert_eq!(
            command_for(State::Idle, down(Button::Stop), THRESHOLD),
            None
        );
    }
}
