//! Text injection boundary adapter: run `ydotool type` with the argv built by
//! the core command builder (which owns the `--`-terminator safety logic).

use std::process::Command;

use anyhow::{Result, anyhow, bail};

/// Type `text` into the focused window via `ydotool`, without pressing Enter.
pub fn type_text(text: &str, key_delay_ms: u32) -> Result<()> {
    let argv = ghostty_voice_core::inject::type_command(text, key_delay_ms);
    let status = Command::new("ydotool")
        .args(&argv)
        .status()
        .map_err(|e| anyhow!("failed to run ydotool: {e} (is ydotoold running?)"))?;
    if !status.success() {
        bail!("ydotool exited with {status}");
    }
    Ok(())
}
