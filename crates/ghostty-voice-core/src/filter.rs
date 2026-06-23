//! Transcript filtering: discard empty audio, sub-minimum-duration
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

/// Whisper's native capture format: 16 kHz, mono, signed-16-bit PCM.
const SAMPLE_RATE_HZ: u64 = 16_000;
const BYTES_PER_SAMPLE: u64 = 2;

/// Duration of `data_bytes` of 16 kHz mono s16 PCM. Lets the daemon derive a
/// recording's length from the WAV `data` chunk size without a WAV decoder.
pub fn pcm_duration(data_bytes: u64) -> Duration {
    let samples = data_bytes / BYTES_PER_SAMPLE;
    Duration::from_nanos(samples * 1_000_000_000 / SAMPLE_RATE_HZ)
}

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

    #[test]
    fn pcm_duration_one_second_is_16k_samples() {
        // 16000 samples * 2 bytes = 32000 bytes per second.
        assert_eq!(pcm_duration(32_000), Duration::from_secs(1));
    }

    #[test]
    fn pcm_duration_below_min_flags_discard() {
        // 0.2 s of audio (6400 bytes) is under the 0.3 s minimum.
        let dur = pcm_duration(6_400);
        assert!(dur < MIN);
        assert!(should_discard("a real word", dur, MIN));
    }

    #[test]
    fn pcm_duration_empty_is_zero() {
        assert_eq!(pcm_duration(0), Duration::ZERO);
    }
}
