//! IO boundary adapters shared by the skeleton and the daemon.
//!
//! Thin wrappers over external processes/services — `pw-record` capture and
//! whisper-server HTTP. The pure decision logic they rely on lives in
//! `ghostty-voice-core`.

pub mod audio;
pub mod cache;
pub mod cue;
pub mod download;
pub mod transcribe;
