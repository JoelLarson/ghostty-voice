//! Wire protocol for the control socket (S2).
//!
//! Newline-delimited UTF-8. A request is one command word; a response is one
//! line: `ok <state>` or `err <message>`. Deliberately dumb-simple — no JSON
//! until a field actually needs it.

/// A command sent by `ghostty-voice-ctl` to the daemon (S2 subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Toggle,
    /// Start a hands-free VAD recording (sox auto-stops on first silence, S5).
    Vad,
    Cancel,
    Status,
    Reload,
    /// Re-inject the most-recent cached transcript (recovery-only, S3).
    ReplayLast,
}

/// The daemon's observable state. `Loading` is the startup window while the
/// model loads into VRAM — distinct from `Idle` (ready) and from S7's
/// `Downloading`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Loading,
    Idle,
    Recording,
    Transcribing,
}

/// A daemon reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Ok(State),
    Err(String),
}

/// Why a request line could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    UnknownCommand(String),
}

impl Command {
    /// Parse one request line (whitespace-trimmed, case-insensitive).
    pub fn parse(line: &str) -> Result<Command, ProtocolError> {
        let word = line.trim();
        match word.to_ascii_lowercase().as_str() {
            "toggle" => Ok(Command::Toggle),
            "vad" => Ok(Command::Vad),
            "cancel" => Ok(Command::Cancel),
            "status" => Ok(Command::Status),
            "reload" => Ok(Command::Reload),
            "replay-last" => Ok(Command::ReplayLast),
            _ => Err(ProtocolError::UnknownCommand(word.to_owned())),
        }
    }
}

impl State {
    /// The wire token for this state.
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Loading => "loading",
            State::Idle => "idle",
            State::Recording => "recording",
            State::Transcribing => "transcribing",
        }
    }
}

impl Response {
    /// Encode as a single response line (no trailing newline).
    pub fn encode(&self) -> String {
        match self {
            Response::Ok(state) => format!("ok {}", state.as_str()),
            Response::Err(message) => format!("err {message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_command() {
        assert_eq!(Command::parse("toggle").unwrap(), Command::Toggle);
        assert_eq!(Command::parse("vad").unwrap(), Command::Vad);
        assert_eq!(Command::parse("cancel").unwrap(), Command::Cancel);
        assert_eq!(Command::parse("status").unwrap(), Command::Status);
        assert_eq!(Command::parse("reload").unwrap(), Command::Reload);
        assert_eq!(Command::parse("replay-last").unwrap(), Command::ReplayLast);
    }

    #[test]
    fn parsing_trims_and_ignores_case() {
        assert_eq!(Command::parse("  Toggle\n").unwrap(), Command::Toggle);
    }

    #[test]
    fn unknown_command_is_an_error() {
        assert_eq!(
            Command::parse("frobnicate"),
            Err(ProtocolError::UnknownCommand("frobnicate".to_owned())),
        );
    }

    #[test]
    fn states_render_as_tokens() {
        assert_eq!(State::Loading.as_str(), "loading");
        assert_eq!(State::Idle.as_str(), "idle");
        assert_eq!(State::Recording.as_str(), "recording");
        assert_eq!(State::Transcribing.as_str(), "transcribing");
    }

    #[test]
    fn encodes_ok_and_err_responses() {
        assert_eq!(Response::Ok(State::Idle).encode(), "ok idle");
        assert_eq!(
            Response::Err("ydotoold unreachable".to_owned()).encode(),
            "err ydotoold unreachable",
        );
    }
}
