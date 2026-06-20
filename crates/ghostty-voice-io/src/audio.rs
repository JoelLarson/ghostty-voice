//! Audio capture boundary adapter.
//!
//! Records a 16 kHz mono s16 WAV (Whisper's native format) via `pw-record`.
//! pw-record is stopped with SIGINT so it can flush and backpatch the WAV
//! header — SIGKILL would truncate the file.

use std::io::BufRead;
use std::path::Path;
use std::process::{Child, Command};

use anyhow::{Context, Result};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

/// Start capturing from `device` ("default" for the default source) into
/// `out`. The caller stops it with [`stop_recorder`]. Used by the daemon,
/// where the stop signal comes from a socket command, not Enter.
pub fn spawn_recorder(device: &str, out: &Path) -> Result<Child> {
    let mut cmd = Command::new("pw-record");
    cmd.arg("--rate=16000")
        .arg("--channels=1")
        .arg("--format=s16");
    if device != "default" {
        cmd.arg(format!("--target={device}"));
    }
    cmd.arg(out);
    cmd.spawn()
        .context("failed to start pw-record (is PipeWire running?)")
}

/// Stop a recorder cleanly (SIGINT, then wait for exit).
pub fn stop_recorder(child: &mut Child) -> Result<()> {
    let pid = Pid::from_raw(child.id() as i32);
    let _ = kill(pid, Signal::SIGINT);
    child.wait().context("pw-record did not exit cleanly")?;
    Ok(())
}

/// Record until the user presses Enter (the S1 standalone flow).
pub fn record_to_wav(device: &str, out: &Path) -> Result<()> {
    let mut child = spawn_recorder(device, out)?;
    println!("● Recording — press Enter to stop.");
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok();
    stop_recorder(&mut child)
}
