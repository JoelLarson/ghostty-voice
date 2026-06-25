//! Wire protocol for the control socket.
//!
//! Newline-delimited UTF-8. A request is one command word; a response is one
//! line: `ok <state>` or `err <message>`. Deliberately dumb-simple — no JSON
//! until a field actually needs it.

/// The control-socket protocol version, exchanged at `register-sink` time so a
/// `talk-to` **wrapper sink** can tell an *incompatible* daemon apart from an
/// unreachable one. Bump this whenever the wrapper-sink wire contract
/// (the `register-sink` handshake or the pushed [`Frame`]s) changes incompatibly.
pub const PROTOCOL_VERSION: u32 = 1;

/// Is a client speaking protocol `client` compatible with a daemon speaking
/// `daemon`? Versions must match exactly — a mismatch in either direction means
/// the wrapper-sink wire contract differs, so the client should report
/// `incompatible` and the daemon should refuse the registration.
pub fn version_compatible(client: u32, daemon: u32) -> bool {
    client == daemon
}

/// A command sent by `ghostty-voice-ctl` to the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Toggle,
    /// Start a hands-free VAD recording (sox auto-stops on first silence).
    Vad,
    /// Start a hands-free **streaming dictation**: a live self-editing preview
    /// flows into the active wrapper's prompt as you speak; a long trailing
    /// silence (or a Shift+F10 force-stop) ends it and the batch-accurate
    /// Transcript replaces the preview. The conscious extension of ADR-0002.
    Streaming,
    /// Start a hands-free Continuous-mode session: short pauses cut clips
    /// that batch-transcribe in the background; a long silence ends the session
    /// and delivers the assembled transcript. `cancel` aborts the whole session.
    Continuous,
    Cancel,
    Status,
    Reload,
    /// Re-deliver the most-recent cached transcript into the active **wrapper
    /// sink** (recovery-only); errors when no `talk-to` is registered.
    ReplayLast,
    /// Register the connecting client as a **wrapper sink** (`talk-to`, IDEAS.md
    /// #4). Unlike every other command this does not get a one-shot reply: the
    /// connection stays open and the daemon *pushes* [`Frame`]s down it until the
    /// client disconnects.
    ///
    /// Carries the client's [`PROTOCOL_VERSION`] (`register-sink <version>`) so an
    /// incompatible daemon can be detected. `None` is a legacy bare
    /// `register-sink` (a pre-handshake `talk-to`), which the daemon still accepts.
    RegisterSink(Option<u32>),
}

/// The daemon's observable state. `Downloading` is the first-run window while
/// the ~3 GB model is fetched — it precedes `Loading`, the window while the
/// model loads into VRAM, which in turn precedes `Idle` (ready).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// First-run model download in progress, carrying the whole-percent complete:
    /// `None` while the percent is unknown (no `Content-Length` yet), `Some(p)`
    /// once `p`% has streamed in. Distinct from `Loading`: nothing is in VRAM yet,
    /// the fetch is on the network, and it can take minutes. Like `Loading`, only
    /// `status` is answered while downloading. The percent lives here so both the
    /// wrapper-sink `Frame::State` push and the `StatusReport` serialize it from
    /// one source of truth.
    Downloading(Option<u8>),
    Loading,
    Idle,
    Recording,
    Transcribing,
    /// A streaming dictation is live: capture is running and the self-paced
    /// decode loop is pushing a live preview into the active wrapper's prompt.
    /// Keystrokes to the wrapped agent are suppressed while in this state.
    Streaming,
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
            "streaming" => Ok(Command::Streaming),
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
    /// Encode this state as its wire token(s). Every state is a single word
    /// except a `Downloading(Some(p))`, which is the two-token `downloading <p>`
    /// — the percent folded onto the deliberately-dumb line protocol, additive
    /// and backward-compatible with a bare `downloading`.
    pub fn encode_token(&self) -> String {
        match self {
            State::Downloading(None) => "downloading".to_owned(),
            State::Downloading(Some(pct)) => format!("downloading {pct}"),
            State::Loading => "loading".to_owned(),
            State::Idle => "idle".to_owned(),
            State::Recording => "recording".to_owned(),
            State::Transcribing => "transcribing".to_owned(),
            State::Streaming => "streaming".to_owned(),
        }
    }

    /// Parse a wire state substring back into a state (inverse of
    /// [`State::encode_token`]). Accepts the whole state substring so the
    /// two-token `downloading 42` reconstructs its percent; a bare `downloading`
    /// is the percent-unknown phase (backward-compatible with an older daemon). A
    /// percent over 100, a non-numeric percent, or any trailing tokens are
    /// malformed and return `None`.
    pub fn parse(token: &str) -> Option<State> {
        let mut parts = token.split_whitespace();
        let head = parts.next()?;
        let rest = parts.next();
        // No state word takes more than two tokens.
        if parts.next().is_some() {
            return None;
        }
        match (head, rest) {
            ("downloading", None) => Some(State::Downloading(None)),
            ("downloading", Some(p)) => {
                let pct: u8 = p.parse().ok()?;
                (pct <= 100).then_some(State::Downloading(Some(pct)))
            }
            ("loading", None) => Some(State::Loading),
            ("idle", None) => Some(State::Idle),
            ("recording", None) => Some(State::Recording),
            ("transcribing", None) => Some(State::Transcribing),
            ("streaming", None) => Some(State::Streaming),
            _ => None,
        }
    }

    /// The human-readable label for the status strip. Mirrors the wire token but
    /// renders the download percent as `downloading 42%` (a bare `downloading`
    /// for the percent-unknown phase), and the plain word for every other state.
    pub fn label(&self) -> String {
        match self {
            State::Downloading(None) => "downloading".to_owned(),
            State::Downloading(Some(pct)) => format!("downloading {pct}%"),
            State::Loading => "loading".to_owned(),
            State::Idle => "idle".to_owned(),
            State::Recording => "recording".to_owned(),
            State::Transcribing => "transcribing".to_owned(),
            State::Streaming => "streaming".to_owned(),
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
    /// One streaming-dictation **live-edit**: revise the prompt's live preview in
    /// place. `committed` is the newly-confirmed text to lock onto the stable
    /// prefix; `tail` is the current unstable tail to (re)display. The wrapper
    /// erases its previous tail, types `committed`, then types `tail` — settled
    /// words never flicker. Both fields carry their joining spaces. The live lane
    /// is an ephemeral rough preview reconciled by the batch Transcript on
    /// finalize — the conscious extension of ADR-0002.
    LiveEdit { committed: String, tail: String },
    /// **Finalize** a streaming dictation: replace the whole live preview with the
    /// batch-accurate, jargon-corrected **Transcript**. The wrapper erases its
    /// entire current streaming buffer (stable prefix + tail) and types this text,
    /// newline-stripped — the reconcile, with no double-typing. Pushed only to the
    /// *live* bound wrapper; a Held-for-replay delivery re-injects a plain
    /// [`Frame::Transcript`] append instead (there is no preview left to erase).
    Finalize(String),
}

/// Separator between a [`Frame::LiveEdit`]'s `committed` and `tail` fields on the
/// wire — ASCII Unit Separator. A normalized Transcript/tail never contains a
/// control char, so this keeps the deliberately-dumb line protocol (no JSON yet)
/// while carrying two space-bearing fields unambiguously.
const LIVE_EDIT_SEP: char = '\u{1f}';

impl Frame {
    /// Encode as a single wire line (no trailing newline; the caller adds it).
    pub fn encode(&self) -> String {
        match self {
            Frame::Transcript(text) => format!("transcript {text}"),
            Frame::State(state) => format!("state {}", state.encode_token()),
            Frame::LiveEdit { committed, tail } => {
                format!("live-edit {committed}{LIVE_EDIT_SEP}{tail}")
            }
            Frame::Finalize(text) => format!("finalize {text}"),
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
        if let Some(text) = line.strip_prefix("finalize ") {
            return Ok(Frame::Finalize(text.to_owned()));
        }
        if let Some(rest) = line.strip_prefix("live-edit ") {
            let (committed, tail) = rest.split_once(LIVE_EDIT_SEP).unwrap_or((rest, ""));
            return Ok(Frame::LiveEdit {
                committed: committed.to_owned(),
                tail: tail.to_owned(),
            });
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
            Response::Ok(state) => format!("ok {}", state.encode_token()),
            Response::Err(message) => format!("err {message}"),
        }
    }
}

/// The `status` reply: the daemon state plus how many **wrapper sinks** are
/// registered. The only kind of sink is a `talk-to` **wrapper sink**, so the
/// count is the whole story — `wrappers=0` means there is nowhere to deliver.
///
/// Encoded **additively** on the existing `ok <state>` line: `ok <state>
/// wrappers=<n>`. A bare `ok <state>` (an older daemon) still parses, defaulting
/// to 0 wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusReport {
    pub state: State,
    pub wrapper_count: usize,
}

impl StatusReport {
    /// Encode as a single `status` reply line (no trailing newline).
    pub fn encode(&self) -> String {
        format!(
            "ok {} wrappers={}",
            self.state.encode_token(),
            self.wrapper_count
        )
    }

    /// Parse a `status` reply line. Requires a leading `ok <state>`; the
    /// `wrappers=<n>` token is optional (a bare `ok <state>` from an older daemon
    /// defaults to 0 wrappers). Returns `None` for an `err` line, an unknown
    /// state, or a malformed line.
    pub fn parse(line: &str) -> Option<StatusReport> {
        let mut tokens = line.split_whitespace().peekable();
        if tokens.next()? != "ok" {
            return None;
        }
        // Isolate the state substring: the tokens after `ok` up to the first
        // `wrappers=` field. The state may be two tokens (`downloading 42`), so it
        // can't be read as a single token.
        let mut state_tokens: Vec<&str> = Vec::new();
        while let Some(token) = tokens.peek() {
            if token.starts_with("wrappers=") {
                break;
            }
            state_tokens.push(tokens.next()?);
        }
        let state = State::parse(&state_tokens.join(" "))?;
        let mut wrapper_count = 0usize;
        for token in tokens {
            if let Some(count) = token.strip_prefix("wrappers=") {
                wrapper_count = count.parse().ok()?;
            }
        }
        Some(StatusReport {
            state,
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
        assert_eq!(Command::parse("streaming").unwrap(), Command::Streaming);
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
    fn states_render_as_wire_tokens() {
        assert_eq!(State::Downloading(None).encode_token(), "downloading");
        assert_eq!(
            State::Downloading(Some(42)).encode_token(),
            "downloading 42"
        );
        assert_eq!(State::Loading.encode_token(), "loading");
        assert_eq!(State::Idle.encode_token(), "idle");
        assert_eq!(State::Recording.encode_token(), "recording");
        assert_eq!(State::Transcribing.encode_token(), "transcribing");
        assert_eq!(State::Streaming.encode_token(), "streaming");
    }

    #[test]
    fn downloading_token_round_trips_with_and_without_a_percent() {
        // The percent is folded into the Downloading state, so a `downloading 42`
        // token reconstructs the exact percent and a bare `downloading` is the
        // percent-unknown (no Content-Length yet) phase.
        for state in [
            State::Downloading(None),
            State::Downloading(Some(0)),
            State::Downloading(Some(42)),
            State::Downloading(Some(100)),
        ] {
            assert_eq!(State::parse(&state.encode_token()), Some(state));
        }
    }

    #[test]
    fn a_bare_downloading_token_is_backward_compatible() {
        // An older daemon emits a bare `downloading` with no percent; the richer
        // parser must still accept it as the percent-unknown phase.
        assert_eq!(State::parse("downloading"), Some(State::Downloading(None)));
    }

    #[test]
    fn a_downloading_token_with_a_bogus_percent_does_not_parse() {
        assert_eq!(
            State::parse("downloading 101"),
            None,
            "over 100% is malformed"
        );
        assert_eq!(State::parse("downloading huge"), None);
        assert_eq!(State::parse("downloading 42 extra"), None);
    }

    #[test]
    fn states_render_human_labels_including_the_percent_form() {
        assert_eq!(State::Downloading(Some(42)).label(), "downloading 42%");
        assert_eq!(State::Downloading(None).label(), "downloading");
        assert_eq!(State::Loading.label(), "loading");
        assert_eq!(State::Idle.label(), "idle");
        assert_eq!(State::Recording.label(), "recording");
        assert_eq!(State::Transcribing.label(), "transcribing");
        assert_eq!(State::Streaming.label(), "streaming");
    }

    #[test]
    fn encodes_ok_and_err_responses() {
        assert_eq!(Response::Ok(State::Idle).encode(), "ok idle");
        assert_eq!(
            Response::Err("whisper-server unreachable".to_owned()).encode(),
            "err whisper-server unreachable",
        );
    }

    // ---- push-sink protocol -----------------------------------

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
            State::Downloading(None),
            State::Downloading(Some(7)),
            State::Loading,
            State::Idle,
            State::Recording,
            State::Transcribing,
            State::Streaming,
        ] {
            assert_eq!(State::parse(&state.encode_token()), Some(state));
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
        for state in [
            State::Idle,
            State::Recording,
            State::Transcribing,
            State::Downloading(None),
            State::Downloading(Some(42)),
        ] {
            let frame = Frame::State(state);
            assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
        }
    }

    #[test]
    fn a_pushed_downloading_percent_frame_encodes_and_parses() {
        // The wrapper-sink strip learns the download percent through the same
        // `state` frame mechanism as every other State — `downloading 42` on the
        // wire, two tokens after `state`.
        let frame = Frame::State(State::Downloading(Some(42)));
        assert_eq!(frame.encode(), "state downloading 42");
        assert_eq!(Frame::parse("state downloading 42"), Ok(frame));
        assert_eq!(
            Frame::parse("state downloading"),
            Ok(Frame::State(State::Downloading(None))),
        );
    }

    #[test]
    fn live_edit_frame_round_trips_both_fields_preserving_spacing() {
        // The two space-bearing fields survive the round trip exactly, so the
        // wrapper erases and types precisely what the commit engine rendered.
        let frame = Frame::LiveEdit {
            committed: " onto".to_owned(),
            tail: " main branch".to_owned(),
        };
        assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
    }

    #[test]
    fn live_edit_frame_round_trips_with_empty_fields() {
        // No new commit this decode (empty committed), or an empty tail once the
        // utterance settles — both must round-trip.
        for frame in [
            Frame::LiveEdit {
                committed: String::new(),
                tail: "rebase".to_owned(),
            },
            Frame::LiveEdit {
                committed: " main".to_owned(),
                tail: String::new(),
            },
            Frame::LiveEdit {
                committed: String::new(),
                tail: String::new(),
            },
        ] {
            assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
        }
    }

    #[test]
    fn finalize_frame_round_trips_preserving_internal_spacing() {
        // The reconcile replaces the preview with the batch Transcript; its exact
        // spacing must survive so the typed text matches what was transcribed.
        let frame = Frame::Finalize("run ydotool  on Ghostty".to_owned());
        assert_eq!(frame.encode(), "finalize run ydotool  on Ghostty");
        assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
    }

    #[test]
    fn a_transcript_whose_text_begins_with_finalize_stays_a_transcript() {
        // The `transcript ` prefix is checked before `finalize `.
        let frame = Frame::Transcript("finalize the refactor".to_owned());
        assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
    }

    #[test]
    fn a_transcript_whose_text_begins_with_live_stays_a_transcript() {
        // The `transcript ` prefix is checked before `live-edit `, so a Transcript
        // that happens to begin with "live-edit" is not mis-parsed.
        let frame = Frame::Transcript("live-edit the handler".to_owned());
        assert_eq!(Frame::parse(&frame.encode()), Ok(frame));
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

    // ---- status report: wrapper count -----

    #[test]
    fn status_report_encodes_state_and_wrapper_count() {
        let report = StatusReport {
            state: State::Idle,
            wrapper_count: 0,
        };
        assert_eq!(report.encode(), "ok idle wrappers=0");

        let report = StatusReport {
            state: State::Recording,
            wrapper_count: 2,
        };
        assert_eq!(report.encode(), "ok recording wrappers=2");
    }

    #[test]
    fn status_report_round_trips_through_encode_parse() {
        for report in [
            StatusReport {
                state: State::Idle,
                wrapper_count: 0,
            },
            StatusReport {
                state: State::Transcribing,
                wrapper_count: 3,
            },
        ] {
            assert_eq!(StatusReport::parse(&report.encode()), Some(report));
        }
    }

    #[test]
    fn status_report_carries_a_downloading_percent_alongside_the_wrapper_count() {
        // The two-token `downloading 42` state must round-trip next to the
        // `wrappers=` field — the parser isolates the state substring (tokens
        // after `ok` up to the first `wrappers=`).
        let report = StatusReport {
            state: State::Downloading(Some(42)),
            wrapper_count: 1,
        };
        assert_eq!(report.encode(), "ok downloading 42 wrappers=1");
        assert_eq!(StatusReport::parse(&report.encode()), Some(report));

        // The percent-unknown phase is a single state token.
        let report = StatusReport {
            state: State::Downloading(None),
            wrapper_count: 0,
        };
        assert_eq!(report.encode(), "ok downloading wrappers=0");
        assert_eq!(StatusReport::parse(&report.encode()), Some(report));
    }

    #[test]
    fn status_report_parse_accepts_a_bare_ok_downloading_percent() {
        // A pre-wrappers daemon could answer `ok downloading 42` with no suffix;
        // the richer parse still isolates the two-token state and defaults the
        // wrapper count.
        assert_eq!(
            StatusReport::parse("ok downloading 42"),
            Some(StatusReport {
                state: State::Downloading(Some(42)),
                wrapper_count: 0,
            }),
        );
    }

    #[test]
    fn status_report_parse_is_backward_compatible_with_a_bare_ok_state() {
        // An older daemon answers `ok idle` with no suffix. The richer parse must
        // still succeed, defaulting to 0 wrappers rather than failing.
        assert_eq!(
            StatusReport::parse("ok idle"),
            Some(StatusReport {
                state: State::Idle,
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
