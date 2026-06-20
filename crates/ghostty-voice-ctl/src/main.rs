//! ghostty-voice-ctl — thin control client.
//!
//! Manual commands connect to the daemon's Unix socket, write one command word,
//! print the reply line, and exit. The everyday triggers are tactile (evdev,
//! read by the daemon — see S8); `bind` is the setup flow that captures the
//! trigger keys and `doctor` diagnoses the environment.

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
    /// Re-inject the most-recent cached transcript (refocus Ghostty first).
    ReplayLast,
    /// Capture the tactile trigger keys (Start/Stop), warn on conflicts, run a
    /// live test, and write them to config. Re-runnable to rebind.
    Bind,
    /// Diagnose the environment (ydotoold, input group, uinput, trigger device).
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
            Cmd::Bind | Cmd::Doctor => return None,
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
        Cmd::Bind => bind(),
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

    let device = load_config().input.device;
    let probes = ghostty_voice_core::doctor::Probes {
        ydotool_socket_exists: Path::new(&ydotool_socket).exists(),
        in_input_group: in_group("input"),
        uinput_present: Path::new("/dev/uinput").exists(),
        trigger_device_readable: ghostty_voice_io::input::device_available(&device),
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

/// The bind/setup flow (S8): capture the Start and Stop trigger keys directly
/// from the configured evdev device, show exactly what each emits, warn on
/// conflicts, run a live "press once" test, then write the combos to config.
fn bind() -> Result<()> {
    use ghostty_voice_core::config::set_input_combos;

    let cfg = load_config();
    let selector = cfg.input.device.clone();
    let (path, mut device) = ghostty_voice_io::input::open_device(&selector).with_context(|| {
        format!(
            "cannot open input device {selector:?} — are you in the 'input' group? \
             (run `ghostty-voice-ctl doctor`)"
        )
    })?;
    println!(
        "Reading from: {} [{}]\n",
        ghostty_voice_io::input::device_name(&device),
        path.display()
    );

    let start = capture_one(&mut device, "START (tap = latch, hold = push-to-talk)")?;
    let stop = capture_one(&mut device, "STOP (tap = stop, hold = hands-free VAD)")?;

    println!("\nBinding:");
    println!("  start_combo = \"{}\"", start.display());
    println!("  stop_combo  = \"{}\"", stop.display());

    // Persist into the user's config, preserving everything else.
    let cfg_path = config_path()?;
    let existing = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let updated = set_input_combos(&existing, &start.display(), &stop.display());
    if let Some(parent) = cfg_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&cfg_path, updated)
        .with_context(|| format!("writing {}", cfg_path.display()))?;

    println!("\nWrote {}.", cfg_path.display());
    println!("Run `systemctl --user restart ghostty-voiced` to apply (or replug the device).");
    Ok(())
}

/// Capture one trigger key, display what it emitted, warn on conflicts, and run
/// the live press-once test (the ground-truth conflict check).
fn capture_one(
    device: &mut ghostty_voice_io::evdev::Device,
    label: &str,
) -> Result<ghostty_voice_core::key_combo::KeyCombo> {
    use ghostty_voice_core::bind::{BindProbes, evaluate, looks_safe};
    use ghostty_voice_core::key_combo::{KeyCombo, key_name};

    println!("Press the key for {label}:");
    let captured = ghostty_voice_io::input::capture_combo(device)?;
    let combo = KeyCombo {
        modifiers: captured.modifiers,
        key: captured.code,
    };
    let label_name = key_name(captured.code);
    println!(
        "  emitted: {} (code {}){}",
        combo.display(),
        captured.code,
        match label_name {
            Some(_) => "",
            None => "  [unnamed key — it may be remapped]",
        }
    );

    let probes = BindProbes {
        combo,
        // GNOME conflict detection is deliberately gone (evdev sits beneath the
        // compositor); the live press-once test below is the real backstop.
        gsettings_bound: false,
        remapped: label_name.is_none(),
    };
    let warnings = evaluate(&probes);
    if looks_safe(&warnings) {
        println!("  looks clear.");
    } else {
        for w in &warnings {
            println!("  WARNING: {}", w.message);
        }
    }

    live_test(device, &combo)?;
    println!();
    Ok(combo)
}

/// Live "press once" test: ask the user to press the just-bound combo again and
/// confirm the device emits exactly that key (nothing else remapped it). This is
/// the ground-truth conflict check — there is no global binding registry.
fn live_test(
    device: &mut ghostty_voice_io::evdev::Device,
    combo: &ghostty_voice_core::key_combo::KeyCombo,
) -> Result<()> {
    println!("  Live test — press {} once to confirm:", combo.display());
    let seen = ghostty_voice_io::input::capture_combo(device)?;
    if seen.code == combo.key {
        println!("  confirmed: it emits {} as expected.", combo.display());
    } else {
        println!(
            "  MISMATCH: that emitted code {} — something remapped the key. Re-run bind.",
            seen.code
        );
    }
    Ok(())
}

fn config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/ghostty-voice/config.toml"))
}

/// Load config (defaults if missing/invalid) — used by `bind` and `doctor`.
fn load_config() -> ghostty_voice_core::config::Config {
    config_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| ghostty_voice_core::config::Config::from_toml_str(&s).ok())
        .unwrap_or_default()
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
    fn bind_and_doctor_are_local_only_subcommands() {
        assert_eq!(Cmd::Bind.word(), None);
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
