//! Continuous-mode session logic (S6) — the pure core.
//!
//! A Session is an ordered sequence of silence-bounded Clips. Short pauses cut
//! clips; a long silence ends the session. Clip transcripts are assembled, in
//! order, into one transcript, and each clip seeds the next with the tail of
//! its transcript (prompt chaining) to retain cross-clip context.

use std::time::Duration;

/// What a detected silence means during a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilenceEvent {
    /// Below the clip-cut threshold — keep recording the current clip.
    Continue,
    /// Past the clip-cut pause — close the current clip, start the next.
    CutClip,
    /// Past the session-end silence — finish and deliver.
    EndSession,
}

/// Classify a silence by the two thresholds (session-end wins over clip-cut).
pub fn classify_silence(
    silence: Duration,
    clip_pause: Duration,
    session_end: Duration,
) -> SilenceEvent {
    if silence >= session_end {
        SilenceEvent::EndSession
    } else if silence >= clip_pause {
        SilenceEvent::CutClip
    } else {
        SilenceEvent::Continue
    }
}

/// Assemble clip transcripts, in record order, into one session transcript.
pub fn assemble(clips: &[String]) -> String {
    clips
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The last `max_words` of `prev`, to seed the next clip's `initial_prompt`.
pub fn chain_tail(prev: &str, max_words: usize) -> String {
    let words: Vec<&str> = prev.split_whitespace().collect();
    let start = words.len().saturating_sub(max_words);
    words[start..].join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIP: Duration = Duration::from_millis(1000);
    const END: Duration = Duration::from_secs(10);

    #[test]
    fn short_pause_continues_the_clip() {
        assert_eq!(
            classify_silence(Duration::from_millis(400), CLIP, END),
            SilenceEvent::Continue,
        );
    }

    #[test]
    fn medium_pause_cuts_a_clip() {
        assert_eq!(
            classify_silence(Duration::from_millis(1200), CLIP, END),
            SilenceEvent::CutClip,
        );
    }

    #[test]
    fn long_silence_ends_the_session() {
        assert_eq!(
            classify_silence(Duration::from_secs(11), CLIP, END),
            SilenceEvent::EndSession,
        );
    }

    #[test]
    fn assemble_joins_clips_in_order_dropping_empties() {
        let clips = vec![
            "first part".to_owned(),
            "  ".to_owned(),
            "second part".to_owned(),
        ];
        assert_eq!(assemble(&clips), "first part second part");
    }

    #[test]
    fn chain_tail_returns_the_last_words() {
        assert_eq!(
            chain_tail("now rebase onto the main branch", 3),
            "the main branch"
        );
        assert_eq!(chain_tail("short", 5), "short");
    }
}
