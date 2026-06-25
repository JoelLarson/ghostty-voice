//! Pure, side-effect-free core logic for ghostty-voice.
//!
//! All decision logic lives here so it is testable with real objects (no
//! mocks); the daemon and CLI binaries are thin IO shells over this crate.

pub mod cache;
pub mod config;
pub mod corrections;
pub mod cue;
pub mod doctor;
pub mod filter;
pub mod link;
pub mod machine;
pub mod model;
pub mod pipeline;
pub mod prompt;
pub mod protocol;
pub mod pty;
pub mod queue;
pub mod session;
pub mod sink;
pub mod streaming;
pub mod strip;
pub mod supervisor;
pub mod transcript;
pub mod trigger;
pub mod vad;
pub mod vulkan;
pub mod window;
