//! Audio-cue boundary adapter (S3).
//!
//! Plays the hot-path start/stop cues via `paplay`. A start cue fires when
//! recording begins ("now listening"); the stop cue fires when recording ends
//! and on an empty/silence transcript ("working / done") — there is no distinct
//! error sound. An empty configured path means "no cue": a deliberate no-op,
//! never an error, so disabling a cue never breaks delivery.

use std::process::Command;

use anyhow::{Result, anyhow, bail};

/// Play the cue at `sound_path` via `paplay`. An empty path is a no-op (cue
/// disabled). Best-effort: a missing `paplay` or sound file should not abort
/// delivery, so callers typically ignore the error — but it is surfaced for the
/// rare caller that wants it.
pub fn play(sound_path: &str) -> Result<()> {
    if sound_path.is_empty() {
        return Ok(());
    }
    let status = Command::new("paplay")
        .arg(sound_path)
        .status()
        .map_err(|e| anyhow!("failed to run paplay: {e}"))?;
    if !status.success() {
        bail!("paplay exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_path_is_a_silent_noop() {
        // No paplay is spawned and no error is raised — disabling a cue is safe.
        assert!(play("").is_ok());
    }
}
