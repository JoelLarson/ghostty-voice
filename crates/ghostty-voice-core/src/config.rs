//! Typed configuration loaded from `config.toml`.
//!
//! Every section and field has a default, so a missing file or a partial
//! config still yields a usable [`Config`]. Parsing is pure (string in, struct
//! out); reading the file and expanding `~` happen at the IO boundary.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Top-level configuration (the S1 subset).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub whisper: WhisperConfig,
    pub audio: AudioConfig,
    pub inject: InjectConfig,
}

/// `[whisper]` — how to reach and pin the transcription server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WhisperConfig {
    /// whisper-server binary (path or name on PATH); config so an upstream
    /// rename is not a rebuild.
    pub binary: String,
    pub host: String,
    pub port: u16,
    /// Raw path; may begin with `~` (expanded at the IO boundary).
    pub model_path: String,
    /// PCI address of the GPU to pin (ADR-0001).
    pub vulkan_device: String,
    /// Extra launch flags passed through to whisper-server.
    pub extra_args: Vec<String>,
}

/// `[audio]` — capture device selection.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioConfig {
    pub device: String,
}

/// `[inject]` — `ydotool` typing behavior.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InjectConfig {
    pub key_delay_ms: u32,
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            binary: "whisper-server".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8910,
            model_path: "~/.local/share/ghostty-voice/models/ggml-large-v3.bin".to_owned(),
            vulkan_device: "0000:03:00.0".to_owned(),
            extra_args: Vec::new(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: "default".to_owned(),
        }
    }
}

impl Default for InjectConfig {
    fn default() -> Self {
        Self { key_delay_ms: 12 }
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
        assert_eq!(cfg.audio.device, "default");
        assert_eq!(cfg.inject.key_delay_ms, 12);
    }

    #[test]
    fn partial_config_overrides_only_given_fields() {
        let cfg = Config::from_toml_str("[whisper]\nport = 9000\n").unwrap();
        assert_eq!(cfg.whisper.port, 9000); // overridden
        assert_eq!(cfg.whisper.host, "127.0.0.1"); // still default
        assert_eq!(cfg.inject.key_delay_ms, 12); // absent section -> default
    }

    #[test]
    fn full_config_parses_all_fields() {
        let toml = r#"
[whisper]
host = "0.0.0.0"
port = 9001
model_path = "/models/x.bin"
vulkan_device = "0000:1a:00.0"

[audio]
device = "alsa_input.pci-0000_03_00"

[inject]
key_delay_ms = 20
"#;
        let cfg = Config::from_toml_str(toml).unwrap();
        assert_eq!(cfg.whisper.host, "0.0.0.0");
        assert_eq!(cfg.whisper.port, 9001);
        assert_eq!(cfg.whisper.model_path, "/models/x.bin");
        assert_eq!(cfg.whisper.vulkan_device, "0000:1a:00.0");
        assert_eq!(cfg.audio.device, "alsa_input.pci-0000_03_00");
        assert_eq!(cfg.inject.key_delay_ms, 20);
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
