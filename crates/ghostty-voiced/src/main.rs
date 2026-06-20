//! ghostty-voiced — the supervising daemon (S2).
//!
//! Owns all state: supervises whisper-server (eager start, readiness, restart
//! with backoff, VRAM teardown on stop), listens on a Unix socket, drives the
//! recording state machine, and performs record/transcribe/inject. Single
//! utterance at a time for S2; the ordered delivery queue is S3.

mod vulkan_enum;

use std::path::{Path, PathBuf};
use std::process::Child as RecorderChild;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use ghostty_voice_core::config::{Config, expand_tilde};
use ghostty_voice_core::machine::{self, Action};
use ghostty_voice_core::protocol::{Command, ProtocolError, Response, State};
use ghostty_voice_core::supervisor::Backoff;
use ghostty_voice_core::transcript::parse_transcript;
use ghostty_voice_core::vulkan::resolve_device_index;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// All daemon state, behind a single async mutex.
struct Daemon {
    state: State,
    config: Config,
    config_path: PathBuf,
    wav_path: PathBuf,
    recorder: Option<RecorderChild>,
    whisper: Option<tokio::process::Child>,
    shutting_down: bool,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let cfg_path = config_path()?;
    let config = load_config(&cfg_path);
    health_check_ydotoold();

    let socket = socket_path()?;
    let _ = std::fs::remove_file(&socket);
    let listener =
        UnixListener::bind(&socket).with_context(|| format!("binding {}", socket.display()))?;

    let daemon = Arc::new(Mutex::new(Daemon {
        state: State::Loading,
        config,
        config_path: cfg_path,
        wav_path: std::env::temp_dir().join("ghostty-voice-rec.wav"),
        recorder: None,
        whisper: None,
        shutting_down: false,
    }));

    let supervisor = tokio::spawn(supervise(daemon.clone()));

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    info!("ghostty-voiced listening on {}", socket.display());

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    let d = daemon.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_conn(stream, d).await {
                            warn!("connection error: {e}");
                        }
                    });
                }
            }
            _ = sigterm.recv() => break,
            _ = sigint.recv() => break,
        }
    }

    info!("shutting down — freeing VRAM");
    supervisor.abort();
    teardown(&daemon).await;
    let _ = std::fs::remove_file(&socket);
    Ok(())
}

// ---- supervision -----------------------------------------------------------

async fn supervise(daemon: Arc<Mutex<Daemon>>) {
    let backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));
    let mut failures = 0u32;

    loop {
        if daemon.lock().await.shutting_down {
            return;
        }

        let config = daemon.lock().await.config.clone();
        match spawn_whisper(&config).await {
            Ok(child) => {
                daemon.lock().await.whisper = Some(child);
                if probe_ready(&config).await {
                    failures = 0;
                    set_state(&daemon, State::Idle).await;
                    info!("whisper-server ready");
                } else {
                    warn!("whisper-server did not become ready in time");
                }
                wait_for_exit(&daemon).await;
            }
            Err(e) => error!("failed to spawn whisper-server: {e}"),
        }

        if daemon.lock().await.shutting_down {
            return;
        }
        set_state(&daemon, State::Loading).await;
        failures = failures.saturating_add(1);
        notify("ghostty-voice: whisper-server died, restarting");
        tokio::time::sleep(backoff.delay(failures)).await;
    }
}

async fn spawn_whisper(config: &Config) -> Result<tokio::process::Child> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let model = expand_tilde(&config.whisper.model_path, &home);

    let mut cmd = tokio::process::Command::new(&config.whisper.binary);
    cmd.arg("--model")
        .arg(&model)
        .arg("--host")
        .arg(&config.whisper.host)
        .arg("--port")
        .arg(config.whisper.port.to_string())
        .args(&config.whisper.extra_args);

    match resolve_vulkan_index(&config.whisper.vulkan_device) {
        Ok(index) => {
            info!(
                "pinning whisper-server to Vulkan device {index} ({})",
                config.whisper.vulkan_device
            );
            cmd.env("GGML_VK_VISIBLE_DEVICES", index.to_string());
        }
        Err(e) => warn!("could not resolve GPU to pin ({e}); whisper-server will choose itself"),
    }

    cmd.spawn().context("spawning whisper-server")
}

fn resolve_vulkan_index(pci: &str) -> Result<u32> {
    let devices = vulkan_enum::enumerate()?;
    let target = ghostty_voice_core::vulkan::PciAddress::parse(pci)
        .map_err(|e| anyhow::anyhow!("invalid vulkan_device {pci}: {e:?}"))?;
    resolve_device_index(&devices, target).map_err(|e| anyhow::anyhow!("{e:?}"))
}

/// Poll a TCP connection until whisper-server accepts (model loaded), or give up.
async fn probe_ready(config: &Config) -> bool {
    let addr = format!("{}:{}", config.whisper.host, config.whisper.port);
    for _ in 0..240 {
        if TcpStream::connect(&addr).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

async fn wait_for_exit(daemon: &Arc<Mutex<Daemon>>) {
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let mut d = daemon.lock().await;
        if d.shutting_down {
            return;
        }
        match d.whisper.as_mut().map(|c| c.try_wait()) {
            Some(Ok(Some(_))) | Some(Err(_)) | None => {
                d.whisper = None;
                return;
            }
            Some(Ok(None)) => {}
        }
    }
}

async fn teardown(daemon: &Arc<Mutex<Daemon>>) {
    let mut d = daemon.lock().await;
    d.shutting_down = true;
    if let Some(mut child) = d.recorder.take() {
        let _ = ghostty_voice_io::audio::stop_recorder(&mut child);
    }
    if let Some(mut child) = d.whisper.take() {
        let _ = child.start_kill();
    }
}

// ---- socket / command handling --------------------------------------------

async fn handle_conn(stream: UnixStream, daemon: Arc<Mutex<Daemon>>) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response = process_command(&line, &daemon).await;
    write_half
        .write_all(format!("{}\n", response.encode()).as_bytes())
        .await?;
    Ok(())
}

async fn process_command(line: &str, daemon: &Arc<Mutex<Daemon>>) -> Response {
    let command = match Command::parse(line) {
        Ok(c) => c,
        Err(ProtocolError::UnknownCommand(word)) => {
            return Response::Err(format!("unknown command: {word}"));
        }
    };

    let mut d = daemon.lock().await;
    let transition = machine::apply(d.state, command);
    match perform(&mut d, daemon, transition.action).await {
        Ok(()) => {
            d.state = transition.next;
            transition.response
        }
        Err(e) => Response::Err(e.to_string()),
    }
}

async fn perform(d: &mut Daemon, daemon: &Arc<Mutex<Daemon>>, action: Action) -> Result<()> {
    match action {
        Action::None => Ok(()),
        Action::StartRecording => {
            let child =
                ghostty_voice_io::audio::spawn_recorder(&d.config.audio.device, &d.wav_path)?;
            d.recorder = Some(child);
            Ok(())
        }
        Action::DiscardRecording => {
            if let Some(mut child) = d.recorder.take() {
                ghostty_voice_io::audio::stop_recorder(&mut child)?;
            }
            let _ = std::fs::remove_file(&d.wav_path);
            Ok(())
        }
        Action::StopAndTranscribe => {
            if let Some(mut child) = d.recorder.take() {
                ghostty_voice_io::audio::stop_recorder(&mut child)?;
            }
            spawn_transcription(daemon.clone(), d.config.clone(), d.wav_path.clone());
            Ok(())
        }
        Action::ReloadConfig => {
            d.config = load_config(&d.config_path);
            Ok(())
        }
    }
}

fn spawn_transcription(daemon: Arc<Mutex<Daemon>>, config: Config, wav: PathBuf) {
    tokio::spawn(async move {
        if let Err(e) = transcribe_and_type(&config, &wav).await {
            error!("transcription failed: {e}");
            notify(&format!("ghostty-voice: transcription failed — {e}"));
        }
        set_state(&daemon, State::Idle).await;
    });
}

async fn transcribe_and_type(config: &Config, wav: &Path) -> Result<()> {
    let host = config.whisper.host.clone();
    let port = config.whisper.port;
    let wav_owned = wav.to_path_buf();
    let body = tokio::task::spawn_blocking(move || {
        ghostty_voice_io::transcribe::post_inference(&host, port, &wav_owned)
    })
    .await??;

    let transcript = parse_transcript(&body).map_err(|e| anyhow::anyhow!("parse: {e:?}"))?;
    if transcript.is_empty() {
        info!("no speech detected — nothing typed");
        return Ok(());
    }

    let key_delay = config.inject.key_delay_ms;
    tokio::task::spawn_blocking(move || {
        ghostty_voice_io::inject::type_text(&transcript, key_delay)
    })
    .await??;
    Ok(())
}

// ---- helpers ---------------------------------------------------------------

async fn set_state(daemon: &Arc<Mutex<Daemon>>, state: State) {
    daemon.lock().await.state = state;
}

fn config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/ghostty-voice/config.toml"))
}

fn load_config(path: &std::path::Path) -> Config {
    match std::fs::read_to_string(path) {
        Ok(s) => match Config::from_toml_str(&s) {
            Ok(c) => c,
            Err(e) => {
                error!("invalid config {}: {e:?} — using defaults", path.display());
                Config::default()
            }
        },
        Err(_) => Config::default(),
    }
}

fn socket_path() -> Result<PathBuf> {
    let dir = std::env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
    Ok(PathBuf::from(dir).join("ghostty-voice.sock"))
}

fn health_check_ydotoold() {
    let socket = std::env::var("YDOTOOL_SOCKET")
        .unwrap_or_else(|_| "/run/user/0/.ydotool_socket".to_owned());
    if !std::path::Path::new(&socket).exists() {
        warn!("ydotoold socket not found at {socket} — injection will fail until it runs");
        notify("ghostty-voice: ydotoold not reachable — injection will fail");
    }
}

fn notify(message: &str) {
    let _ = std::process::Command::new("notify-send")
        .arg(message)
        .status();
}
