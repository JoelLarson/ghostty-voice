//! Post-transcription accuracy pipeline (S4).
//!
//! The single pure decision the daemon makes after whisper-server replies:
//! discard junk (empty audio, sub-min-duration blips, known hallucinations) so
//! nothing is typed, otherwise apply the deterministic jargon correction
//! dictionary to the surviving transcript. Keeping it here lets the daemon stay
//! a thin shell and lets this be unit-tested with real objects.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::corrections::{apply_corrections, ordered_corrections};
use crate::filter::should_discard;

/// Turn a raw transcript into the text to type, or `None` to type nothing.
///
/// `audio_duration` is the recording's length and `min_duration` the discard
/// floor; `corrections` is the `[corrections]` table. Discarded transcripts
/// (empty/hallucination/too-short) return `None`; survivors are corrected.
pub fn finalize_transcript(
    raw: &str,
    audio_duration: Duration,
    min_duration: Duration,
    corrections: &BTreeMap<String, String>,
) -> Option<String> {
    if should_discard(raw, audio_duration, min_duration) {
        return None;
    }
    let ordered = ordered_corrections(corrections);
    Some(apply_corrections(raw, &ordered))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_millis(300);
    const OK: Duration = Duration::from_secs(3);

    fn table(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn corrects_jargon_in_a_kept_transcript() {
        let out = finalize_transcript(
            "run why do tool on ghosty",
            OK,
            MIN,
            &table(&[("why do tool", "ydotool"), ("ghosty", "Ghostty")]),
        );
        assert_eq!(out.as_deref(), Some("run ydotool on Ghostty"));
    }

    #[test]
    fn discards_empty_audio() {
        assert_eq!(finalize_transcript("", OK, MIN, &BTreeMap::new()), None);
    }

    #[test]
    fn discards_a_known_hallucination() {
        assert_eq!(
            finalize_transcript("Thank you.", OK, MIN, &BTreeMap::new()),
            None
        );
    }

    #[test]
    fn discards_sub_min_duration_even_with_real_words() {
        assert_eq!(
            finalize_transcript(
                "rebase onto main",
                Duration::from_millis(100),
                MIN,
                &BTreeMap::new()
            ),
            None
        );
    }

    #[test]
    fn keeps_real_speech_without_corrections_unchanged() {
        assert_eq!(
            finalize_transcript("rebase onto main", OK, MIN, &BTreeMap::new()).as_deref(),
            Some("rebase onto main"),
        );
    }
}
