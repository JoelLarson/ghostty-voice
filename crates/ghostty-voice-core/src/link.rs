//! The `talk-to` **wrapper sink**'s link state to the daemon.
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
    /// The daemon answered `register-sink` with an error for a reason other than a
    /// version mismatch — it refused the wrapper sink.
    Rejected,
    /// The daemon is **version-incompatible**: it refused the versioned
    /// `register-sink` (a new daemon answering `err incompatible …`), or it is too
    /// old to understand the command at all (answering `err unknown command …`).
    /// Distinct from `unreachable` so a stale daemon after an upgrade is legible
    /// — the remedy is to restart/upgrade the daemon.
    Incompatible,
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
            LinkState::Incompatible => "incompatible",
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
    /// The daemon refused us for a **version** reason: an explicit
    /// `err incompatible …` (a newer/older daemon that speaks the handshake) or an
    /// `err unknown command …` (an old daemon that errs on the versioned command).
    Incompatible,
    /// The daemon refused us for some other reason (any other `err`, or an
    /// unparseable line).
    Rejected,
}

/// Classify the daemon's first reply line to `register-sink`.
///
/// A registered wrapper sink receives pushed [`Frame`]s, so a parseable frame
/// means we are registered. Otherwise the daemon answered a one-shot reply: an
/// `err` mentioning an incompatible version, or an old daemon's
/// `err unknown command`, is a version mismatch ([`Registration::Incompatible`]);
/// any other reply is a plain [`Registration::Rejected`].
pub fn classify_first_line(line: &str) -> Registration {
    if Frame::parse(line).is_ok() {
        return Registration::Registered;
    }
    if let Some(message) = line.trim().strip_prefix("err ") {
        let message = message.trim_start();
        if message.starts_with("incompatible") || message.starts_with("unknown command") {
            return Registration::Incompatible;
        }
    }
    Registration::Rejected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::State;

    #[test]
    fn link_state_tokens_are_distinct_per_failure_mode() {
        assert_eq!(LinkState::Unreachable.token(), "unreachable");
        assert_eq!(LinkState::Rejected.token(), "rejected");
        assert_eq!(LinkState::Incompatible.token(), "incompatible");
        assert_eq!(LinkState::Dropped.token(), "dropped");
        // All must be distinguishable (no collapse into one "offline").
        let tokens = [
            LinkState::Unreachable.token(),
            LinkState::Rejected.token(),
            LinkState::Incompatible.token(),
            LinkState::Dropped.token(),
        ];
        let unique: std::collections::HashSet<_> = tokens.iter().collect();
        assert_eq!(unique.len(), 4, "every link state needs a distinct token");
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
    fn an_old_daemons_unknown_command_error_is_an_incompatible_version() {
        // An old daemon that doesn't know the versioned register-sink answers a
        // one-shot `err unknown command …` — treated as incompatible (the exact
        // stale-daemon-after-upgrade confusion), not a generic rejection/offline.
        assert_eq!(
            classify_first_line("err unknown command: register-sink 1"),
            Registration::Incompatible,
        );
    }

    #[test]
    fn an_explicit_incompatible_error_is_an_incompatible_version() {
        assert_eq!(
            classify_first_line(
                "err incompatible protocol version (daemon speaks 1, client sent 2)"
            ),
            Registration::Incompatible,
        );
    }

    #[test]
    fn another_error_reply_is_a_plain_rejection() {
        assert_eq!(
            classify_first_line("err ydotoold unreachable"),
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
