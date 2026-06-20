//! Pure, side-effect-free core logic for ghostty-voice.
//!
//! All decision logic lives here so it is testable with real objects (no
//! mocks); the daemon and CLI binaries are thin IO shells over this crate.

pub mod config;
pub mod hotkeys;
pub mod inject;
pub mod machine;
pub mod protocol;
pub mod supervisor;
pub mod transcript;
pub mod vulkan;
