//! Pure helpers for the `talk-to` PTY wrapper (IDEAS.md #4).
//!
//! The proxy itself is OS glue (forkpty / termios / poll — not unit-tested);
//! the *decisions* it makes are pure and live here so they're testable with
//! real objects (Chicago-style, no doubles). Two rules matter for the tracer
//! bullet:
//!
//! - **argv split** — `talk-to <program> [args...]` separates the wrapped
//!   program from its arguments. SSH is not special: `talk-to ssh host claude`
//!   wraps `ssh` with args `host claude`.
//! - **the injection invariant** — bytes written into the child PTY never carry
//!   a trailing newline, so the human still reviews and presses Enter. This is
//!   **Auto-type** into a **wrapper sink** (CONTEXT.md): deliver as if typed,
//!   never submit. The debug string and the **Transcript** both go through this
//!   rule.

/// Why `talk-to`'s arguments could not be turned into a command to wrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyError {
    /// `talk-to` was invoked with no command to wrap.
    EmptyCommand,
}

/// Split `talk-to`'s arguments into the wrapped program and its arguments.
///
/// `args` is everything after `talk-to` itself. The first element is the
/// program to `exec`; the rest are passed through verbatim. Because the wrapper
/// wraps whatever command it is given, `ssh host claude` is just `ssh` with args
/// `["host", "claude"]` — no remote machinery, no SSH special-case.
pub fn split_command(args: &[String]) -> Result<(&str, &[String]), PtyError> {
    match args.split_first() {
        Some((program, rest)) => Ok((program.as_str(), rest)),
        None => Err(PtyError::EmptyCommand),
    }
}

/// The bytes to write into the child PTY for `text`, with any trailing carriage
/// returns / newlines stripped.
///
/// This is the load-bearing **review-before-Enter** invariant: injected text
/// (the debug string, a **Transcript**) arrives exactly
/// as if typed but is *never* submitted for you. Internal newlines and trailing
/// spaces are preserved — only a trailing `\r`/`\n` (which would press Enter) is
/// removed.
pub fn injection_bytes(text: &str) -> Vec<u8> {
    text.trim_end_matches(['\r', '\n']).as_bytes().to_vec()
}

/// The terminal/composer **erase** byte: ASCII DEL (`0x7f`), which deletes one
/// *logical character* to the left. The streaming live preview revises in place
/// by erasing the unstable tail and re-typing it; deletes are counted in Unicode
/// codepoints (the ASCII-dominant assumption), since the composer removes logical
/// chars, not bytes — a multi-byte char is one DEL, not its byte count.
const ERASE: u8 = 0x7f;

/// Codepoint count of `text` — the number of [`ERASE`] presses that delete it.
pub fn codepoint_len(text: &str) -> usize {
    text.chars().count()
}

/// Bytes to revise the streaming live preview in place: erase the previous
/// unstable tail (`old_tail_len` logical chars), then type the `newly_committed`
/// text (which becomes part of the stable prefix) followed by the `new_tail`.
///
/// The already-stable prefix (everything committed in *prior* edits) is never
/// touched, so settled words never flicker; only the mutable tail region is
/// rewritten. The committed/tail strings already carry their joining spaces (the
/// commit engine renders them), so this is a pure byte assembly.
pub fn edit_bytes(old_tail_len: usize, newly_committed: &str, new_tail: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(old_tail_len + newly_committed.len() + new_tail.len());
    out.extend(std::iter::repeat_n(ERASE, old_tail_len));
    out.extend_from_slice(newly_committed.as_bytes());
    out.extend_from_slice(new_tail.as_bytes());
    out
}

/// Bytes to **finalize** a streaming dictation: erase the entire current preview
/// (`buffer_len` logical chars = the stable prefix plus the tail) and type the
/// batch-accurate `final_text`, newline-stripped (review-before-Enter). This is
/// the reconcile that replaces the rough live preview with the corrected
/// Transcript — no double-typing.
pub fn finalize_bytes(buffer_len: usize, final_text: &str) -> Vec<u8> {
    let typed = injection_bytes(final_text);
    let mut out = Vec::with_capacity(buffer_len + typed.len());
    out.extend(std::iter::repeat_n(ERASE, buffer_len));
    out.extend_from_slice(&typed);
    out
}

/// Tracks the live preview's stable/tail boundary (in logical chars) so each
/// [`LiveEdit`](crate::streaming::LiveEdit) and the finalize can be turned into
/// the right erase-and-type [byte stream](edit_bytes) for the wrapped composer.
///
/// `stable_len` is everything committed in prior edits (never erased); `tail_len`
/// is the current unstable tail (erased and rewritten each edit). The wrapper
/// holds one per dictation and resets it between dictations.
#[derive(Debug, Clone, Default)]
pub struct PreviewCursor {
    stable_len: usize,
    tail_len: usize,
}

impl PreviewCursor {
    /// A fresh cursor with an empty preview.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total logical chars currently displayed (stable prefix + unstable tail).
    pub fn preview_len(&self) -> usize {
        self.stable_len + self.tail_len
    }

    /// Apply one live edit: erase the old tail, type the newly-committed text
    /// (folded into the stable prefix) and the new tail. Returns the bytes to
    /// write into the composer.
    pub fn apply_edit(&mut self, committed: &str, tail: &str) -> Vec<u8> {
        let bytes = edit_bytes(self.tail_len, committed, tail);
        self.stable_len += codepoint_len(committed);
        self.tail_len = codepoint_len(tail);
        bytes
    }

    /// Finalize: erase the whole preview and type `final_text`. Resets the cursor
    /// to empty (the dictation is over).
    pub fn finalize(&mut self, final_text: &str) -> Vec<u8> {
        let bytes = finalize_bytes(self.preview_len(), final_text);
        self.stable_len = 0;
        self.tail_len = 0;
        bytes
    }

    /// Reset to an empty preview (a new dictation begins).
    pub fn reset(&mut self) {
        self.stable_len = 0;
        self.tail_len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn split_command_takes_the_first_arg_as_the_program() {
        let args = owned(&["claude"]);
        let (program, rest) = split_command(&args).unwrap();
        assert_eq!(program, "claude");
        assert!(rest.is_empty());
    }

    #[test]
    fn split_command_passes_ssh_args_through_verbatim() {
        // SSH is just a command: `talk-to ssh host claude` wraps ssh.
        let args = owned(&["ssh", "host", "claude"]);
        let (program, rest) = split_command(&args).unwrap();
        assert_eq!(program, "ssh");
        assert_eq!(rest, &owned(&["host", "claude"])[..]);
    }

    #[test]
    fn split_command_rejects_an_empty_invocation() {
        assert_eq!(split_command(&[]), Err(PtyError::EmptyCommand));
    }

    #[test]
    fn injection_never_carries_a_trailing_newline() {
        // Review-before-Enter: a trailing newline would submit the line.
        for raw in ["hello", "hello\n", "hello\r\n", "hello\n\n"] {
            let bytes = injection_bytes(raw);
            assert!(
                !bytes.ends_with(b"\n") && !bytes.ends_with(b"\r"),
                "injection of {raw:?} must not end in CR/LF",
            );
            assert_eq!(bytes, b"hello");
        }
    }

    #[test]
    fn injection_preserves_internal_spacing_and_trailing_spaces() {
        assert_eq!(injection_bytes("write a function  "), b"write a function  ");
        assert_eq!(injection_bytes("a b\tc"), b"a b\tc");
    }

    // ---- streaming live-preview edit bytes ------------------------------

    /// A stand-in for the wrapped composer's line editor: a buffer of logical
    /// chars where the ERASE byte (`0x7f`) deletes one char from the end and every
    /// other byte is UTF-8 text appended. Independent of how `edit_bytes` is built
    /// — it models *only* "DEL deletes one logical char" — so driving the real
    /// edit bytes through it proves they reproduce the intended preview (the same
    /// shape as a readline/bash line editor; the real Claude composer is the
    /// human's one-time manual smoke-test).
    #[derive(Default)]
    struct LineEditor {
        chars: Vec<char>,
    }

    impl LineEditor {
        fn apply(&mut self, bytes: &[u8]) {
            // Split the byte stream into ERASE controls and UTF-8 text runs.
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == 0x7f {
                    self.chars.pop();
                    i += 1;
                } else {
                    let start = i;
                    while i < bytes.len() && bytes[i] != 0x7f {
                        i += 1;
                    }
                    let text = std::str::from_utf8(&bytes[start..i]).unwrap();
                    self.chars.extend(text.chars());
                }
            }
        }

        fn text(&self) -> String {
            self.chars.iter().collect()
        }
    }

    #[test]
    fn edit_bytes_erase_the_old_tail_then_type_committed_and_new_tail() {
        // Erase 4 logical chars, then type the committed chunk and the new tail.
        let bytes = edit_bytes(4, " onto", " main");
        assert_eq!(&bytes[..4], &[0x7f, 0x7f, 0x7f, 0x7f]);
        assert_eq!(&bytes[4..], b" onto main");
    }

    #[test]
    fn deletes_count_unicode_codepoints_not_bytes() {
        // A 3-char tail of multi-byte chars is erased with 3 DELs, not 6+ — the
        // composer deletes logical characters.
        assert_eq!(codepoint_len("café"), 4);
        assert_eq!(codepoint_len("→ x"), 3);
        let bytes = edit_bytes(codepoint_len("café"), "", "");
        assert_eq!(bytes, vec![0x7f, 0x7f, 0x7f, 0x7f]);
    }

    #[test]
    fn a_monotonic_dictation_only_appends_and_never_flickers_the_stable_prefix() {
        // "rebase onto main" arrives word by word; once a word commits it is never
        // erased again. Driving the edits through the stand-in line editor must
        // reproduce the growing prompt with no stable-prefix churn.
        let mut cursor = PreviewCursor::new();
        let mut editor = LineEditor::default();

        // (committed, tail) as the commit engine renders them, decode by decode.
        let edits = [
            ("", "rebase"),
            ("rebase", " onto"),
            (" onto", " main"),
            (" main", ""),
        ];
        let expect = [
            "rebase",
            "rebase onto",
            "rebase onto main",
            "rebase onto main",
        ];
        for ((committed, tail), want) in edits.into_iter().zip(expect) {
            editor.apply(&cursor.apply_edit(committed, tail));
            assert_eq!(editor.text(), want);
        }
        // The settled words are all there; the stable prefix never lost a char.
        assert_eq!(editor.text(), "rebase onto main");
    }

    #[test]
    fn a_tail_revision_rewrites_only_the_tail_leaving_the_stable_prefix_intact() {
        // Whisper mishears the tail ("auto") then corrects it ("onto"); only the
        // tail region is erased and rewritten — "rebase" (committed) is untouched.
        let mut cursor = PreviewCursor::new();
        let mut editor = LineEditor::default();

        editor.apply(&cursor.apply_edit("", "rebase auto"));
        assert_eq!(editor.text(), "rebase auto");
        // "rebase" commits, "auto" → "onto" in the tail.
        let edit = cursor.apply_edit("rebase", " onto");
        // Only the 11-char old tail "rebase auto" is erased (not into the prefix).
        assert_eq!(edit.iter().filter(|&&b| b == 0x7f).count(), 11);
        editor.apply(&edit);
        assert_eq!(editor.text(), "rebase onto");
    }

    #[test]
    fn finalize_erases_the_whole_preview_and_types_the_reconciled_transcript() {
        // The rough preview "run why do tool" is replaced wholesale by the
        // batch-accurate, newline-stripped Transcript — no double-typing.
        let mut cursor = PreviewCursor::new();
        let mut editor = LineEditor::default();
        editor.apply(&cursor.apply_edit("", "run why do tool"));
        assert_eq!(editor.text(), "run why do tool");
        assert_eq!(cursor.preview_len(), 15);

        editor.apply(&cursor.finalize("run ydotool\n"));
        assert_eq!(
            editor.text(),
            "run ydotool",
            "preview replaced, no trailing Enter"
        );
        assert_eq!(cursor.preview_len(), 0, "cursor resets after finalize");
    }

    #[test]
    fn cancel_erases_the_whole_preview_and_types_nothing() {
        // `cancel` during a dictation delivers an empty finalize: the whole rough
        // preview is backspaced and nothing is typed in its place — the prompt is
        // left clean and nothing is delivered.
        let mut cursor = PreviewCursor::new();
        let mut editor = LineEditor::default();
        editor.apply(&cursor.apply_edit("", "rebase onto"));
        assert_eq!(editor.text(), "rebase onto");

        let erase = cursor.finalize("");
        assert_eq!(
            erase,
            vec![0x7f; 11],
            "erases the 11-char preview, types nothing"
        );
        editor.apply(&erase);
        assert_eq!(editor.text(), "", "the streaming buffer is erased");
        assert_eq!(cursor.preview_len(), 0);
    }

    #[test]
    fn reset_clears_the_cursor_between_dictations() {
        let mut cursor = PreviewCursor::new();
        cursor.apply_edit("", "leftover tail");
        assert!(cursor.preview_len() > 0);
        cursor.reset();
        assert_eq!(cursor.preview_len(), 0);
        // A fresh dictation starts from an empty preview (no stray backspaces).
        let bytes = cursor.apply_edit("", "fresh");
        assert!(!bytes.contains(&0x7f));
    }
}
