//! Delivery decision.
//!
//! The hands-free rule: a transcript is auto-typed if it's still fresh — i.e.
//! produced within the freshness window of the recording ending — otherwise it
//! is held for `replay-last`, never blasted into a window you've likely left.
//! The window is generous (a backstop, not a routine gate).

use std::time::Duration;

/// What to do with a freshly produced transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Fresh enough — type it into the focused window.
    AutoType,
    /// Stale (e.g. a long outage) — cache it for `replay-last`.
    HoldForReplay,
}

/// Decide delivery from how long ago recording ended and the freshness window.
pub fn decide(since_record_end: Duration, freshness_window: Duration) -> Delivery {
    if since_record_end <= freshness_window {
        Delivery::AutoType
    } else {
        Delivery::HoldForReplay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(900);

    #[test]
    fn fresh_transcript_is_auto_typed() {
        assert_eq!(decide(Duration::from_secs(3), WINDOW), Delivery::AutoType);
    }

    #[test]
    fn at_the_boundary_is_still_auto_typed() {
        assert_eq!(decide(WINDOW, WINDOW), Delivery::AutoType);
    }

    #[test]
    fn stale_transcript_is_held() {
        assert_eq!(
            decide(WINDOW + Duration::from_secs(1), WINDOW),
            Delivery::HoldForReplay,
        );
    }
}
