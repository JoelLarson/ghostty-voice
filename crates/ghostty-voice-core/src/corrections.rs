//! Correction dictionary (S4) — a deterministic jargon spell-fixer.
//!
//! Case-insensitive (ASCII), whole-word find/replace for terms Whisper
//! reliably mishears the same way (`"why do tool" → "ydotool"`). Explicitly
//! NOT a code-symbol munger. Apply order is the caller's (longest-first is
//! recommended so phrases win over their substrings).

/// Apply each `(from, to)` correction to `text`, in the given order.
pub fn apply_corrections(text: &str, corrections: &[(String, String)]) -> String {
    let mut out = text.to_owned();
    for (from, to) in corrections {
        out = replace_word_ci(&out, from, to);
    }
    out
}

/// Replace whole-word, case-insensitive (ASCII) occurrences of `from` with
/// `to`. Word boundaries are ASCII alphanumerics/underscore, so `"ghosty"`
/// matches the word but not inside `"ghostyish"`.
fn replace_word_ci(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_owned();
    }
    let haystack = text.to_ascii_lowercase();
    let needle = from.to_ascii_lowercase();
    let bytes = text.as_bytes();

    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut from_idx = 0;
    while let Some(rel) = haystack[from_idx..].find(&needle) {
        let start = from_idx + rel;
        let end = start + needle.len();
        let left_boundary = start == 0 || !is_word_byte(bytes[start - 1]);
        let right_boundary = end == bytes.len() || !is_word_byte(bytes[end]);
        if left_boundary && right_boundary {
            out.push_str(&text[last..start]);
            out.push_str(to);
            last = end;
            from_idx = end;
        } else {
            from_idx = start + 1;
        }
    }
    out.push_str(&text[last..]);
    out
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corr(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(f, t)| ((*f).to_owned(), (*t).to_owned()))
            .collect()
    }

    #[test]
    fn replaces_a_misheard_phrase() {
        let out = apply_corrections("run why do tool now", &corr(&[("why do tool", "ydotool")]));
        assert_eq!(out, "run ydotool now");
    }

    #[test]
    fn matching_is_case_insensitive_replacement_is_fixed() {
        let out = apply_corrections("Ghosty and GHOSTY", &corr(&[("ghosty", "Ghostty")]));
        assert_eq!(out, "Ghostty and Ghostty");
    }

    #[test]
    fn does_not_match_inside_a_larger_word() {
        let out = apply_corrections("ghostyish", &corr(&[("ghosty", "Ghostty")]));
        assert_eq!(out, "ghostyish");
    }

    #[test]
    fn applies_corrections_in_order() {
        // Longest-first: the phrase rule consumes "git" before the word rule runs.
        let out = apply_corrections(
            "git stash please",
            &corr(&[("git stash", "stash-changes"), ("git", "GIT")]),
        );
        assert_eq!(out, "stash-changes please");
    }

    #[test]
    fn no_corrections_is_identity() {
        assert_eq!(apply_corrections("untouched text", &[]), "untouched text");
    }
}
