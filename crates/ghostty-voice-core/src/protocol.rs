//! Wire protocol for the control socket (S2).
//!
//! Newline-delimited UTF-8. A request is one command word; a response is one
//! line: `ok <state>` or `err <message>`. Deliberately dumb-simple — no JSON
//! until a field actually needs it.

/// The control-socket protocol version, exchanged at `register-sink` time so a
/// `talk-to` **wrapper sink** can tell an *incompatible* daemon apart from an
/// unreachable one (task-10.3). Bump this whenever the wrapper-sink wire contract
/// (the `register-sink` handshake or the pushed [`Frame`]s) changes incompatibly.
pub const PROTOCOL_VERSION: u32 = 1;

/// Is a client speaking protocol `client` compatible with a daemon speaking
/// `daemon`? Versions must match exactly — a mismatch in either direction means
/// the wrapper-sink wire contract differs, so the client should report
/// `incompatible` and the daemon should refuse the registration.
pub fn version_compatible(client: u32, daemon: u32) -> bool {
    client == daemon
}

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
    ///
    /// Carries the client's [`PROTOCOL_VERSION`] (`register-sink <version>`) so an
    /// incompatible daemon can be detected (task-10.3). `None` is a legacy bare
    /// `register-sink` (a pre-handshake `talk-to`), which the daemon still accepts.
    RegisterSink(Option<u32>),
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
    ///
    /// `register-sink` may carry an optional protocol version
    /// (`register-sink <version>`); every other command is an exact word.
    pub fn parse(line: &str) -> Result<Command, ProtocolError> {
        let word = line.trim();
        let lower = word.to_ascii_lowercase();
        // register-sink optionally carries the client's PROTOCOL_VERSION.
        if let Some(rest) = lower.strip_prefix("register-sink") {
            let rest = rest.trim();
            if rest.is_empty() {
                return Ok(Command::RegisterSink(None));
            }
            return rest
                .parse::<u32>()
                .map(|v| Command::RegisterSink(Some(v)))
                .map_err(|_| ProtocolError::UnknownCommand(word.to_owned()));
        }
        match lower.as_str() {
            "toggle" => Ok(Command::Toggle),
            "vad" => Ok(Command::Vad),
            "continuous" => Ok(Command::Continuous),
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

/// Which kind of **Delivery sink** is active, for the `status` report (task-10.1).
/// Exactly one sink is active at a time (CONTEXT.md): the default
/// **focused-window sink** or a **wrapper sink** (a running `talk-to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkKind {
    FocusedWindow,
    Wrapper,
}

impl SinkKind {
    /// The wire token for this sink kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            SinkKind::FocusedWindow => "focused-window",
            SinkKind::Wrapper => "wrapper",
        }
    }

    /// Parse a wire token back into a sink kind (inverse of [`SinkKind::as_str`]).
    pub fn parse(token: &str) -> Option<SinkKind> {
        match token {
            "focused-window" => Some(SinkKind::FocusedWindow),
            "wrapper" => Some(SinkKind::Wrapper),
            _ => None,
        }
    }
}

/// The `status` reply: the daemon state plus which **Delivery sink** is active and
/// how many **wrapper sinks** are registered (task-10.1). Lets a user confirm
/// routing — wrapper sink vs focused-window sink — without tailing journald.
///
/// Encoded **additively** on the existing `ok <state>` line so it stays
/// backward-compatible: `ok <state> sink=<kind> wrappers=<n>`. An older daemon
/// that answers a bare `ok <state>` still parses (defaulting to the
/// focused-window sink / 0 wrappers — the pre-wrapper semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusReport {
    pub state: State,
    pub active_sink: SinkKind,
    pub wrapper_count: usize,
}

impl StatusReport {
    /// Encode as a single `status` reply line (no trailing newline).
    pub fn encode(&self) -> String {
        format!(
            "ok {} sink={} wrappers={}",
            self.state.as_str(),
            self.active_sink.as_str(),
            self.wrapper_count,
        )
    }

    /// Parse a `status` reply line. Requires a leading `ok <state>`; the
    /// `sink=<kind>` and `wrappers=<n>` tokens are optional (a bare `ok <state>`
    /// from an older daemon defaults to the focused-window sink / 0 wrappers).
    /// Returns `None` for an `err` line, an unknown state, or a malformed line.
    pub fn parse(line: &str) -> Option<StatusReport> {
        let mut tokens = line.split_whitespace();
        if tokens.next()? != "ok" {
            return None;
        }
        let state = State::parse(tokens.next()?)?;
        let mut active_sink = SinkKind::FocusedWindow;
        let mut wrapper_count = 0usize;
        for token in tokens {
            if let Some(kind) = token.strip_prefix("sink=") {
                active_sink = SinkKind::parse(kind)?;
            } else if let Some(count) = token.strip_prefix("wrappers=") {
                wrapper_count = count.parse().ok()?;
            }
        }
        Some(StatusReport {
            state,
            active_sink,
            wrapper_count,
        })
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
    fn parses_a_bare_register_sink_as_a_legacy_no_version_registration() {
        assert_eq!(
            Command::parse("register-sink").unwrap(),
            Command::RegisterSink(None)
        );
        assert_eq!(
            Command::parse(" REGISTER-SINK \n").unwrap(),
            Command::RegisterSink(None)
        );
    }

    #[test]
    fn parses_register_sink_with_a_protocol_version() {
        assert_eq!(
            Command::parse("register-sink 1").unwrap(),
            Command::RegisterSink(Some(1)),
        );
        assert_eq!(
            Command::parse(&format!("register-sink {PROTOCOL_VERSION}")).unwrap(),
            Command::RegisterSink(Some(PROTOCOL_VERSION)),
        );
    }

    #[test]
    fn register_sink_with_a_non_numeric_version_is_an_error() {
        assert!(Command::parse("register-sink twelve").is_err());
    }

    #[test]
    fn version_compatibility_requires_an_exact_match() {
        assert!(version_compatible(PROTOCOL_VERSION, PROTOCOL_VERSION));
        assert!(version_compatible(1, 1));
        assert!(!version_compatible(0, 1), "an older client is incompatible");
        assert!(!version_compatible(2, 1), "a newer client is incompatible");
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

    // ---- status report: active Delivery sink + wrapper count (task-10.1) -----

    #[test]
    fn sink_kind_round_trips_through_its_wire_token() {
        for kind in [SinkKind::FocusedWindow, SinkKind::Wrapper] {
            assert_eq!(SinkKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(SinkKind::FocusedWindow.as_str(), "focused-window");
        assert_eq!(SinkKind::Wrapper.as_str(), "wrapper");
        assert_eq!(SinkKind::parse("bogus"), None);
    }

    #[test]
    fn status_report_encodes_state_active_sink_and_wrapper_count() {
        let report = StatusReport {
            state: State::Idle,
            active_sink: SinkKind::FocusedWindow,
            wrapper_count: 0,
        };
        assert_eq!(report.encode(), "ok idle sink=focused-window wrappers=0");

        let report = StatusReport {
            state: State::Recording,
            active_sink: SinkKind::Wrapper,
            wrapper_count: 2,
        };
        assert_eq!(report.encode(), "ok recording sink=wrapper wrappers=2");
    }

    #[test]
    fn status_report_round_trips_through_encode_parse() {
        for report in [
            StatusReport {
                state: State::Idle,
                active_sink: SinkKind::FocusedWindow,
                wrapper_count: 0,
            },
            StatusReport {
                state: State::Transcribing,
                active_sink: SinkKind::Wrapper,
                wrapper_count: 3,
            },
        ] {
            assert_eq!(StatusReport::parse(&report.encode()), Some(report));
        }
    }

    #[test]
    fn status_report_parse_is_backward_compatible_with_a_bare_ok_state() {
        // An older daemon answers `ok idle` with no sink suffix. The richer parse
        // must still succeed, defaulting to the focused-window sink and 0 wrappers
        // (the pre-wrapper semantics) rather than failing.
        assert_eq!(
            StatusReport::parse("ok idle"),
            Some(StatusReport {
                state: State::Idle,
                active_sink: SinkKind::FocusedWindow,
                wrapper_count: 0,
            }),
        );
    }

    #[test]
    fn status_report_parse_rejects_a_non_ok_or_unknown_state_line() {
        assert_eq!(StatusReport::parse("err something broke"), None);
        assert_eq!(StatusReport::parse("ok bogus-state"), None);
        assert_eq!(StatusReport::parse("garbage"), None);
    }
}
