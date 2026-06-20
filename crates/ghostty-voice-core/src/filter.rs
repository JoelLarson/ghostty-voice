//! Transcript filtering (S4): discard empty audio, sub-minimum-duration
//! recordings, and Whisper's stock silence hallucinations so nothing junk is
//! typed.

use std::time::Duration;

/// Phrases Whisper reliably hallucinates from silence/near-silence.
const HALLUCINATIONS: &[&str] = &[
    "[blank_audio]",
    "[silence]",
    "thank you.",
    "thank you",
    "thanks for watching.",
    "thanks for watching",
    "you",
    ".",
];

/// Should this transcript be discarded (typed as nothing)?
pub fn should_discard(transcript: &str, audio_duration: Duration, min_duration: Duration) -> bool {
    if audio_duration < min_duration {
        return true;
    }
    let trimmed = transcript.trim();
    if trimmed.is_empty() {
        return true;
    }
    HALLUCINATIONS.contains(&trimmed.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_millis(300);
    const OK: Duration = Duration::from_secs(3);

    #[test]
    fn empty_or_whitespace_is_discarded() {
        assert!(should_discard("", OK, MIN));
        assert!(should_discard("   \n ", OK, MIN));
    }

    #[test]
    fn known_hallucinations_are_discarded_case_insensitively() {
        assert!(should_discard("[BLANK_AUDIO]", OK, MIN));
        assert!(should_discard("Thank you.", OK, MIN));
        assert!(should_discard("thanks for watching", OK, MIN));
    }

    #[test]
    fn sub_minimum_duration_is_discarded_even_with_text() {
        assert!(should_discard(
            "rebase onto main",
            Duration::from_millis(100),
            MIN
        ));
    }

    #[test]
    fn real_speech_of_adequate_length_is_kept() {
        assert!(!should_discard("rebase onto main", OK, MIN));
    }
}
