//! whisper-server supervision policy (S2).
//!
//! The async spawn/kill/probe loop lives in the daemon; the *policy* — how
//! long to wait before each restart after the child dies — lives here, pure
//! and testable. Exponential backoff with a cap.

use std::time::Duration;

/// Exponential restart backoff, capped at `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    base: Duration,
    max: Duration,
}

impl Backoff {
    pub fn new(base: Duration, max: Duration) -> Self {
        Self { base, max }
    }

    /// Delay before the next restart given the number of consecutive failures
    /// so far (0 → no delay; 1 → `base`; 2 → `2*base`; … capped at `max`).
    pub fn delay(&self, consecutive_failures: u32) -> Duration {
        if consecutive_failures == 0 {
            return Duration::ZERO;
        }
        // base * 2^(failures-1), saturating, then capped at max.
        let factor = 1u64
            .checked_shl(consecutive_failures - 1)
            .unwrap_or(u64::MAX);
        let base_ms = self.base.as_millis().min(u64::MAX as u128) as u64;
        Duration::from_millis(base_ms.saturating_mul(factor)).min(self.max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backoff() -> Backoff {
        Backoff::new(Duration::from_secs(1), Duration::from_secs(30))
    }

    #[test]
    fn no_failures_means_no_delay() {
        assert_eq!(backoff().delay(0), Duration::ZERO);
    }

    #[test]
    fn first_failure_waits_base() {
        assert_eq!(backoff().delay(1), Duration::from_secs(1));
    }

    #[test]
    fn delay_doubles_each_failure() {
        assert_eq!(backoff().delay(2), Duration::from_secs(2));
        assert_eq!(backoff().delay(3), Duration::from_secs(4));
        assert_eq!(backoff().delay(4), Duration::from_secs(8));
    }

    #[test]
    fn delay_is_capped_at_max() {
        assert_eq!(backoff().delay(10), Duration::from_secs(30));
    }

    #[test]
    fn huge_failure_count_does_not_overflow() {
        assert_eq!(backoff().delay(u32::MAX), Duration::from_secs(30));
    }
}
