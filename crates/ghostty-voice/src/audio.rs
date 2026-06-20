//! Audio capture boundary adapter.
//!
//! Records a 16 kHz mono s16 WAV (Whisper's native format) via `pw-record`,
//! stopping when the user presses Enter. pw-record is sent SIGINT so it can
//! flush and backpatch the WAV header — SIGKILL would truncate the file.

use std::io::BufRead;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

/// Record from `device` ("default" for the default source) into `out`.
pub fn record_to_wav(device: &str, out: &Path) -> Result<()> {
    let mut cmd = Command::new("pw-record");
    cmd.arg("--rate=16000")
        .arg("--channels=1")
        .arg("--format=s16");
    if device != "default" {
        cmd.arg(format!("--target={device}"));
    }
    cmd.arg(out);

    let mut child = cmd
        .spawn()
        .context("failed to start pw-record (is PipeWire running?)")?;

    println!("● Recording — press Enter to stop.");
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok();

    let pid = Pid::from_raw(child.id() as i32);
    let _ = kill(pid, Signal::SIGINT);
    child.wait().context("pw-record did not exit cleanly")?;
    Ok(())
}
