//! Streaming-dictation commit engine — the pure LocalAgreement-2 core.
//!
//! The live preview is a *rough* draft (the conscious extension of ADR-0002):
//! whisper-server re-decodes the growing capture and each pass may revise the
//! most recent words. To keep settled words from flickering while the tail still
//! churns, the engine applies **LocalAgreement-2**: a word is **committed** (made
//! part of the stable prefix, never to change) once **two consecutive decodes
//! agree** on it at the same position. Everything past the committed boundary is
//! the **unstable tail**, rewritten in place each decode.
//!
//! Pure word-list math: feed successive hypotheses, get back the *newly-committed*
//! text to lock onto the prompt and the *current tail* to (re)display. The stable
//! prefix only ever grows. The daemon is thin glue; this owns the policy.

/// One incremental edit to apply to the live preview in the prompt.
///
/// Both fields are rendered with their joining spaces so the wrapper can apply
/// them verbatim: `committed` carries a leading space when it follows existing
/// committed text, and `tail` carries a leading space when any committed text
/// precedes it. Either may be empty (no new commit this decode, or an empty tail).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveEdit {
    /// Newly-committed text to lock onto the stable prefix. Once typed, it is
    /// never rewritten.
    pub committed: String,
    /// The current unstable tail rendering, rewritten in place each decode.
    pub tail: String,
}

/// LocalAgreement-2 commit engine over a single streaming dictation.
#[derive(Debug, Clone, Default)]
pub struct CommitEngine {
    /// Words confirmed by two agreeing decodes — the stable prefix, never revised.
    committed: Vec<String>,
    /// The previous decode's full hypothesis, for the next agreement check.
    prev: Vec<String>,
}

impl CommitEngine {
    /// A fresh engine with nothing committed.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many words have been committed (the stable prefix length).
    pub fn committed_len(&self) -> usize {
        self.committed.len()
    }

    /// The full committed text so far (space-joined). The settled portion of the
    /// preview — used when reasoning about the whole current preview.
    pub fn committed_text(&self) -> String {
        self.committed.join(" ")
    }

    /// Observe the next decode's `hypothesis` (whitespace-split words) and return
    /// the [`LiveEdit`] to apply: any words now confirmed by agreement with the
    /// previous decode become newly committed, and the rest is the unstable tail.
    ///
    /// LocalAgreement-2: beyond the already-committed prefix, the longest run of
    /// words on which this decode and the previous one agree is committed. The
    /// committed prefix never retracts; an unchanged hypothesis commits nothing
    /// new and re-renders the same tail (idempotent).
    pub fn observe(&mut self, hypothesis: &[&str]) -> LiveEdit {
        let k = self.committed.len();
        // Agreement in the fresh region beyond the committed prefix only.
        let agree = if hypothesis.len() > k {
            agree_len(self.prev.get(k..).unwrap_or(&[]), &hypothesis[k..])
        } else {
            0
        };

        let had_committed = k > 0;
        let newly: Vec<String> = hypothesis[k.min(hypothesis.len())..]
            .iter()
            .take(agree)
            .map(|s| (*s).to_owned())
            .collect();
        for word in &newly {
            self.committed.push(word.clone());
        }

        let tail_words: Vec<&str> = hypothesis.iter().skip(k + agree).copied().collect();

        let committed = render(
            &newly.iter().map(String::as_str).collect::<Vec<_>>(),
            had_committed,
        );
        let tail = render(&tail_words, !self.committed.is_empty());

        self.prev = hypothesis.iter().map(|s| (*s).to_owned()).collect();
        LiveEdit { committed, tail }
    }
}

/// Length of the common prefix of a previous hypothesis (owned words) and the
/// current one (borrowed words).
fn agree_len(prev: &[String], cur: &[&str]) -> usize {
    prev.iter()
        .zip(cur)
        .take_while(|(p, c)| p.as_str() == **c)
        .count()
}

/// Render `words` space-joined, prefixed with a single separating space when
/// `leading_space` (the words follow existing preview text). Empty in, empty out.
fn render(words: &[&str], leading_space: bool) -> String {
    if words.is_empty() {
        return String::new();
    }
    let joined = words.join(" ");
    if leading_space {
        format!(" {joined}")
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a sequence of hypotheses and collect the live edits.
    fn run(decodes: &[&[&str]]) -> Vec<LiveEdit> {
        let mut engine = CommitEngine::new();
        decodes.iter().map(|h| engine.observe(h)).collect()
    }

    #[test]
    fn a_word_commits_one_decode_after_two_passes_agree() {
        // "rebase onto main" arrives word by word; each word is committed exactly
        // one decode after it first appears and is re-confirmed.
        let edits = run(&[
            &["rebase"],
            &["rebase", "onto"],
            &["rebase", "onto", "main"],
            &["rebase", "onto", "main"],
        ]);
        // First sighting: nothing committed yet, the word is the unstable tail.
        assert_eq!(
            edits[0],
            LiveEdit {
                committed: String::new(),
                tail: "rebase".to_owned()
            }
        );
        // "rebase" now agrees twice ⇒ committed; "onto" is the new tail.
        assert_eq!(
            edits[1],
            LiveEdit {
                committed: "rebase".to_owned(),
                tail: " onto".to_owned()
            }
        );
        // "onto" confirmed ⇒ committed (leading space); "main" is the tail.
        assert_eq!(
            edits[2],
            LiveEdit {
                committed: " onto".to_owned(),
                tail: " main".to_owned()
            }
        );
        // "main" confirmed ⇒ committed; tail empties.
        assert_eq!(
            edits[3],
            LiveEdit {
                committed: " main".to_owned(),
                tail: String::new()
            }
        );
    }

    #[test]
    fn the_committed_prefix_grows_monotonically_and_never_retracts() {
        let mut engine = CommitEngine::new();
        engine.observe(&["rebase"]);
        engine.observe(&["rebase", "onto"]);
        assert_eq!(engine.committed_text(), "rebase");
        engine.observe(&["rebase", "onto", "main"]);
        assert_eq!(engine.committed_text(), "rebase onto");
        // A later decode can never shrink the committed prefix.
        engine.observe(&["rebase", "onto", "main", "branch"]);
        assert_eq!(engine.committed_text(), "rebase onto main");
        assert!(engine.committed_len() >= 3);
    }

    #[test]
    fn a_tail_revision_before_commit_does_not_pollute_the_stable_prefix() {
        // Whisper first mishears the tail ("auto"), then corrects it ("onto").
        // Because the two decodes disagreed on that word, it was never committed,
        // so the correction lands cleanly and the stable prefix stays right.
        let mut engine = CommitEngine::new();
        let e0 = engine.observe(&["rebase", "auto"]);
        assert_eq!(e0.committed, "");
        assert_eq!(e0.tail, "rebase auto");
        let e1 = engine.observe(&["rebase", "onto"]);
        // "rebase" agreed across both decodes ⇒ committed; the revised word is the
        // new tail — no "auto" ever leaked into the committed prefix.
        assert_eq!(e1.committed, "rebase");
        assert_eq!(e1.tail, " onto");
        assert_eq!(engine.committed_text(), "rebase");
    }

    #[test]
    fn re_observing_an_unchanged_hypothesis_is_idempotent() {
        let mut engine = CommitEngine::new();
        engine.observe(&["alpha", "beta"]);
        // Second identical decode confirms both words (two passes agree).
        let second = engine.observe(&["alpha", "beta"]);
        assert_eq!(second.committed, "alpha beta");
        assert_eq!(second.tail, "");
        // A third identical decode has nothing new to commit and an empty tail —
        // idempotent re-emit.
        let third = engine.observe(&["alpha", "beta"]);
        assert_eq!(
            third,
            LiveEdit {
                committed: String::new(),
                tail: String::new()
            }
        );
        assert_eq!(engine.committed_text(), "alpha beta");
    }

    #[test]
    fn an_empty_decode_commits_nothing() {
        let mut engine = CommitEngine::new();
        let e = engine.observe(&[]);
        assert_eq!(e, LiveEdit::default());
        assert_eq!(engine.committed_len(), 0);
    }

    #[test]
    fn the_first_committed_word_has_no_leading_space_but_later_ones_do() {
        // Rendering discipline: the very first committed word starts the prompt
        // text (no leading space); every later committed chunk and the tail join
        // with a single separating space.
        let edits = run(&[&["one"], &["one", "two"], &["one", "two", "three"]]);
        assert_eq!(edits[0].tail, "one"); // starts the line
        assert_eq!(edits[1].committed, "one"); // first commit, no leading space
        assert_eq!(edits[1].tail, " two"); // tail joins committed
        assert_eq!(edits[2].committed, " two"); // later commit, leading space
    }
}
