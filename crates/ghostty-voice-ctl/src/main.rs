//! ghostty-voice-ctl — thin control client.
//!
//! Manual commands connect to the daemon's Unix socket, write one command word,
//! print the reply line, and exit. The everyday triggers are recognized inside
//! `talk-to` (Shift+F10/F9), not here; this client is for one-shot commands like
//! `cancel`, `status`, `reload`, and `replay-last`, plus `doctor`.

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ghostty-voice-ctl",
    about = "Control client for ghostty-voiced"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start/stop recording (toggle).
    Toggle,
    /// Start a hands-free VAD recording (sox auto-stops on silence).
    Vad,
    /// Start a hands-free Continuous-mode session: talk with pauses; clips
    /// transcribe in the background and the assembled transcript is delivered
    /// when you stop for a while. `cancel` aborts the whole session.
    Continuous,
    /// Abort the current recording.
    Cancel,
    /// Print the daemon's current state.
    Status,
    /// Re-read non-model config.
    Reload,
    /// Re-deliver the most-recent cached transcript into the active talk-to.
    ReplayLast,
    /// Diagnose the environment (is the daemon reachable?).
    Doctor,
}

impl Cmd {
    /// The wire word for a socket command, or `None` for local-only subcommands.
    fn word(&self) -> Option<&'static str> {
        Some(match self {
            Cmd::Toggle => "toggle",
            Cmd::Vad => "vad",
            Cmd::Continuous => "continuous",
            Cmd::Cancel => "cancel",
            Cmd::Status => "status",
            Cmd::Reload => "reload",
            Cmd::ReplayLast => "replay-last",
            Cmd::Doctor => return None,
        })
    }
}

fn socket_path() -> Result<PathBuf> {
    ghostty_voice_core::config::socket_path().context("XDG_RUNTIME_DIR is not set")
}

/// Send one command word to the daemon socket and return its reply line.
fn send_to(path: &Path, word: &str) -> Result<String> {
    let mut stream = UnixStream::connect(path).with_context(|| {
        format!(
            "cannot reach daemon at {} (is ghostty-voiced running?)",
            path.display()
        )
    })?;
    stream.write_all(format!("{word}\n").as_bytes())?;
    stream.shutdown(Shutdown::Write).ok();
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response.trim().to_owned())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Doctor => doctor(),
        ref other => {
            let word = other.word().expect("a socket command");
            println!("{}", send_to(&socket_path()?, word)?);
            Ok(())
        }
    }
}

/// Probe whether the daemon is reachable and print the diagnostic checks.
fn doctor() -> Result<()> {
    use ghostty_voice_core::doctor::{CheckStatus, evaluate};

    let daemon_reachable = socket_path()
        .map(|p| UnixStream::connect(p).is_ok())
        .unwrap_or(false);
    let probes = ghostty_voice_core::doctor::Probes { daemon_reachable };

    let mut all_ok = true;
    for check in evaluate(&probes) {
        match check.status {
            CheckStatus::Ok => println!("  ok   {}", check.name),
            CheckStatus::Problem(msg) => {
                all_ok = false;
                println!("  FAIL {} — {msg}", check.name);
            }
        }
    }
    if !all_ok {
        anyhow::bail!("environment has problems (see above)");
    }
    println!("environment looks good.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::thread;

    #[test]
    fn vad_maps_to_the_vad_wire_word() {
        assert_eq!(Cmd::Vad.word(), Some("vad"));
    }

    #[test]
    fn doctor_is_a_local_only_subcommand() {
        assert_eq!(Cmd::Doctor.word(), None);
    }

    #[test]
    fn continuous_maps_to_the_continuous_wire_word() {
        assert_eq!(Cmd::Continuous.word(), Some("continuous"));
    }

    #[test]
    fn sends_command_and_reads_reply() {
        let path = std::env::temp_dir().join(format!("gv-ctl-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut req = String::new();
            stream.read_to_string(&mut req).unwrap();
            assert_eq!(req.trim(), "status");
            stream.write_all(b"ok idle\n").unwrap();
        });

        let reply = send_to(&path, "status").unwrap();
        server.join().unwrap();
        assert_eq!(reply, "ok idle");
        let _ = std::fs::remove_file(&path);
    }
}
