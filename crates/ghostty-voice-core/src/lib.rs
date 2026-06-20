//! Pure, side-effect-free core logic for ghostty-voice.
//!
//! All decision logic lives here so it is testable with real objects (no
//! mocks); the daemon and CLI binaries are thin IO shells over this crate.

pub mod cache;
pub mod config;
pub mod corrections;
pub mod delivery;
pub mod doctor;
pub mod filter;
pub mod hotkeys;
pub mod inject;
pub mod machine;
pub mod pipeline;
pub mod prompt;
pub mod protocol;
pub mod queue;
pub mod session;
pub mod supervisor;
pub mod transcript;
pub mod vad;
pub mod vulkan;
