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

/// Group consecutive clips so each transcribed segment clears the `min`
/// duration: a clip shorter than `min` folds forward into the following clip(s)
/// until the running group total reaches `min`. The micro-pause / stutter guard
/// (story 7) — tiny fragments are not sent to Whisper alone, where they invite
/// hallucinations. A trailing short group has nothing to fold into, so it is
/// still emitted (the session simply ended on a short clip). Returns the ordered
/// groups of clip indices to transcribe as single batch jobs.
pub fn accumulate_clips(durations: &[Duration], min: Duration) -> Vec<Vec<usize>> {
    let mut groups = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut running = Duration::ZERO;
    for (i, &d) in durations.iter().enumerate() {
        current.push(i);
        running += d;
        if running >= min {
            groups.push(std::mem::take(&mut current));
            running = Duration::ZERO;
        }
    }
    // Flush a trailing short group (the session ended mid-accumulation).
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// How many of the `present` clip files are finalized (complete) and safe to
/// transcribe, given whether `sox` is still recording. While sox runs, the
/// newest clip is still being written, so only clip N is complete once clip N+1
/// has opened — i.e. all but the last. Once sox has exited (session end or
/// stop/cancel) it has flushed the final clip, so every present clip is
/// finalized. The daemon polls the session dir and feeds this to its serial
/// transcription queue.
pub fn finalized_clip_count(present: usize, sox_running: bool) -> usize {
    if sox_running {
        present.saturating_sub(1)
    } else {
        present
    }
}

/// An in-progress Continuous-mode session: the ordered clip transcripts as they
/// finalize, assembled in record order and used to seed each next clip's
/// `initial_prompt` (prompt chaining) from the running transcript tail.
///
/// Pure accumulator — the daemon transcribes clips serially (one GPU, so order
/// and chaining are free) and pushes each transcript here; on session end it
/// reads [`assembled`](Session::assembled) once to deliver.
#[derive(Debug, Clone)]
pub struct Session {
    clips: Vec<String>,
    chain_words: usize,
}

impl Session {
    /// A fresh session whose prompt chaining carries the last `chain_words`
    /// words of prior transcript into each next clip.
    pub fn new(chain_words: usize) -> Self {
        Self {
            clips: Vec::new(),
            chain_words,
        }
    }

    /// Record clip `n`'s transcript in order (empty/whitespace is kept as a
    /// placeholder slot but dropped from assembly, leaving no gap).
    pub fn push_clip(&mut self, transcript: &str) {
        self.clips.push(transcript.to_owned());
    }

    /// How many clip transcripts have been recorded so far.
    pub fn clip_count(&self) -> usize {
        self.clips.len()
    }

    /// The `initial_prompt` for the next clip: the tail of everything
    /// transcribed so far, bounded to `chain_words`. Empty before the first clip.
    pub fn prompt_for_next(&self) -> String {
        chain_tail(&self.assembled(), self.chain_words)
    }

    /// The full session transcript: clip transcripts joined in record order,
    /// empties dropped. Read once on session end to deliver.
    pub fn assembled(&self) -> String {
        assemble(&self.clips)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIP: Duration = Duration::from_millis(1000);
    const END: Duration = Duration::from_secs(10);

    const MIN: Duration = Duration::from_secs(2);

    // ---- min-clip accumulation ------------------------------------------

    #[test]
    fn clips_at_or_above_min_each_stand_alone() {
        // Three healthy clips -> three single-clip groups, in order.
        let durs = [
            Duration::from_secs(3),
            Duration::from_secs(2),
            Duration::from_secs(5),
        ];
        assert_eq!(
            accumulate_clips(&durs, MIN),
            vec![vec![0], vec![1], vec![2]]
        );
    }

    #[test]
    fn a_short_clip_folds_into_the_following_clip() {
        // Clip 0 is a 1 s stutter below the 2 s floor: it accumulates with the
        // next clip so Whisper gets one healthy segment, not a tiny fragment.
        let durs = [
            Duration::from_millis(1000),
            Duration::from_secs(3),
            Duration::from_secs(4),
        ];
        assert_eq!(accumulate_clips(&durs, MIN), vec![vec![0, 1], vec![2]]);
    }

    #[test]
    fn consecutive_short_clips_all_fold_forward_together() {
        // Two micro-pauses in a row keep accumulating until the running total
        // clears the floor.
        let durs = [
            Duration::from_millis(500),
            Duration::from_millis(800),
            Duration::from_secs(3),
        ];
        assert_eq!(accumulate_clips(&durs, MIN), vec![vec![0, 1, 2]]);
    }

    #[test]
    fn a_trailing_short_clip_is_still_emitted_as_its_own_group() {
        // The final clip can be short (the session ended): it has nothing to
        // fold into, so it is transcribed on its own rather than dropped.
        let durs = [Duration::from_secs(3), Duration::from_millis(800)];
        assert_eq!(accumulate_clips(&durs, MIN), vec![vec![0], vec![1]]);
    }

    #[test]
    fn no_clips_accumulate_to_nothing() {
        assert_eq!(accumulate_clips(&[], MIN), Vec::<Vec<usize>>::new());
    }

    // ---- clip finalization (dir-watch) ---------------------------------

    #[test]
    fn while_recording_all_but_the_newest_clip_are_finalized() {
        // sox is still writing the newest clip; clip N is only complete once
        // clip N+1 has been opened. With 3 clips present and sox running, the
        // first 2 are safe to transcribe; the 3rd is still being written.
        assert_eq!(finalized_clip_count(3, true), 2);
    }

    #[test]
    fn no_clip_is_finalized_until_a_second_one_opens() {
        // A single clip present while sox runs is still in progress.
        assert_eq!(finalized_clip_count(1, true), 0);
        assert_eq!(finalized_clip_count(0, true), 0);
    }

    #[test]
    fn when_sox_has_exited_every_clip_is_finalized() {
        // Session end / stop: sox flushed the last clip, so all present clips
        // are complete and ready to transcribe.
        assert_eq!(finalized_clip_count(3, false), 3);
        assert_eq!(finalized_clip_count(0, false), 0);
    }

    // ---- Session: ordered assembly + prompt chaining --------------------

    #[test]
    fn an_empty_session_assembles_to_nothing_and_seeds_no_prompt() {
        let s = Session::new(3);
        assert_eq!(s.assembled(), "");
        assert_eq!(s.prompt_for_next(), "");
    }

    #[test]
    fn pushing_clip_transcripts_assembles_them_in_record_order() {
        let mut s = Session::new(3);
        s.push_clip("first part");
        s.push_clip("second part");
        s.push_clip("third part");
        assert_eq!(s.assembled(), "first part second part third part");
    }

    #[test]
    fn prompt_for_next_is_the_tail_of_what_has_been_transcribed_so_far() {
        // Each clip seeds the next: the running prompt is the tail of all prior
        // clip transcripts, bounded to the chain word count (3 here).
        let mut s = Session::new(3);
        s.push_clip("now rebase onto the main branch");
        assert_eq!(s.prompt_for_next(), "the main branch");
        // The tail spans the whole running transcript, so after the second clip
        // it is the last 3 words of "...the main branch then force push it".
        s.push_clip("then force push it");
        assert_eq!(s.prompt_for_next(), "force push it");
    }

    #[test]
    fn empty_clip_transcripts_are_dropped_from_assembly() {
        // A clip that came back empty (silence/hallucination filtered) leaves no
        // gap in the assembled text.
        let mut s = Session::new(3);
        s.push_clip("hello there");
        s.push_clip("   ");
        s.push_clip("general kenobi");
        assert_eq!(s.assembled(), "hello there general kenobi");
    }

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
