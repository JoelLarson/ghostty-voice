//! The daemon's recording state machine (Recorder + delivery queue).
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
    /// the first trailing silence, then enqueues like any other utterance.
    StartVadRecording,
    /// Start a hands-free Continuous-mode session: `sox` splits the capture
    /// into silence-bounded clips that batch-transcribe in the background; a long
    /// silence ends the session and delivers the assembled transcript. `cancel`
    /// (DiscardRecording) aborts the whole session.
    StartContinuous,
    /// Stop the recorder, enqueue the utterance, and kick off background
    /// transcription — the recorder is freed (Idle) so the next recording can
    /// start while this one drains through the delivery queue.
    StopAndEnqueue,
    DiscardRecording,
    ReloadConfig,
    /// Re-inject the most-recent cached transcript (recovery-only).
    ReplayLast,
    /// Start a hands-free streaming dictation: `sox` captures into a growing WAV
    /// (auto-stopping on a long trailing silence) while a self-paced decode loop
    /// pushes a live preview into the active wrapper's prompt. The conscious
    /// extension of ADR-0002 (live rough preview, batch-accurate reconcile).
    StartStreaming,
    /// Force-stop the streaming dictation now (Shift+F10): stop capture, run the
    /// batch-accurate reconcile, and deliver. The hands-free path reaches the same
    /// finalize when `sox` auto-stops on the long silence.
    StopStreaming,
    /// Abort the streaming dictation: stop capture, erase the live preview, and
    /// deliver nothing (`cancel`).
    DiscardStreaming,
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
        // While the first-run model download runs, only status is answered; the
        // rest are rejected (the daemon notifies). Nothing is operable yet — the
        // model isn't even on disk, let alone in VRAM.
        (State::Downloading(_), Command::Status) => go(state, Action::None),
        (State::Downloading(_), _) => reject(state, "model still downloading"),

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

        // Continuous mode opens a hands-free session; like VAD, a second press
        // while the recorder is busy is ignored. `cancel` (DiscardRecording)
        // aborts the whole session.
        (State::Idle, Command::Continuous) => go(State::Recording, Action::StartContinuous),
        (State::Recording, Command::Continuous) => go(State::Recording, Action::None),
        (State::Transcribing, Command::Continuous) => go(State::Recording, Action::StartContinuous),
        // The recorder is freed on stop, so it is never in Transcribing when a
        // toggle arrives; treat any stray case as starting a fresh recording.
        (State::Transcribing, Command::Toggle) => go(State::Recording, Action::StartRecording),

        // Streaming dictation: Shift+F9 opens a hands-free live preview from idle
        // (or after a prior recording freed the recorder). The one-mouth invariant
        // holds — a start while the recorder is already busy is ignored.
        (State::Idle, Command::Streaming) => go(State::Streaming, Action::StartStreaming),
        (State::Transcribing, Command::Streaming) => go(State::Streaming, Action::StartStreaming),
        (State::Recording, Command::Streaming) => go(State::Recording, Action::None),
        (State::Streaming, Command::Streaming) => go(State::Streaming, Action::None),
        // Shift+F10 (Toggle) force-stops a live dictation: stop capture, reconcile,
        // deliver. The hands-free path reaches the same finalize on the long
        // silence. A `vad`/`continuous` arriving mid-dictation is ignored (busy).
        (State::Streaming, Command::Toggle) => go(State::Idle, Action::StopStreaming),
        (State::Streaming, Command::Cancel) => go(State::Idle, Action::DiscardStreaming),
        (State::Streaming, Command::Vad) => go(State::Streaming, Action::None),
        (State::Streaming, Command::Continuous) => go(State::Streaming, Action::None),

        (State::Recording, Command::Cancel) => go(State::Idle, Action::DiscardRecording),
        (s, Command::Cancel) => go(s, Action::None),

        (s, Command::Status) => go(s, Action::None),
        (s, Command::Reload) => go(s, Action::ReloadConfig),

        // Replay re-injects a cached transcript — independent of the recorder,
        // so it's allowed in any ready state without changing it.
        (s, Command::ReplayLast) => go(s, Action::ReplayLast),

        // Registering a wrapper sink is handled at the connection layer (the
        // persistent registered-sink path), not the recorder state machine — it
        // never changes recording state. This arm is a defensive no-op so the
        // match stays total; in practice `register-sink` is intercepted before
        // it ever reaches here.
        (s, Command::RegisterSink(_)) => go(s, Action::None),
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
    fn continuous_starts_a_session_from_idle() {
        // Continuous mode opens a hands-free session: sox splits the
        // capture into clips that transcribe in the background. The recorder
        // goes to Recording, exactly like the other start paths.
        let t = apply(State::Idle, Command::Continuous);
        assert_eq!(t.next, State::Recording);
        assert_eq!(t.action, Action::StartContinuous);
        assert_eq!(t.response, Response::Ok(State::Recording));
    }

    #[test]
    fn continuous_while_already_recording_is_ignored() {
        // The recorder is busy; a second continuous press must not start a new
        // session over the running one.
        let t = apply(State::Recording, Command::Continuous);
        assert_eq!(t.next, State::Recording);
        assert_eq!(t.action, Action::None);
    }

    #[test]
    fn continuous_is_rejected_while_loading() {
        assert!(matches!(
            apply(State::Loading, Command::Continuous).response,
            Response::Err(_)
        ));
    }

    #[test]
    fn cancel_during_a_continuous_session_aborts_the_recording() {
        // cancel aborts the whole session: it discards the recorder just like a
        // normal recording. The daemon tears the session pipeline down on the
        // same DiscardRecording action.
        let started = apply(State::Idle, Command::Continuous);
        let cancelled = apply(started.next, Command::Cancel);
        assert_eq!(cancelled.next, State::Idle);
        assert_eq!(cancelled.action, Action::DiscardRecording);
    }

    // ---- streaming dictation -------------------------------------------

    #[test]
    fn streaming_starts_a_live_dictation_from_idle() {
        let t = apply(State::Idle, Command::Streaming);
        assert_eq!(t.next, State::Streaming);
        assert_eq!(t.action, Action::StartStreaming);
        assert_eq!(t.response, Response::Ok(State::Streaming));
    }

    #[test]
    fn streaming_starts_from_transcribing_like_the_other_hands_free_modes() {
        // The recorder is free while a prior utterance only transcribes, so a new
        // streaming dictation can begin (mirrors vad/continuous from Transcribing).
        let t = apply(State::Transcribing, Command::Streaming);
        assert_eq!(t.next, State::Streaming);
        assert_eq!(t.action, Action::StartStreaming);
    }

    #[test]
    fn streaming_while_already_recording_is_ignored() {
        // One mouth: a streaming start must not open a second capture over a
        // running batch recording.
        let t = apply(State::Recording, Command::Streaming);
        assert_eq!(t.next, State::Recording);
        assert_eq!(t.action, Action::None);
    }

    #[test]
    fn a_second_streaming_press_while_streaming_is_ignored() {
        let t = apply(State::Streaming, Command::Streaming);
        assert_eq!(t.next, State::Streaming);
        assert_eq!(t.action, Action::None);
    }

    #[test]
    fn toggle_during_streaming_force_stops_and_finalizes() {
        // Shift+F10 keeps one meaning: stop whatever runs. During a live dictation
        // that is the force-stop — stop capture, reconcile, deliver — back to idle.
        let t = apply(State::Streaming, Command::Toggle);
        assert_eq!(t.next, State::Idle);
        assert_eq!(t.action, Action::StopStreaming);
    }

    #[test]
    fn cancel_during_streaming_discards_the_preview() {
        let t = apply(State::Streaming, Command::Cancel);
        assert_eq!(t.next, State::Idle);
        assert_eq!(t.action, Action::DiscardStreaming);
    }

    #[test]
    fn vad_or_continuous_during_streaming_is_ignored() {
        // The recorder is busy with the live dictation; a stray batch start press
        // must not disturb it.
        for cmd in [Command::Vad, Command::Continuous] {
            let t = apply(State::Streaming, cmd);
            assert_eq!(t.next, State::Streaming);
            assert_eq!(t.action, Action::None);
        }
    }

    #[test]
    fn streaming_is_rejected_while_loading() {
        assert!(matches!(
            apply(State::Loading, Command::Streaming).response,
            Response::Err(_)
        ));
    }

    #[test]
    fn streaming_is_rejected_while_downloading() {
        let t = apply(State::Downloading(None), Command::Streaming);
        assert_eq!(t.next, State::Downloading(None));
        assert_eq!(t.action, Action::None);
        assert!(matches!(t.response, Response::Err(_)));
    }

    #[test]
    fn status_and_replay_are_answered_while_streaming() {
        // Read-only commands stay available during a live dictation without
        // disturbing it.
        let status = apply(State::Streaming, Command::Status);
        assert_eq!(status.next, State::Streaming);
        assert_eq!(status.action, Action::None);
        let replay = apply(State::Streaming, Command::ReplayLast);
        assert_eq!(replay.next, State::Streaming);
        assert_eq!(replay.action, Action::ReplayLast);
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
    fn downloading_rejects_recording_commands_with_a_clear_message() {
        // First-run model download: toggle/vad/continuous must be rejected
        // (the daemon notifies "model still downloading"), never hang or start a
        // recording with no model to transcribe against.
        for cmd in [Command::Toggle, Command::Vad, Command::Continuous] {
            let t = apply(State::Downloading(None), cmd);
            assert_eq!(t.next, State::Downloading(None), "stays in Downloading");
            assert_eq!(t.action, Action::None, "no recording starts");
            match &t.response {
                Response::Err(msg) => assert!(
                    msg.contains("download"),
                    "rejection should mention downloading, got {msg:?}"
                ),
                other => panic!("expected an Err response, got {other:?}"),
            }
        }
    }

    #[test]
    fn downloading_reports_status_without_changing_state() {
        let t = apply(State::Downloading(None), Command::Status);
        assert_eq!(t.next, State::Downloading(None));
        assert_eq!(t.action, Action::None);
        assert_eq!(t.response, Response::Ok(State::Downloading(None)));
    }

    #[test]
    fn downloading_status_preserves_the_carried_percent() {
        // The download percent lives in the state, so a status query while
        // downloading must echo the current percent unchanged — never reset it.
        let t = apply(State::Downloading(Some(42)), Command::Status);
        assert_eq!(t.next, State::Downloading(Some(42)));
        assert_eq!(t.action, Action::None);
        assert_eq!(t.response, Response::Ok(State::Downloading(Some(42))));
    }

    #[test]
    fn downloading_rejects_replay_and_reload_too() {
        // Nothing is operable until the model lands; only status is answered.
        for cmd in [Command::ReplayLast, Command::Reload, Command::Cancel] {
            assert!(
                matches!(
                    apply(State::Downloading(Some(50)), cmd).response,
                    Response::Err(_)
                ),
                "{cmd:?} must be rejected while downloading"
            );
        }
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
