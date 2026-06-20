//! The daemon's recording state machine (S2, single-utterance).
//!
//! A pure function from (current [`State`], [`Command`]) to a [`Transition`]:
//! the next state, the side-effecting [`Action`] the daemon must perform, and
//! the [`Response`] to send back. The daemon owns the IO; this owns the rules.

use crate::protocol::{Command, Response, State};

/// A side effect the daemon must carry out as part of a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    StartRecording,
    StopAndTranscribe,
    DiscardRecording,
    ReloadConfig,
}

/// The result of applying a command: where to go, what to do, what to reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub next: State,
    pub action: Action,
    pub response: Response,
}

fn go(next: State, action: Action) -> Transition {
    Transition {
        next,
        action,
        response: Response::Ok(next),
    }
}

fn reject(state: State, message: &str) -> Transition {
    Transition {
        next: state,
        action: Action::None,
        response: Response::Err(message.to_owned()),
    }
}

/// Apply `command` to `state`, returning the resulting transition.
pub fn apply(state: State, command: Command) -> Transition {
    match (state, command) {
        // While the model loads, only status is answered; the rest are rejected.
        (State::Loading, Command::Status) => go(State::Loading, Action::None),
        (State::Loading, _) => reject(State::Loading, "model still loading"),

        (State::Idle, Command::Toggle) => go(State::Recording, Action::StartRecording),
        (State::Recording, Command::Toggle) => go(State::Transcribing, Action::StopAndTranscribe),
        (State::Transcribing, Command::Toggle) => go(State::Transcribing, Action::None),

        (State::Recording, Command::Cancel) => go(State::Idle, Action::DiscardRecording),
        (s, Command::Cancel) => go(s, Action::None),

        (s, Command::Status) => go(s, Action::None),
        (s, Command::Reload) => go(s, Action::ReloadConfig),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_starts_recording_from_idle() {
        let t = apply(State::Idle, Command::Toggle);
        assert_eq!(t.next, State::Recording);
        assert_eq!(t.action, Action::StartRecording);
        assert_eq!(t.response, Response::Ok(State::Recording));
    }

    #[test]
    fn toggle_stops_and_transcribes_from_recording() {
        let t = apply(State::Recording, Command::Toggle);
        assert_eq!(t.next, State::Transcribing);
        assert_eq!(t.action, Action::StopAndTranscribe);
    }

    #[test]
    fn toggle_is_ignored_while_transcribing() {
        let t = apply(State::Transcribing, Command::Toggle);
        assert_eq!(t.next, State::Transcribing);
        assert_eq!(t.action, Action::None);
    }

    #[test]
    fn cancel_discards_a_recording() {
        let t = apply(State::Recording, Command::Cancel);
        assert_eq!(t.next, State::Idle);
        assert_eq!(t.action, Action::DiscardRecording);
    }

    #[test]
    fn cancel_is_a_noop_when_idle() {
        let t = apply(State::Idle, Command::Cancel);
        assert_eq!(t.next, State::Idle);
        assert_eq!(t.action, Action::None);
    }

    #[test]
    fn status_preserves_state_with_no_action() {
        for s in [State::Idle, State::Recording, State::Transcribing] {
            let t = apply(s, Command::Status);
            assert_eq!(t.next, s);
            assert_eq!(t.action, Action::None);
            assert_eq!(t.response, Response::Ok(s));
        }
    }

    #[test]
    fn reload_requests_config_reload_without_changing_state() {
        let t = apply(State::Recording, Command::Reload);
        assert_eq!(t.next, State::Recording);
        assert_eq!(t.action, Action::ReloadConfig);
    }

    #[test]
    fn loading_rejects_commands_but_reports_status() {
        assert!(matches!(
            apply(State::Loading, Command::Toggle).response,
            Response::Err(_)
        ));
        let status = apply(State::Loading, Command::Status);
        assert_eq!(status.response, Response::Ok(State::Loading));
        assert_eq!(status.action, Action::None);
    }
}
