//! IO boundary adapters shared by the skeleton and the daemon.
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
pub mod transcribe;
