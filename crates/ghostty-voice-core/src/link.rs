//! The `talk-to` **wrapper sink**'s link state to the daemon (task-10).
//!
//! The status strip normally shows the daemon's voice [`State`](crate::protocol::State)
//! (`idle`/`recording`/`transcribing`), pushed over a healthy registered
//! connection. When the link is *not* cleanly registered, the strip instead shows
//! a [`LinkState`] token so the user can tell the failure modes apart — a stale or
//! incompatible daemon must not masquerade as a generic "offline" (the exact
//! confusion seen during dogfooding).
//!
//! This is the pure decision layer: the token mapping and the classification of
//! the daemon's first reply to `register-sink`. The socket/IO glue lives in the
//! `talk-to` binary.

use crate::protocol::Frame;

/// The wrapper sink's link to the daemon when it is **not** cleanly registered.
/// (A healthy link shows the daemon's voice `State` instead.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// `connect()` failed — no daemon is listening on the control socket.
    Unreachable,
    /// The daemon answered `register-sink` with an error — it refused the wrapper
    /// sink (e.g. an old daemon that doesn't understand the command).
    Rejected,
    /// The connection had registered and was then dropped (EOF) — the daemon went
    /// away after a previously-good connection.
    Dropped,
}

impl LinkState {
    /// The strip token for this link state — distinct per failure mode so a
    /// user (and the docs) can tell them apart.
    pub fn token(&self) -> &'static str {
        match self {
            LinkState::Unreachable => "unreachable",
            LinkState::Rejected => "rejected",
            LinkState::Dropped => "dropped",
        }
    }
}

/// How the daemon's **first reply** to `register-sink` classifies the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// The daemon accepted us as a wrapper sink — it pushed a [`Frame`] (a `state`
    /// or `transcript` line) rather than a one-shot reply.
    Registered,
    /// The daemon did not accept us: it replied with a one-shot response (an
    /// `err`/`ok` line) or something unparseable — an old daemon answers
    /// `err unknown command: register-sink`.
    Rejected,
}

/// Classify the daemon's first reply line to `register-sink`.
///
/// A registered wrapper sink receives pushed [`Frame`]s, so a parseable frame
/// means we are registered; anything else (a one-shot `ok`/`err` reply, or junk)
/// means the daemon refused us — surfaced as [`LinkState::Rejected`].
pub fn classify_first_line(line: &str) -> Registration {
    if Frame::parse(line).is_ok() {
        Registration::Registered
    } else {
        Registration::Rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::State;

    #[test]
    fn link_state_tokens_are_distinct_per_failure_mode() {
        assert_eq!(LinkState::Unreachable.token(), "unreachable");
        assert_eq!(LinkState::Rejected.token(), "rejected");
        assert_eq!(LinkState::Dropped.token(), "dropped");
        // The three must be distinguishable (no collapse into one "offline").
        let tokens = [
            LinkState::Unreachable.token(),
            LinkState::Rejected.token(),
            LinkState::Dropped.token(),
        ];
        let unique: std::collections::HashSet<_> = tokens.iter().collect();
        assert_eq!(unique.len(), 3, "every link state needs a distinct token");
    }

    #[test]
    fn a_pushed_state_frame_means_we_are_registered() {
        let first = format!("{}\n", Frame::State(State::Idle).encode());
        assert_eq!(classify_first_line(&first), Registration::Registered);
    }

    #[test]
    fn a_pushed_transcript_frame_means_we_are_registered() {
        let first = format!("{}\n", Frame::Transcript("hello world".to_owned()).encode());
        assert_eq!(classify_first_line(&first), Registration::Registered);
    }

    #[test]
    fn an_error_reply_means_the_daemon_rejected_the_sink() {
        // An old daemon that doesn't know register-sink answers a one-shot error.
        assert_eq!(
            classify_first_line("err unknown command: register-sink"),
            Registration::Rejected,
        );
    }

    #[test]
    fn a_one_shot_ok_reply_is_not_a_registration() {
        // `ok idle` is the normal one-shot reply shape — never a sink push frame.
        assert_eq!(classify_first_line("ok idle"), Registration::Rejected);
    }

    #[test]
    fn garbage_is_treated_as_rejected_not_registered() {
        assert_eq!(classify_first_line("\u{fffd}@#$"), Registration::Rejected);
    }
}
