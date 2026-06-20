//! The daemon's recording state machine (S3 — Recorder + delivery queue).
//!
//! The Recorder is the single mic facility: `Idle` or `Recording`. Stopping a
//! recording enqueues the utterance and returns to `Idle` immediately, so a new
//! recording can start while prior utterances transcribe and type through the
//! ordered delivery queue (which the daemon drains, serialized, in record-order).
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
    /// Start a hands-free VAD recording: `sox` records and self-terminates on
    /// the first trailing silence, then enqueues like any other utterance (S5).
    StartVadRecording,
    /// Stop the recorder, enqueue the utterance, and kick off background
    /// transcription — the recorder is freed (Idle) so the next recording can
    /// start while this one drains through the delivery queue.
    StopAndEnqueue,
    DiscardRecording,
    ReloadConfig,
    /// Re-inject the most-recent cached transcript (recovery-only, S3).
    ReplayLast,
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
        (State::Recording, Command::Toggle) => go(State::Idle, Action::StopAndEnqueue),

        // VAD starts a hands-free recording; `sox` self-terminates on the first
        // silence. A `toggle` while it runs is the Recording+Toggle case above —
        // a manual early stop. A `vad` arriving mid-recording is ignored (the
        // recorder is already busy).
        (State::Idle, Command::Vad) => go(State::Recording, Action::StartVadRecording),
        (State::Recording, Command::Vad) => go(State::Recording, Action::None),
        (State::Transcribing, Command::Vad) => go(State::Recording, Action::StartVadRecording),
        // The recorder is freed on stop, so it is never in Transcribing when a
        // toggle arrives; treat any stray case as starting a fresh recording.
        (State::Transcribing, Command::Toggle) => go(State::Recording, Action::StartRecording),

        (State::Recording, Command::Cancel) => go(State::Idle, Action::DiscardRecording),
        (s, Command::Cancel) => go(s, Action::None),

        (s, Command::Status) => go(s, Action::None),
        (s, Command::Reload) => go(s, Action::ReloadConfig),

        // Replay re-injects a cached transcript — independent of the recorder,
        // so it's allowed in any ready state without changing it.
        (s, Command::ReplayLast) => go(s, Action::ReplayLast),
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
    fn toggle_stops_and_enqueues_then_frees_the_recorder() {
        // The Recorder + delivery queue model: stopping enqueues the utterance
        // and returns to Idle so a new recording can start immediately while
        // the prior one transcribes/types in the background.
        let t = apply(State::Recording, Command::Toggle);
        assert_eq!(t.next, State::Idle);
        assert_eq!(t.action, Action::StopAndEnqueue);
    }

    #[test]
    fn a_new_recording_can_start_while_prior_utterances_transcribe() {
        // After stop->Idle, another toggle starts recording again — back-to-back
        // dictation without waiting for the queue to drain.
        let stopped = apply(State::Recording, Command::Toggle);
        let restarted = apply(stopped.next, Command::Toggle);
        assert_eq!(restarted.next, State::Recording);
        assert_eq!(restarted.action, Action::StartRecording);
    }

    #[test]
    fn vad_starts_a_hands_free_recording_from_idle() {
        let t = apply(State::Idle, Command::Vad);
        assert_eq!(t.next, State::Recording);
        assert_eq!(t.action, Action::StartVadRecording);
        assert_eq!(t.response, Response::Ok(State::Recording));
    }

    #[test]
    fn toggle_during_a_vad_recording_is_a_manual_early_stop() {
        // VAD put the recorder in Recording; a toggle stops + enqueues it early,
        // exactly like stopping any other recording.
        let started = apply(State::Idle, Command::Vad);
        let stopped = apply(started.next, Command::Toggle);
        assert_eq!(stopped.next, State::Idle);
        assert_eq!(stopped.action, Action::StopAndEnqueue);
    }

    #[test]
    fn vad_while_already_recording_is_ignored() {
        // The recorder is busy; a second vad press must not start a new capture.
        let t = apply(State::Recording, Command::Vad);
        assert_eq!(t.next, State::Recording);
        assert_eq!(t.action, Action::None);
    }

    #[test]
    fn vad_is_rejected_while_loading() {
        assert!(matches!(
            apply(State::Loading, Command::Vad).response,
            Response::Err(_)
        ));
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
    fn replay_last_reinjects_without_changing_state() {
        for s in [State::Idle, State::Recording, State::Transcribing] {
            let t = apply(s, Command::ReplayLast);
            assert_eq!(t.next, s, "replay-last must not change state");
            assert_eq!(t.action, Action::ReplayLast);
            assert_eq!(t.response, Response::Ok(s));
        }
    }

    #[test]
    fn replay_last_is_rejected_while_loading() {
        assert!(matches!(
            apply(State::Loading, Command::ReplayLast).response,
            Response::Err(_)
        ));
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
