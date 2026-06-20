//! IO boundary adapters shared by the S1 skeleton and the S2+ daemon.
//!
//! Thin wrappers over external processes/services — `pw-record` capture,
//! whisper-server HTTP, and `ydotool` injection. The pure decision logic they
//! rely on lives in `ghostty-voice-core`.

pub mod audio;
pub mod inject;
pub mod transcribe;
