//! `initial_prompt` builder (S4).
//!
//! Whisper's `initial_prompt` is capped (~224 tokens); a growing vocab list
//! would silently truncate past it, dropping later terms. This builder bounds
//! the vocab to a token budget and reports whether it had to truncate, so the
//! caller can warn instead of silently losing biasing.
//!
//! Token counting is approximate (no in-process tokenizer): whitespace/comma
//! separated chunks. Keep the budget conservative.

/// A built prompt and whether the vocab was truncated to fit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialPrompt {
    pub text: String,
    pub truncated: bool,
}

/// Build `"<prefix> Vocabulary: term1, term2, …"`, including as many vocab
/// terms as fit within `max_tokens`.
pub fn build_initial_prompt(prefix: &str, vocab: &[String], max_tokens: usize) -> InitialPrompt {
    let mut text = prefix.to_owned();
    if !vocab.is_empty() {
        text.push_str(" Vocabulary:");
    }

    let mut truncated = false;
    for term in vocab {
        let candidate = format!("{text} {term},");
        if estimate_tokens(&candidate) > max_tokens {
            truncated = true;
            break;
        }
        text = candidate;
    }

    InitialPrompt {
        text: text.trim_end_matches(',').to_owned(),
        truncated,
    }
}

fn estimate_tokens(s: &str) -> usize {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|chunk| !chunk.is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab(terms: &[&str]) -> Vec<String> {
        terms.iter().map(|t| (*t).to_owned()).collect()
    }

    const PREFIX: &str = "Transcript of technical instructions.";

    #[test]
    fn includes_all_vocab_under_the_cap() {
        let p = build_initial_prompt(PREFIX, &vocab(&["ydotool", "Ghostty", "kubectl"]), 50);
        assert!(!p.truncated);
        assert!(p.text.contains("ydotool"));
        assert!(p.text.contains("kubectl"));
        assert!(p.text.starts_with(PREFIX));
    }

    #[test]
    fn truncates_and_flags_when_over_the_cap() {
        let p = build_initial_prompt(PREFIX, &vocab(&["ydotool", "Ghostty", "kubectl"]), 6);
        assert!(p.truncated);
        assert!(p.text.contains("ydotool"));
        assert!(!p.text.contains("Ghostty"));
        assert!(!p.text.ends_with(','));
    }

    #[test]
    fn empty_vocab_yields_just_the_prefix() {
        let p = build_initial_prompt(PREFIX, &[], 50);
        assert_eq!(p.text, PREFIX);
        assert!(!p.truncated);
    }
}
