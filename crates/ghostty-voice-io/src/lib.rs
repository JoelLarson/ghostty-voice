//! IO boundary adapters shared by the S1 skeleton and the S2+ daemon.
//!
//! Thin wrappers over external processes/services — `pw-record` capture,
//! whisper-server HTTP, and `ydotool` injection. The pure decision logic they
//! rely on lives in `ghostty-voice-core`.

pub mod audio;
pub mod cache;
pub mod cue;
pub mod download;
pub mod inject;
pub mod input;

/// Re-exported so callers (e.g. `ghostty-voice-ctl bind`) can name the device
/// type without taking a direct `evdev` dependency.
pub use evdev;
pub mod transcribe;
