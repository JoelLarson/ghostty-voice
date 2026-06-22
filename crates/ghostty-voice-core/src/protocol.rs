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
    /// Start a hands-free Continuous-mode session (S6): short pauses cut clips
    /// that batch-transcribe in the background; a long silence ends the session
    /// and delivers the assembled transcript. `cancel` aborts the whole session.
    Continuous,
    Cancel,
    Status,
    Reload,
    /// Re-inject the most-recent cached transcript (recovery-only, S3).
    ReplayLast,
    /// Register the connecting client as a **wrapper sink** (`talk-to`, IDEAS.md
    /// #4). Unlike every other command this does not get a one-shot reply: the
    /// connection stays open and the daemon *pushes* [`Frame`]s down it until the
    /// client disconnects, at which point the focused-window sink reactivates.
    RegisterSink,
}

/// The daemon's observable state. `Downloading` is the first-run window while
/// the ~3 GB model is fetched (S7) — it precedes `Loading`, the window while the
/// model loads into VRAM, which in turn precedes `Idle` (ready).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// First-run model download in progress (S7). Distinct from `Loading`:
    /// nothing is in VRAM yet, the fetch is on the network, and it can take
    /// minutes. Like `Loading`, only `status` is answered while downloading.
    Downloading,
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
            "continuous" => Ok(Command::Continuous),
            "cancel" => Ok(Command::Cancel),
            "status" => Ok(Command::Status),
            "reload" => Ok(Command::Reload),
            "replay-last" => Ok(Command::ReplayLast),
            "register-sink" => Ok(Command::RegisterSink),
            _ => Err(ProtocolError::UnknownCommand(word.to_owned())),
        }
    }
}

impl State {
    /// The wire token for this state.
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Downloading => "downloading",
            State::Loading => "loading",
            State::Idle => "idle",
            State::Recording => "recording",
            State::Transcribing => "transcribing",
        }
    }

    /// Parse a wire token back into a state (inverse of [`State::as_str`]).
    pub fn parse(token: &str) -> Option<State> {
        match token {
            "downloading" => Some(State::Downloading),
            "loading" => Some(State::Loading),
            "idle" => Some(State::Idle),
            "recording" => Some(State::Recording),
            "transcribing" => Some(State::Transcribing),
            _ => None,
        }
    }
}

/// A daemon→client push frame on a *persistent* registered-sink connection (a
/// **wrapper sink**, IDEAS.md #4).
///
/// The control socket's normal traffic is one-shot request/response lines; these
/// frames are pushed *unsolicited* down a connection that stays open after
/// `register-sink`. They keep the deliberately-dumb line protocol — a
/// **Transcript** is already a single newline-free line and a state is one
/// token, so no JSON is needed yet (the slice-3 decision).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// The finished **Transcript** to inject into the wrapped agent's PTY.
    Transcript(String),
    /// A daemon state change, for the wrapper's status strip.
    State(State),
}

impl Frame {
    /// Encode as a single wire line (no trailing newline; the caller adds it).
    pub fn encode(&self) -> String {
        match self {
            Frame::Transcript(text) => format!("transcript {text}"),
            Frame::State(state) => format!("state {}", state.as_str()),
        }
    }

    /// Parse one pushed frame line. Only a trailing `\r`/`\n` is stripped — a
    /// Transcript keeps its internal and trailing spaces verbatim, so the bytes
    /// injected into the PTY are exactly what was transcribed.
    pub fn parse(line: &str) -> Result<Frame, ProtocolError> {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(text) = line.strip_prefix("transcript ") {
            return Ok(Frame::Transcript(text.to_owned()));
        }
        if let Some(token) = line.strip_prefix("state ") {
            return State::parse(token.trim())
                .map(Frame::State)
                .ok_or_else(|| ProtocolError::UnknownCommand(line.to_owned()));
        }
        Err(ProtocolError::UnknownCommand(line.to_owned()))
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
        assert_eq!(Command::parse("continuous").unwrap(), Command::Continuous);
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
        assert_eq!(State::Downloading.as_str(), "downloading");
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

    // ---- push-sink protocol (slice 3) -----------------------------------

    #[test]
    fn parses_register_sink_command() {
        assert_eq!(
            Command::parse("register-sink").unwrap(),
            Command::RegisterSink
        );
        assert_eq!(
            Command::parse(" REGISTER-SINK \n").unwrap(),
            Command::RegisterSink
        );
    }

    #[test]
    fn states_round_trip_through_their_wire_token() {
        for state in [
            State::Downloading,
            State::Loading,
            State::Idle,
            State::Recording,
            State::Transcribing,
        ] {
            assert_eq!(State::parse(state.as_str()), Some(state));
        }
        assert_eq!(State::parse("bogus"), None);
    }

    #[test]
    fn transcript_frame_round_trips_preserving_internal_spacing() {
        let frame = Frame::Transcript("create a function  that reverses a string".to_owned());
        assert_eq!(
            frame.encode(),
            "transcript create a function  that reverses a string"
        );
        assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
    }

    #[test]
    fn state_frame_round_trips_for_each_state() {
        for state in [State::Idle, State::Recording, State::Transcribing] {
            let frame = Frame::State(state);
            assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
        }
    }

    #[test]
    fn a_transcript_whose_text_begins_with_state_stays_a_transcript() {
        // The `transcript ` prefix is checked first, so a transcript that happens
        // to start with the word "state" is not mis-parsed as a state frame.
        let frame = Frame::Transcript("state of the art design".to_owned());
        assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
    }

    #[test]
    fn frame_parse_strips_only_the_trailing_newline() {
        assert_eq!(
            Frame::parse("transcript hello world\n"),
            Ok(Frame::Transcript("hello world".to_owned())),
        );
        assert_eq!(
            Frame::parse("state recording\r\n"),
            Ok(Frame::State(State::Recording)),
        );
    }

    #[test]
    fn frame_parse_rejects_an_unknown_line() {
        assert!(Frame::parse("frobnicate now").is_err());
    }
}
