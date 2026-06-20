//! Build the `ydotool type` argument vector — the injection-safety logic.
//!
//! Producing argv directly (no shell) and passing the transcript as a single
//! `--`-terminated argument guarantees no part of it is interpreted as a flag
//! or a shell token, even if it begins with `-` or contains quotes/`$`/`;`.
//!
//! Note: the transcript must already be newline-free (the response parser's
//! job) — a stray `\n` would type as Enter, which would submit the prompt.

/// Build the argument vector (after the `ydotool` binary) to type `text` with
/// the given inter-key delay.
pub fn type_command(text: &str, key_delay_ms: u32) -> Vec<String> {
    vec![
        "type".to_owned(),
        "--key-delay".to_owned(),
        key_delay_ms.to_string(),
        "--".to_owned(),
        text.to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_type_argv_with_terminator() {
        assert_eq!(
            type_command("hello world", 12),
            vec!["type", "--key-delay", "12", "--", "hello world"],
        );
    }

    #[test]
    fn text_is_a_single_argument() {
        let argv = type_command("two words", 12);
        assert_eq!(argv.len(), 5);
        assert_eq!(argv.last().unwrap(), "two words");
    }

    #[test]
    fn leading_dash_text_sits_after_the_terminator() {
        let argv = type_command("--help", 5);
        let term = argv.iter().position(|a| a == "--").unwrap();
        assert_eq!(argv[term + 1], "--help");
    }

    #[test]
    fn special_characters_pass_through_literally() {
        let text = r#"echo "$HOME"; rm -rf /"#;
        assert_eq!(type_command(text, 12).last().unwrap(), text);
    }

    #[test]
    fn key_delay_is_rendered() {
        assert_eq!(type_command("x", 20)[2], "20");
    }
}
