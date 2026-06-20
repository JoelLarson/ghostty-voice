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
    /// Re-inject the most-recent cached transcript (refocus Ghostty first).
    ReplayLast,
    /// Install GNOME custom keybindings (Super+D toggle, Super+Alt+D cancel).
    InstallHotkeys,
    /// Diagnose the injection environment (ydotoold, input group, uinput).
    Doctor,
}

impl Cmd {
    /// The wire word for a socket command, or `None` for local-only subcommands.
    fn word(&self) -> Option<&'static str> {
        Some(match self {
            Cmd::Toggle => "toggle",
            Cmd::Cancel => "cancel",
            Cmd::Status => "status",
            Cmd::Reload => "reload",
            Cmd::ReplayLast => "replay-last",
            Cmd::InstallHotkeys | Cmd::Doctor => return None,
        })
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
    match cli.command {
        Cmd::InstallHotkeys => install_hotkeys(),
        Cmd::Doctor => doctor(),
        ref other => {
            let word = other.word().expect("a socket command");
            println!("{}", send_to(&socket_path()?, word)?);
            Ok(())
        }
    }
}

/// Probe the injection environment and print the diagnostic checks.
fn doctor() -> Result<()> {
    use ghostty_voice_core::doctor::{CheckStatus, evaluate};

    let ydotool_socket = std::env::var("YDOTOOL_SOCKET").unwrap_or_else(|_| {
        std::env::var("XDG_RUNTIME_DIR")
            .map(|d| format!("{d}/.ydotool_socket"))
            .unwrap_or_else(|_| "/tmp/.ydotool_socket".to_owned())
    });

    let probes = ghostty_voice_core::doctor::Probes {
        ydotool_socket_exists: Path::new(&ydotool_socket).exists(),
        in_input_group: in_group("input"),
        uinput_present: Path::new("/dev/uinput").exists(),
    };

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

/// Is the current user a member of `group` (via `id -Gn`)?
fn in_group(group: &str) -> bool {
    std::process::Command::new("id")
        .arg("-Gn")
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .any(|g| g == group)
        })
        .unwrap_or(false)
}

/// Install the GNOME custom keybindings via `gsettings`, merging with any
/// already present.
fn install_hotkeys() -> Result<()> {
    use ghostty_voice_core::hotkeys::{
        Hotkey, format_path_list, keybinding_path, merge_paths, parse_path_list,
    };

    const SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
    let exe = "ghostty-voice-ctl";
    let hotkeys = [
        Hotkey {
            slug: "ghostty-voice-toggle",
            name: "ghostty-voice toggle",
            command: format!("{exe} toggle"),
            binding: "<Super>d",
        },
        Hotkey {
            slug: "ghostty-voice-cancel",
            name: "ghostty-voice cancel",
            command: format!("{exe} cancel"),
            binding: "<Super><Alt>d",
        },
    ];

    let existing = parse_path_list(&run_gsettings(&["get", SCHEMA, "custom-keybindings"])?);
    let ours: Vec<String> = hotkeys.iter().map(|h| keybinding_path(h.slug)).collect();
    let merged = merge_paths(&existing, &ours);
    run_gsettings(&[
        "set",
        SCHEMA,
        "custom-keybindings",
        &format_path_list(&merged),
    ])?;

    for h in &hotkeys {
        let target = format!("{SCHEMA}.custom-keybinding:{}", keybinding_path(h.slug));
        run_gsettings(&["set", &target, "name", h.name])?;
        run_gsettings(&["set", &target, "command", &h.command])?;
        run_gsettings(&["set", &target, "binding", h.binding])?;
    }

    println!("Installed hotkeys: Super+D = toggle, Super+Alt+D = cancel.");
    Ok(())
}

fn run_gsettings(args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("gsettings")
        .args(args)
        .output()
        .context("failed to run gsettings (a GNOME session is required)")?;
    if !output.status.success() {
        anyhow::bail!(
            "gsettings {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
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
