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
}
