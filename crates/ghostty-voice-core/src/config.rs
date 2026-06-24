//! Typed configuration loaded from `config.toml`.
//!
//! Every section and field has a default, so a missing file or a partial
//! config still yields a usable [`Config`]. Parsing is pure (string in, struct
//! out); reading the file and expanding `~` happen at the IO boundary.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Top-level configuration.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub whisper: WhisperConfig,
    pub audio: AudioConfig,
    pub feedback: FeedbackConfig,
    pub cache: CacheConfig,
    /// `[corrections]` — deterministic jargon spell-fixer (`"why do tool" =
    /// "ydotool"`). A TOML table of `misheard = correct` pairs.
    pub corrections: BTreeMap<String, String>,
}

/// `[whisper]` — how to reach and pin the transcription server, plus the
/// accuracy-stack request params.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WhisperConfig {
    /// whisper-server binary (path or name on PATH); config so an upstream
    /// rename is not a rebuild.
    pub binary: String,
    pub host: String,
    pub port: u16,
    /// Raw path; may begin with `~` (expanded at the IO boundary).
    pub model_path: String,
    /// First-run download source: where `ggml-large-v3.bin` is fetched
    /// from if `model_path` is missing. Defaults to the HuggingFace LFS object.
    pub model_url: String,
    /// First-run download integrity: expected SHA-256 of the model file
    /// (lowercase hex), or empty to skip verification. Pin from HuggingFace.
    pub model_sha256: String,
    /// PCI address of the GPU to pin (ADR-0001).
    pub vulkan_device: String,
    /// Extra launch flags passed through to whisper-server.
    pub extra_args: Vec<String>,
    /// Decoder beam width: larger beam buys accuracy on ambiguous audio.
    pub beam_size: u32,
    /// Sampling temperature: `0.0` for deterministic decoding.
    pub temperature: f64,
    /// `initial_prompt` prefix sentence; the `vocab` terms are appended after
    /// `" Vocabulary:"` by the bounded prompt builder.
    pub prompt_prefix: String,
    /// Domain vocab biased into the decoder via `initial_prompt`. Grows as
    /// misses are noticed; bounded to the token cap by the prompt builder.
    pub vocab: Vec<String>,
}

/// `[audio]` — capture device selection and the runaway-recording cap.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioConfig {
    pub device: String,
    /// Safety cap (~900 s): on expiry the recorder stops + enqueues so a
    /// forgotten recording can't run away. Also backstops a VAD "never speak".
    pub max_recording_seconds: u64,
    /// VAD mode: seconds of trailing silence below `vad_threshold_pct`
    /// after which `sox` self-terminates the recording. Real-mic tunable.
    pub vad_silence_seconds: f32,
    /// VAD mode: the `sox` `silence` threshold as a percentage of full
    /// scale; audio below this counts as silence. Real-mic tunable.
    pub vad_threshold_pct: u32,
    /// Recordings shorter than this are discarded (accidental blips type
    /// nothing). Default 0.3 s.
    pub min_duration_seconds: f64,
    /// Continuous mode: a pause this long (below `vad_threshold_pct`) cuts
    /// the current clip and starts the next. Shorter than `session_end` — the
    /// clip-cut threshold of the dual-threshold split. Real-mic tunable.
    pub clip_cut_pause_seconds: f32,
    /// Continuous mode: a trailing silence this long ends the whole session
    /// and delivers the assembled transcript hands-free (~10 s). The session-end
    /// threshold of the dual-threshold split. Real-mic tunable.
    pub session_end_silence_seconds: f32,
    /// Continuous mode: clips shorter than this are accumulated into the
    /// next rather than transcribed alone, so stutters and micro-pauses don't
    /// spray tiny hallucination-prone fragments at Whisper (~2-3 s).
    pub min_clip_seconds: f32,
}

/// `[feedback]` — audio cues played on the hot path via `paplay`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FeedbackConfig {
    /// "Now listening" cue, played when recording starts. Empty = no cue.
    pub sound_start: String,
    /// "Working / done" cue, played when recording stops (and on
    /// empty/silence). Empty = no cue.
    pub sound_stop: String,
}

/// `[cache]` — WAV/transcript retention and the transcription-retry window.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CacheConfig {
    /// How many WAV recordings to keep (the accuracy-debugging corpus).
    pub wav_keep: usize,
    /// How many transcripts to keep (backs `replay-last`).
    pub transcript_keep: usize,
    /// Transcription-retry window: how many seconds the daemon keeps retrying a
    /// transcription while whisper-server is unreachable (e.g. mid-restart) before
    /// giving up and holding the utterance. Generous backstop (~15 min).
    pub retry_window_seconds: u64,
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            binary: "whisper-server".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8910,
            model_path: "~/.local/share/ghostty-voice/models/ggml-large-v3.bin".to_owned(),
            model_url: crate::model::GGML_LARGE_V3_URL.to_owned(),
            model_sha256: crate::model::GGML_LARGE_V3_SHA256.to_owned(),
            vulkan_device: "0000:03:00.0".to_owned(),
            extra_args: Vec::new(),
            beam_size: 8,
            temperature: 0.0,
            prompt_prefix: "Transcript of technical instructions.".to_owned(),
            vocab: Vec::new(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: "default".to_owned(),
            max_recording_seconds: 900,
            min_duration_seconds: 0.3,
            vad_silence_seconds: 2.0,
            vad_threshold_pct: 3,
            clip_cut_pause_seconds: 1.0,
            session_end_silence_seconds: 10.0,
            min_clip_seconds: 2.0,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            wav_keep: 30,
            transcript_keep: 5,
            retry_window_seconds: 900,
        }
    }
}

/// Why a configuration could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// The TOML was malformed.
    Parse(String),
}

impl Config {
    /// Parse a `config.toml` string into a [`Config`], filling any missing
    /// section or field from its default.
    pub fn from_toml_str(s: &str) -> Result<Config, ConfigError> {
        toml::from_str(s).map_err(|e| ConfigError::Parse(e.to_string()))
    }
}

/// The daemon's control socket, `$XDG_RUNTIME_DIR/ghostty-voice.sock`. Shared by
/// the daemon (which binds it) and every client (`ghostty-voice-ctl`, `talk-to`),
/// so the path contract lives in one place. `None` if `XDG_RUNTIME_DIR` is unset.
pub fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join("ghostty-voice.sock"))
}

/// The user config file, `$HOME/.config/ghostty-voice/config.toml`. `None` if
/// `HOME` is unset.
pub fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/ghostty-voice/config.toml"))
}

/// Expand a leading `~` or `~/...` to `home`. Paths without a leading tilde
/// (and `~user` forms, which name a different home) are returned unchanged.
/// `home` is injected so this stays pure and testable.
pub fn expand_tilde(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        home.to_path_buf()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_all_defaults() {
        let cfg = Config::from_toml_str("").unwrap();
        assert_eq!(cfg, Config::default());
        assert_eq!(cfg.whisper.binary, "whisper-server");
        assert_eq!(cfg.whisper.extra_args, Vec::<String>::new());
        assert_eq!(cfg.whisper.host, "127.0.0.1");
        assert_eq!(cfg.whisper.port, 8910);
        assert_eq!(cfg.whisper.beam_size, 8);
        assert_eq!(cfg.whisper.temperature, 0.0);
        assert_eq!(
            cfg.whisper.prompt_prefix,
            "Transcript of technical instructions."
        );
        assert_eq!(cfg.whisper.vocab, Vec::<String>::new());
        // First-run download: the model URL defaults to the HF LFS object,
        // and the expected SHA is unset (verification deferred until pinned).
        assert_eq!(
            cfg.whisper.model_url,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin"
        );
        assert_eq!(cfg.whisper.model_sha256, "");
        assert_eq!(cfg.audio.device, "default");
        assert_eq!(cfg.audio.max_recording_seconds, 900);
        assert_eq!(cfg.audio.min_duration_seconds, 0.3);
        assert_eq!(cfg.audio.vad_silence_seconds, 2.0);
        assert_eq!(cfg.audio.vad_threshold_pct, 3);
        assert_eq!(cfg.audio.clip_cut_pause_seconds, 1.0);
        assert_eq!(cfg.audio.session_end_silence_seconds, 10.0);
        assert_eq!(cfg.audio.min_clip_seconds, 2.0);
        assert!(cfg.corrections.is_empty());
        assert_eq!(cfg.feedback.sound_start, "");
        assert_eq!(cfg.feedback.sound_stop, "");
        assert_eq!(cfg.cache.wav_keep, 30);
        assert_eq!(cfg.cache.transcript_keep, 5);
        assert_eq!(cfg.cache.retry_window_seconds, 900);
    }

    #[test]
    fn partial_config_overrides_only_given_fields() {
        let cfg = Config::from_toml_str("[whisper]\nport = 9000\n").unwrap();
        assert_eq!(cfg.whisper.port, 9000); // overridden
        assert_eq!(cfg.whisper.host, "127.0.0.1"); // still default
        assert_eq!(cfg.audio.device, "default"); // absent section -> default
    }

    #[test]
    fn full_config_parses_all_fields() {
        let toml = r#"
[whisper]
host = "0.0.0.0"
port = 9001
model_path = "/models/x.bin"
vulkan_device = "0000:1a:00.0"
beam_size = 5
temperature = 0.2
prompt_prefix = "Custom prefix."
vocab = ["ydotool", "Ghostty", "kubectl"]
model_sha256 = "abc123"

[audio]
device = "alsa_input.pci-0000_03_00"
max_recording_seconds = 600
min_duration_seconds = 0.5
vad_silence_seconds = 1.5
vad_threshold_pct = 5
clip_cut_pause_seconds = 1.2
session_end_silence_seconds = 8.0
min_clip_seconds = 3.0

[feedback]
sound_start = "/usr/share/ghostty-voice/start.wav"
sound_stop = "/usr/share/ghostty-voice/stop.wav"

[cache]
wav_keep = 50
transcript_keep = 8
retry_window_seconds = 1200

[corrections]
"why do tool" = "ydotool"
"ghosty" = "Ghostty"
"#;
        let cfg = Config::from_toml_str(toml).unwrap();
        assert_eq!(cfg.whisper.host, "0.0.0.0");
        assert_eq!(cfg.whisper.port, 9001);
        assert_eq!(cfg.whisper.model_path, "/models/x.bin");
        assert_eq!(cfg.whisper.vulkan_device, "0000:1a:00.0");
        assert_eq!(cfg.whisper.beam_size, 5);
        assert_eq!(cfg.whisper.temperature, 0.2);
        assert_eq!(cfg.whisper.prompt_prefix, "Custom prefix.");
        assert_eq!(cfg.whisper.vocab, vec!["ydotool", "Ghostty", "kubectl"]);
        assert_eq!(cfg.whisper.model_sha256, "abc123");
        assert_eq!(cfg.audio.device, "alsa_input.pci-0000_03_00");
        assert_eq!(cfg.audio.max_recording_seconds, 600);
        assert_eq!(cfg.audio.min_duration_seconds, 0.5);
        assert_eq!(cfg.audio.vad_silence_seconds, 1.5);
        assert_eq!(cfg.audio.vad_threshold_pct, 5);
        assert_eq!(cfg.audio.clip_cut_pause_seconds, 1.2);
        assert_eq!(cfg.audio.session_end_silence_seconds, 8.0);
        assert_eq!(cfg.audio.min_clip_seconds, 3.0);
        assert_eq!(cfg.corrections.get("why do tool").unwrap(), "ydotool");
        assert_eq!(cfg.corrections.get("ghosty").unwrap(), "Ghostty");
        assert_eq!(
            cfg.feedback.sound_start,
            "/usr/share/ghostty-voice/start.wav"
        );
        assert_eq!(cfg.feedback.sound_stop, "/usr/share/ghostty-voice/stop.wav");
        assert_eq!(cfg.cache.wav_keep, 50);
        assert_eq!(cfg.cache.transcript_keep, 8);
        assert_eq!(cfg.cache.retry_window_seconds, 1200);
    }

    #[test]
    fn shipped_example_config_parses() {
        // The shipped config.toml.example must stay valid under deny_unknown_fields
        // so a fresh install's copy-and-edit never starts from a broken file.
        let example = include_str!("../../../config.toml.example");
        let cfg = Config::from_toml_str(example).expect("config.toml.example must parse");
        assert_eq!(cfg.whisper.beam_size, 8);
        assert_eq!(cfg.audio.min_duration_seconds, 0.3);
        assert_eq!(cfg.corrections.get("why do tool").unwrap(), "ydotool");
        assert!(cfg.whisper.vocab.contains(&"ydotool".to_owned()));
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(Config::from_toml_str("this is = = not toml").is_err());
    }

    #[test]
    fn rejects_unknown_field() {
        // A typo (`por` for `port`) must fail loudly, not be silently ignored.
        assert!(Config::from_toml_str("[whisper]\npor = 9000\n").is_err());
    }

    #[test]
    fn rejects_unknown_section() {
        assert!(Config::from_toml_str("[wibble]\nx = 1\n").is_err());
    }

    #[test]
    fn expands_tilde_slash() {
        assert_eq!(
            expand_tilde("~/.local/share/x", Path::new("/home/joel")),
            PathBuf::from("/home/joel/.local/share/x"),
        );
    }

    #[test]
    fn expands_bare_tilde() {
        assert_eq!(
            expand_tilde("~", Path::new("/home/joel")),
            PathBuf::from("/home/joel"),
        );
    }

    #[test]
    fn leaves_absolute_path_unchanged() {
        assert_eq!(
            expand_tilde("/models/x.bin", Path::new("/home/joel")),
            PathBuf::from("/models/x.bin"),
        );
    }

    #[test]
    fn leaves_relative_path_unchanged() {
        assert_eq!(
            expand_tilde("models/x.bin", Path::new("/home/joel")),
            PathBuf::from("models/x.bin"),
        );
    }

    #[test]
    fn does_not_expand_tilde_user() {
        // `~bob` names a different user's home, which we can't resolve here.
        assert_eq!(
            expand_tilde("~bob/x", Path::new("/home/joel")),
            PathBuf::from("~bob/x"),
        );
    }
}
