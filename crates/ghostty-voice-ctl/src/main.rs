//! ghostty-voice-ctl — thin control client.
//!
//! Spawned by each GNOME hotkey: connect to the daemon's Unix socket, write one
//! command word, print the reply line, exit.

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
    /// Abort the current recording.
    Cancel,
    /// Print the daemon's current state.
    Status,
    /// Re-read non-model config.
    Reload,
}

impl Cmd {
    fn word(&self) -> &'static str {
        match self {
            Cmd::Toggle => "toggle",
            Cmd::Cancel => "cancel",
            Cmd::Status => "status",
            Cmd::Reload => "reload",
        }
    }
}

fn socket_path() -> Result<PathBuf> {
    let dir = std::env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
    Ok(PathBuf::from(dir).join("ghostty-voice.sock"))
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
    let response = send_to(&socket_path()?, cli.command.word())?;
    println!("{response}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::thread;

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
