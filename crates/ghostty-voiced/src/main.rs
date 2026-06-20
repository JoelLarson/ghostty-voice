//! ghostty-voiced — the supervising daemon (S3).
//!
//! Owns all state: supervises whisper-server (eager start, readiness, restart
//! with backoff, VRAM teardown on stop), listens on a Unix socket, drives the
//! recording state machine, and performs record/transcribe/inject.
//!
//! Delivery (S3): the Recorder frees on stop so recordings can be fired
//! back-to-back; each utterance is cached as a WAV, enqueued into the ordered
//! [`DeliveryQueue`], and transcribed in the background (retrying while the
//! server is down). A single serialized drain caches each transcript to disk
//! *before* typing, then auto-types it if fresh or holds it for `replay-last`
//! if stale — strict record-order, never interleaved.

mod vulkan_enum;

use std::path::{Path, PathBuf};
use std::process::Child as RecorderChild;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ghostty_voice_core::config::{Config, expand_tilde};
use ghostty_voice_core::delivery::Delivery;
use ghostty_voice_core::machine::{self, Action};
use ghostty_voice_core::protocol::{Command, ProtocolError, Response, State};
use ghostty_voice_core::queue::DeliveryQueue;
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
    /// The active recording's WAV path and the seq it will enqueue as, set on
    /// StartRecording and consumed on StopAndEnqueue.
    current_wav: Option<PathBuf>,
    recorder: Option<RecorderChild>,
    whisper: Option<tokio::process::Child>,
    /// Ordered delivery queue — utterances drain in strict record-order.
    queue: DeliveryQueue,
    /// Monotonic base for per-utterance freshness offsets.
    clock_base: Instant,
    /// XDG cache root: holds `recordings/` and `transcripts/`.
    cache_root: PathBuf,
    /// Held true while a drain is in flight so typing never interleaves.
    draining: bool,
    shutting_down: bool,
}

impl Daemon {
    fn now_offset(&self) -> Duration {
        self.clock_base.elapsed()
    }
    fn recordings_dir(&self) -> PathBuf {
        self.cache_root.join("recordings")
    }
    fn transcripts_dir(&self) -> PathBuf {
        self.cache_root.join("transcripts")
    }
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
        current_wav: None,
        recorder: None,
        whisper: None,
        queue: DeliveryQueue::new(),
        clock_base: Instant::now(),
        cache_root: cache_root()?,
        draining: false,
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
            let wav = fresh_wav_path();
            let child = ghostty_voice_io::audio::spawn_recorder(&d.config.audio.device, &wav)?;
            d.recorder = Some(child);
            d.current_wav = Some(wav);
            play_start_cue(&d.config);
            arm_recording_cap(daemon.clone(), d.config.audio.max_recording_seconds);
            Ok(())
        }
        Action::StartVadRecording => {
            // Hands-free: sox self-terminates on the first trailing silence. We
            // still track the child so a `toggle` can stop it early, and the
            // max_recording_seconds cap backstops a never-speak hang. A watcher
            // notices sox's own exit and enqueues the utterance.
            let wav = fresh_wav_path();
            let child = ghostty_voice_io::audio::spawn_vad_recorder(
                &d.config.audio.device,
                &wav,
                d.config.audio.vad_silence_seconds,
                d.config.audio.vad_threshold_pct,
            )?;
            d.recorder = Some(child);
            d.current_wav = Some(wav);
            play_start_cue(&d.config);
            arm_recording_cap(daemon.clone(), d.config.audio.max_recording_seconds);
            watch_vad_autostop(daemon.clone());
            Ok(())
        }
        Action::DiscardRecording => {
            if let Some(mut child) = d.recorder.take() {
                ghostty_voice_io::audio::stop_recorder(&mut child)?;
            }
            if let Some(wav) = d.current_wav.take() {
                let _ = std::fs::remove_file(&wav);
            }
            Ok(())
        }
        Action::StopAndEnqueue => {
            stop_and_enqueue(d, daemon).await;
            Ok(())
        }
        Action::ReloadConfig => {
            d.config = load_config(&d.config_path);
            Ok(())
        }
        Action::ReplayLast => replay_last(d).await,
    }
}

/// Stop the recorder, cache the WAV, enqueue the utterance with its freshness
/// deadline, play the stop/done cue, and kick off background transcription.
async fn stop_and_enqueue(d: &mut Daemon, daemon: &Arc<Mutex<Daemon>>) {
    if let Some(mut child) = d.recorder.take() {
        let _ = ghostty_voice_io::audio::stop_recorder(&mut child);
    }
    let Some(wav) = d.current_wav.take() else {
        return; // no active recording (e.g. cap already fired)
    };

    // Cache the WAV (the accuracy-debugging corpus); keep the working copy too.
    let rec_dir = d.recordings_dir();
    let keep = d.config.cache.wav_keep;
    if let Err(e) = ghostty_voice_io::cache::store_wav(&rec_dir, &wav, keep) {
        warn!("could not cache recording: {e}");
    }

    let record_end = d.now_offset();
    let seq = d.queue.enqueue_at(record_end);
    play_stop_cue(&d.config);

    spawn_transcription(daemon.clone(), seq, wav);
}

/// Background-transcribe utterance `seq`, retrying while the server is down
/// (within the freshness window), then either mark it ready or resolve it.
fn spawn_transcription(daemon: Arc<Mutex<Daemon>>, seq: u64, wav: PathBuf) {
    tokio::spawn(async move {
        let config = daemon.lock().await.config.clone();
        match transcribe_with_retry(&daemon, &config, &wav).await {
            Ok(Some(transcript)) => {
                daemon.lock().await.queue.set_ready(seq, transcript);
            }
            Ok(None) => {
                // Empty/silence: the stop cue already played; type nothing.
                info!("utterance {seq}: no speech detected — nothing typed");
                daemon.lock().await.queue.resolve(seq);
            }
            Err(e) => {
                error!("utterance {seq}: transcription failed: {e}");
                notify("ghostty-voice: transcription failed — recording kept, re-speak");
                daemon.lock().await.queue.resolve(seq);
            }
        }
        let _ = std::fs::remove_file(&wav);
        drain_queue(&daemon).await;
    });
}

/// POST the WAV to whisper-server, retrying while it is unreachable (a
/// mid-restart hiccup) until it comes back or the freshness window elapses.
/// `Ok(None)` means an empty/silence transcript.
async fn transcribe_with_retry(
    daemon: &Arc<Mutex<Daemon>>,
    config: &Config,
    wav: &Path,
) -> Result<Option<String>> {
    let host = config.whisper.host.clone();
    let port = config.whisper.port;
    let window = Duration::from_secs(config.cache.retry_window_seconds);
    let params =
        ghostty_voice_io::transcribe::InferenceParams::from_whisper_config(&config.whisper);
    if params.prompt_truncated {
        warn!(
            "initial_prompt vocab exceeds the ~224-token cap — later terms dropped; trim [whisper].vocab"
        );
    }

    // Sub-min-duration recordings (accidental blips) are discarded up front — no
    // need to bother whisper-server. `should_discard` re-checks duration too.
    let min_duration = Duration::from_secs_f64(config.audio.min_duration_seconds);
    let audio_duration = ghostty_voice_io::audio::wav_duration(wav).unwrap_or_else(|e| {
        warn!("could not read WAV duration ({e}); skipping the duration filter");
        min_duration // treat as just-long-enough so the text filter still runs
    });
    if audio_duration < min_duration {
        info!("recording shorter than min_duration — discarded, nothing typed");
        return Ok(None);
    }

    let started = Instant::now();

    loop {
        let (h, p, w, params) = (host.clone(), port, wav.to_path_buf(), params.clone());
        let result = tokio::task::spawn_blocking(move || {
            ghostty_voice_io::transcribe::post_inference(&h, p, &w, &params)
        })
        .await?;

        match result {
            Ok(body) => {
                let transcript =
                    parse_transcript(&body).map_err(|e| anyhow::anyhow!("parse: {e:?}"))?;
                // Pure accuracy pipeline: discard junk (type nothing) or correct
                // the surviving transcript before it is typed.
                let final_text = ghostty_voice_core::pipeline::finalize_transcript(
                    &transcript,
                    audio_duration,
                    min_duration,
                    &config.corrections,
                );
                if final_text.is_none() {
                    info!("transcript filtered (empty/hallucination) — nothing typed");
                }
                return Ok(final_text);
            }
            Err(e) => {
                if started.elapsed() >= window || daemon.lock().await.shutting_down {
                    return Err(e.context("whisper-server unreachable past the retry window"));
                }
                warn!("whisper-server unreachable, retrying: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

/// Drain the delivery queue head-first, serialized so utterances never
/// interleave: cache each transcript to disk *before* typing, then auto-type if
/// fresh or hold-for-replay if stale. Only one drain runs at a time.
async fn drain_queue(daemon: &Arc<Mutex<Daemon>>) {
    {
        let mut d = daemon.lock().await;
        if d.draining {
            return; // another drain is already advancing the queue
        }
        d.draining = true;
    }

    loop {
        // Decide the next action while holding the lock, then release it to do IO.
        let next = {
            let d = daemon.lock().await;
            let now = d.now_offset();
            let window = Duration::from_secs(d.config.cache.retry_window_seconds);
            d.queue
                .head_delivery(now, window)
                .map(|(seq, transcript, delivery)| {
                    (
                        seq,
                        transcript.to_owned(),
                        delivery,
                        d.transcripts_dir(),
                        d.config.cache.transcript_keep,
                        d.config.inject.key_delay_ms,
                    )
                })
        };

        let Some((seq, transcript, delivery, tdir, tkeep, key_delay)) = next else {
            break; // head is pending or queue is empty
        };

        // Cache the transcript BEFORE typing, so delivery survives a type fail.
        if let Err(e) = ghostty_voice_io::cache::store_transcript(&tdir, &transcript, tkeep) {
            warn!("could not cache transcript: {e}");
        }

        match delivery {
            Delivery::AutoType => {
                let text = transcript.clone();
                let typed = tokio::task::spawn_blocking(move || {
                    ghostty_voice_io::inject::type_text(&text, key_delay)
                })
                .await;
                match typed {
                    Ok(Ok(())) => info!("utterance {seq}: auto-typed"),
                    Ok(Err(e)) => {
                        // Typing failed (e.g. ydotoold down). The transcript is
                        // already cached, so recover with `replay-last`.
                        error!("utterance {seq}: type failed: {e}");
                        notify("ghostty-voice: type failed — run `replay-last` after refocusing");
                    }
                    Err(join) => error!("utterance {seq}: type task panicked: {join}"),
                }
            }
            Delivery::HoldForReplay => {
                info!("utterance {seq}: held for replay (stale)");
                notify("ghostty-voice: transcript held — run `replay-last` after refocusing");
            }
        }

        daemon.lock().await.queue.resolve(seq);
    }

    daemon.lock().await.draining = false;
}

/// Re-inject the most-recent cached transcript (recovery-only).
async fn replay_last(d: &Daemon) -> Result<()> {
    let dir = d.transcripts_dir();
    let key_delay = d.config.inject.key_delay_ms;
    let Some(text) = ghostty_voice_io::cache::latest_transcript(&dir)? else {
        anyhow::bail!("no transcript cached to replay");
    };
    tokio::task::spawn_blocking(move || ghostty_voice_io::inject::type_text(&text, key_delay))
        .await??;
    Ok(())
}

/// Arm the runaway-recording cap: after `seconds`, if still recording the same
/// utterance, stop + enqueue it (preserving speech) and notify.
fn arm_recording_cap(daemon: Arc<Mutex<Daemon>>, seconds: u64) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        let mut d = daemon.lock().await;
        if d.recorder.is_some() {
            warn!("max_recording_seconds reached — stopping and enqueueing");
            notify("ghostty-voice: recording hit the time cap — stopped and queued");
            let dc = daemon.clone();
            stop_and_enqueue(&mut d, &dc).await;
            d.state = State::Idle;
        }
    });
}

/// Watch a VAD recording for `sox`'s own exit (it self-terminates on the first
/// trailing silence). When sox exits while still the active recorder, enqueue
/// the utterance — the hands-free auto-stop. A manual `toggle`/`cancel` or the
/// time cap takes the recorder first, in which case this watcher finds nothing
/// to do and returns.
fn watch_vad_autostop(daemon: Arc<Mutex<Daemon>>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let mut d = daemon.lock().await;
            if d.shutting_down {
                return;
            }
            let exited = match d.recorder.as_mut().map(|c| c.try_wait()) {
                // No recorder means a toggle/cancel/cap already took it: stop.
                None => return,
                Some(Ok(Some(_))) | Some(Err(_)) => true,
                Some(Ok(None)) => false,
            };
            if exited {
                // sox already exited and flushed its WAV; drop the handle so
                // stop_and_enqueue doesn't try to SIGINT a dead process.
                d.recorder = None;
                if d.current_wav.is_some() {
                    info!("VAD: sox auto-stopped on silence — enqueueing utterance");
                    let dc = daemon.clone();
                    stop_and_enqueue(&mut d, &dc).await;
                    d.state = State::Idle;
                }
                return;
            }
        }
    });
}

fn fresh_wav_path() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "ghostty-voice-rec-{}-{}.wav",
        std::process::id(),
        nanos
    ))
}

fn play_start_cue(config: &Config) {
    let sound = config.feedback.sound_start.clone();
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || ghostty_voice_io::cue::play(&sound)).await;
    });
}

fn play_stop_cue(config: &Config) {
    let sound = config.feedback.sound_stop.clone();
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || ghostty_voice_io::cue::play(&sound)).await;
    });
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

/// `$XDG_CACHE_HOME/ghostty-voice/` (falling back to `~/.cache/...`).
fn cache_root() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(dir).join("ghostty-voice"));
    }
    let home = std::env::var("HOME").context("neither XDG_CACHE_HOME nor HOME is set")?;
    Ok(PathBuf::from(home).join(".cache/ghostty-voice"))
}

fn health_check_ydotoold() {
    let socket = std::env::var("YDOTOOL_SOCKET").unwrap_or_else(|_| {
        std::env::var("XDG_RUNTIME_DIR")
            .map(|dir| format!("{dir}/.ydotool_socket"))
            .unwrap_or_else(|_| "/tmp/.ydotool_socket".to_owned())
    });
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
