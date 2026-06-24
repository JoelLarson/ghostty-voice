//! ghostty-voiced — the supervising daemon.
//!
//! Owns all state: supervises whisper-server (eager start, readiness, restart
//! with backoff, VRAM teardown on stop), listens on a Unix socket, drives the
//! recording state machine, and performs record/transcribe/inject.
//!
//! Delivery: the Recorder frees on stop so recordings can be fired
//! back-to-back; each utterance is cached as a WAV, enqueued into the ordered
//! [`DeliveryQueue`], and transcribed in the background (retrying while the
//! server is down). A single serialized drain caches each transcript to disk
//! *before* typing, then auto-types it if fresh or holds it for `replay-last`
//! if stale — strict record-order, never interleaved.

mod vulkan_enum;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Child as RecorderChild;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ghostty_voice_core::config::{Config, expand_tilde};
use ghostty_voice_core::machine::{self, Action};
use ghostty_voice_core::protocol::{
    Command, Frame, PROTOCOL_VERSION, ProtocolError, Response, State, StatusReport,
    version_compatible,
};
use ghostty_voice_core::queue::DeliveryQueue;
use ghostty_voice_core::sink::{Route, SinkId, SinkRegistry};
use ghostty_voice_core::supervisor::Backoff;
use ghostty_voice_core::transcript::parse_transcript;
use ghostty_voice_core::vulkan::resolve_device_index;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::sync::{Mutex, mpsc, watch};
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
    /// XDG cache root: holds `recordings/` and `transcripts/`.
    cache_root: PathBuf,
    /// Held true while a drain is in flight so typing never interleaves.
    draining: bool,
    /// The active Continuous-mode session, if any. The recorder/`current_wav`
    /// machinery is bypassed for continuous sessions — sox writes numbered clips
    /// into the session dir and the driver task owns the pipeline.
    continuous: Option<ContinuousSession>,
    /// Monotonically incremented each time a continuous session starts, so a
    /// session's driver task can tell it has been superseded/cancelled and stop.
    continuous_gen: u64,
    /// The active **Delivery sink** tracker (IDEAS.md #4): at most one
    /// `talk-to` **wrapper sink** is active at a time; with none registered there
    /// is no active sink and deliveries are held for replay.
    sinks: SinkRegistry,
    /// Push channels to each registered wrapper sink's persistent connection,
    /// keyed by sink id. The drain writes a pre-encoded [`Frame`] line here to
    /// deliver a Transcript to that wrapper's PTY.
    sink_conns: HashMap<SinkId, mpsc::UnboundedSender<String>>,
    /// Each in-flight utterance's **bound target sink**, keyed by its queue seq.
    /// Bound at *trigger time* (when recording starts/stops), never at delivery
    /// time — so switching sinks mid-utterance can't misroute it, and a bound
    /// wrapper that dies before delivery is Held-for-replay, not redirected.
    /// `None` means no wrapper was active at trigger time (nowhere to deliver).
    bindings: HashMap<u64, Option<SinkId>>,
    /// Broadcasts daemon [`State`] changes to every registered wrapper sink so
    /// each one's status strip stays live. Updated via [`Daemon::set_state`].
    state_tx: watch::Sender<State>,
    shutting_down: bool,
}

/// State for one in-flight Continuous-mode session.
struct ContinuousSession {
    /// This session's generation; the driver task stops once it no longer
    /// matches `Daemon::continuous_gen` (cancel started/superseded the session).
    generation: u64,
    /// Directory holding this session's numbered clip WAVs (`clip-%n.wav`).
    dir: PathBuf,
    /// The sox child spraying clips; SIGINT-stopped on session-end or cancel.
    recorder: Option<RecorderChild>,
    /// When the latest clip last advanced — the session-end-silence countdown
    /// is measured from here (no new clip for `session_end_silence` ⇒ end).
    last_progress: Instant,
}

impl Daemon {
    /// Set the observable state and broadcast it to every registered wrapper
    /// sink (so each `talk-to` status strip tracks the daemon live). The single
    /// chokepoint for state changes — call this instead of writing `state`.
    fn set_state(&mut self, state: State) {
        self.state = state;
        let _ = self.state_tx.send(state);
    }
    fn recordings_dir(&self) -> PathBuf {
        self.cache_root.join("recordings")
    }
    fn transcripts_dir(&self) -> PathBuf {
        self.cache_root.join("transcripts")
    }
    /// A fresh per-session clip directory under the cache root.
    fn fresh_session_dir(&self) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        self.cache_root.join(format!("sessions/{nanos}"))
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let cfg_path = config_path()?;
    let config = load_config(&cfg_path);

    let socket = socket_path()?;
    let _ = std::fs::remove_file(&socket);
    let listener =
        UnixListener::bind(&socket).with_context(|| format!("binding {}", socket.display()))?;

    let (state_tx, _state_rx) = watch::channel(State::Loading);
    let daemon = Arc::new(Mutex::new(Daemon {
        state: State::Loading,
        config,
        config_path: cfg_path,
        current_wav: None,
        recorder: None,
        whisper: None,
        queue: DeliveryQueue::new(),
        cache_root: cache_root()?,
        draining: false,
        continuous: None,
        continuous_gen: 0,
        sinks: SinkRegistry::new(),
        sink_conns: HashMap::new(),
        bindings: HashMap::new(),
        state_tx,
        shutting_down: false,
    }));

    let supervisor = tokio::spawn(supervise(daemon.clone()));

    // Triggers are not read here: `talk-to` recognizes the Shift+F9/F10 escape
    // sequences in its own PTY proxy and sends `vad`/`toggle` over this control
    // socket. The daemon owns no input device.

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
    // First-run model fetch: before any whisper-server spawn, ensure the
    // ~3 GB model is on disk. While it downloads the daemon is in `Downloading`
    // and rejects toggle/vad/continuous (the state machine), notifying instead
    // of hanging. A failed fetch leaves the daemon in `Downloading` and retries
    // on the next supervise pass rather than spinning whisper-server with no model.
    ensure_model_present(&daemon).await;

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

/// Ensure the model file is on disk, fetching it on first run.
///
/// Presence-only check (no multi-GB re-hash on every boot): if `model_path` is
/// absent the daemon enters `Downloading`, streams the model from `model_url`
/// with SHA-256 verification, and reports progress through the observable
/// `Downloading(Some(pct))` State (the status strip and `ghostty-voice-ctl
/// status`) rather than `notify-send`. The fetch is retried with backoff until it
/// succeeds — the daemon stays in `Downloading` (commands rejected) the whole
/// time, never spinning up whisper-server against a missing model. Returns once
/// the model is present. The start/complete/failure milestones stay in the
/// journald log for after-the-fact diagnosis.
async fn ensure_model_present(daemon: &Arc<Mutex<Daemon>>) {
    let config = daemon.lock().await.config.clone();
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let model = expand_tilde(&config.whisper.model_path, &home);

    let status =
        ghostty_voice_core::model::classify(model.exists(), None, &config.whisper.model_sha256);
    if !ghostty_voice_core::model::needs_download(&status) {
        return; // already present — straight to Loading
    }

    set_state(daemon, State::Downloading(None)).await;
    info!(
        "model not found at {} — first-run download",
        model.display()
    );

    let backoff = Backoff::new(Duration::from_secs(2), Duration::from_secs(60));
    let mut attempt = 0u32;
    loop {
        if daemon.lock().await.shutting_down {
            return;
        }
        attempt = attempt.saturating_add(1);
        match download_model_once(daemon, &config, &model).await {
            Ok(()) => {
                info!("model download complete: {}", model.display());
                return;
            }
            Err(e) => {
                error!("model download failed (attempt {attempt}): {e:#}");
                tokio::time::sleep(backoff.delay(attempt)).await;
            }
        }
    }
}

/// Whole-percent throttle for download progress.
///
/// A multi-GB fetch fires `on_progress` on every network chunk, but the strip
/// and `status` only care about whole-percent movement — so the percent advances
/// smoothly without thrashing. Given the latest [`Progress::percent`], `update`
/// returns `Some(pct)` the first time each whole percent is seen and `None` for
/// repeats at that percent or while the total is still unknown. Pure and
/// stateful only in the last-emitted percent — unit-tested without any IO.
struct PercentThrottle {
    last: Option<u8>,
}

impl PercentThrottle {
    fn new() -> Self {
        Self { last: None }
    }

    /// Return `Some(pct)` only when the whole percent advances to a new value;
    /// `None` for a repeat of the current percent or the percent-unknown phase.
    fn update(&mut self, pct: Option<u8>) -> Option<u8> {
        match pct {
            Some(p) if self.last != Some(p) => {
                self.last = Some(p);
                Some(p)
            }
            _ => None,
        }
    }
}

/// One model-download attempt: stream + SHA-verify into place, streaming real
/// progress into the observable [`State`] so the **status strip** and
/// `ghostty-voice-ctl status` both report it from one source of truth.
///
/// The attempt opens in `Downloading(None)` (percent unknown) — so a retry
/// restarts the percent — then the blocking transfer runs off the async runtime
/// in `spawn_blocking`. Its `on_progress` closure feeds a [`PercentThrottle`] and
/// sends whole-percent updates across the sync→async boundary over a channel; a
/// concurrent task applies each via `set_state(Downloading(Some(pct)))`, keeping
/// the cached state (read by `status`) and the `watch<State>` broadcast (read by
/// the strip) in lockstep. Progress is no longer surfaced via `notify-send`; the
/// journald log records the milestones for after-the-fact diagnosis.
async fn download_model_once(
    daemon: &Arc<Mutex<Daemon>>,
    config: &Config,
    dest: &Path,
) -> Result<()> {
    let url = config.whisper.model_url.clone();
    let sha = config.whisper.model_sha256.clone();
    let dest = dest.to_path_buf();

    // Each attempt starts indeterminate, so a retry visibly restarts its percent.
    set_state(daemon, State::Downloading(None)).await;

    // Carry whole-percent updates from the blocking transfer (sync) into the
    // async world that owns `set_state`.
    let (tx, mut rx) = mpsc::unbounded_channel::<u8>();
    let applier_daemon = daemon.clone();
    let applier = tokio::spawn(async move {
        while let Some(pct) = rx.recv().await {
            set_state(&applier_daemon, State::Downloading(Some(pct))).await;
        }
    });

    let transfer = tokio::task::spawn_blocking(move || {
        let mut throttle = PercentThrottle::new();
        ghostty_voice_io::download::download_model(&url, &dest, &sha, |p| {
            if let Some(pct) = throttle.update(p.percent()) {
                // The receiver lives until the transfer ends; a send failure only
                // means the daemon is shutting down, so it's safe to ignore.
                let _ = tx.send(pct);
            }
        })
    })
    .await;

    // The closure (and its `tx`) is dropped when the transfer task ends, closing
    // the channel so the applier finishes draining and returns.
    let _ = applier.await;
    transfer?
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
    if let Some(mut session) = d.continuous.take() {
        if let Some(mut child) = session.recorder.take() {
            let _ = ghostty_voice_io::audio::stop_recorder(&mut child);
        }
        let _ = std::fs::remove_dir_all(&session.dir);
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

    // `register-sink` is the one persistent command: the connection stays open
    // and the daemon *pushes* frames down it until the client disconnects. The
    // client carries its PROTOCOL_VERSION; an incompatible version is
    // refused with an explicit `err incompatible …` so `talk-to` can show
    // `incompatible` rather than a generic offline. A legacy bare register-sink
    // (no version) is still accepted.
    if let Ok(Command::RegisterSink(version)) = Command::parse(&line) {
        if let Some(v) = version
            && !version_compatible(v, PROTOCOL_VERSION)
        {
            warn!(
                "rejecting wrapper sink: incompatible protocol v{v} (daemon v{PROTOCOL_VERSION})"
            );
            let msg = format!(
                "incompatible protocol version (daemon speaks {PROTOCOL_VERSION}, client sent {v}) — restart/upgrade the daemon"
            );
            write_half
                .write_all(format!("{}\n", Response::Err(msg).encode()).as_bytes())
                .await?;
            return Ok(());
        }
        return serve_sink(reader, write_half, daemon).await;
    }

    // `status` answers with the richer [`StatusReport`]: the daemon
    // state plus the active **Delivery sink** and the registered wrapper count, so
    // a user can confirm routing without tailing journald. It is always allowed
    // and read-only, so reading state directly matches the machine's status no-op.
    if matches!(Command::parse(&line), Ok(Command::Status)) {
        let report = status_report(&daemon).await;
        write_half
            .write_all(format!("{}\n", report.encode()).as_bytes())
            .await?;
        return Ok(());
    }

    let response = process_command(&line, &daemon).await;
    write_half
        .write_all(format!("{}\n", response.encode()).as_bytes())
        .await?;
    Ok(())
}

/// Snapshot the daemon state and how many **wrapper sinks** are registered into a
/// [`StatusReport`] — `wrappers=0` means there is nowhere to deliver.
async fn status_report(daemon: &Arc<Mutex<Daemon>>) -> StatusReport {
    let d = daemon.lock().await;
    StatusReport {
        state: d.state,
        wrapper_count: d.sinks.wrapper_count(),
    }
}

/// Serve a registered **wrapper sink** (`talk-to`, IDEAS.md #4) on a persistent
/// connection. Registration makes this the active Delivery sink; the daemon then
/// pushes [`Frame`]s — `state` changes (from the watch channel) for the status
/// strip and `transcript` deliveries (from this sink's mpsc) — until the
/// client disconnects, at which point the newest still-live wrapper (if any)
/// takes over, or there is no active sink.
async fn serve_sink(
    mut reader: BufReader<OwnedReadHalf>,
    mut write_half: OwnedWriteHalf,
    daemon: Arc<Mutex<Daemon>>,
) -> Result<()> {
    // Register and capture this sink's push channel + a state subscription. Done
    // under one lock so the initial state we send matches what the watch will
    // report changes against (no missed/duplicated state push).
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let (id, mut state_rx, current) = {
        let mut d = daemon.lock().await;
        let id = d.sinks.register();
        d.sink_conns.insert(id, tx);
        (id, d.state_tx.subscribe(), d.state)
    };
    info!("wrapper sink {id:?} registered — now the active Delivery sink");

    // Push the current state immediately so the strip is correct on connect.
    let _ = write_half
        .write_all(format!("{}\n", Frame::State(current).encode()).as_bytes())
        .await;

    let mut sink_line = String::new();
    loop {
        sink_line.clear();
        tokio::select! {
            // A pushed Transcript (or other pre-encoded frame) to deliver.
            msg = rx.recv() => {
                match msg {
                    Some(line) => {
                        if write_half.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    None => break, // daemon dropped the sender
                }
            }
            // A daemon state change → push it to the strip.
            changed = state_rx.changed() => {
                if changed.is_err() {
                    break; // state channel closed (shutdown)
                }
                let s = *state_rx.borrow();
                if write_half
                    .write_all(format!("{}\n", Frame::State(s).encode()).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
            }
            // Detect client disconnect: a registered sink sends nothing more, so
            // any read returning 0 (EOF) or erroring means `talk-to` is gone.
            read = reader.read_line(&mut sink_line) => {
                match read {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {} // unexpected extra input — ignore
                }
            }
        }
    }

    // Deregister: hand off to the newest still-live wrapper, or no active sink.
    {
        let mut d = daemon.lock().await;
        d.sinks.deregister(id);
        d.sink_conns.remove(&id);
    }
    info!("wrapper sink {id:?} deregistered");
    Ok(())
}

async fn process_command(line: &str, daemon: &Arc<Mutex<Daemon>>) -> Response {
    let command = match Command::parse(line) {
        Ok(c) => c,
        Err(ProtocolError::UnknownCommand(word)) => {
            return Response::Err(format!("unknown command: {word}"));
        }
    };
    apply_command(command, daemon).await
}

/// Drive one command through the state machine and perform its action. Shared by
/// the control socket and the evdev input listener (gestures resolve to the same
/// `Command`s), so both paths obey identical transition rules.
async fn apply_command(command: Command, daemon: &Arc<Mutex<Daemon>>) -> Response {
    let mut d = daemon.lock().await;
    let transition = machine::apply(d.state, command);
    match perform(&mut d, daemon, transition.action).await {
        Ok(()) => {
            d.set_state(transition.next);
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
        Action::StartContinuous => {
            start_continuous(d, daemon)?;
            Ok(())
        }
        Action::DiscardRecording => {
            // Cancel aborts the whole continuous session (if one is active): the
            // generation bump retires its driver task, and we stop sox and bin
            // the clip dir so nothing is transcribed or delivered.
            if let Some(mut session) = d.continuous.take() {
                d.continuous_gen += 1;
                if let Some(mut child) = session.recorder.take() {
                    let _ = ghostty_voice_io::audio::stop_recorder(&mut child);
                }
                let _ = std::fs::remove_dir_all(&session.dir);
                info!("continuous session cancelled — discarded");
            }
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

/// How many trailing transcript words seed the next clip's `initial_prompt`.
const CLIP_CHAIN_WORDS: usize = 24;

/// Start a Continuous-mode session: make a fresh clip dir, launch `sox`
/// to spray numbered silence-bounded clips into it, and spawn the driver task
/// that watches the dir, transcribes finalized clips serially (context-chained),
/// and on the long session-end silence assembles and delivers once.
fn start_continuous(d: &mut Daemon, daemon: &Arc<Mutex<Daemon>>) -> Result<()> {
    let dir = d.fresh_session_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let template = dir.join("clip-%n.wav");

    let child = ghostty_voice_io::audio::spawn_continuous_recorder(
        &d.config.audio.device,
        &template,
        d.config.audio.clip_cut_pause_seconds,
        d.config.audio.vad_threshold_pct,
    )?;

    d.continuous_gen += 1;
    let generation = d.continuous_gen;
    d.continuous = Some(ContinuousSession {
        generation,
        dir,
        recorder: Some(child),
        last_progress: Instant::now(),
    });
    play_start_cue(&d.config);
    info!("continuous session {generation} started");

    tokio::spawn(drive_continuous(daemon.clone(), generation));
    Ok(())
}

/// Drive one Continuous-mode session to completion: poll the clip dir, transcribe
/// each finalized clip in strict order (seeded with the running transcript tail),
/// and end the session on `session_end_silence` of no progress — stopping sox,
/// transcribing any remaining clips, then delivering the assembled transcript
/// exactly once through the delivery queue. Retires immediately if the
/// session is cancelled (generation bumped) or the daemon shuts down.
async fn drive_continuous(daemon: Arc<Mutex<Daemon>>, generation: u64) {
    let mut session = ghostty_voice_core::session::Session::new(CLIP_CHAIN_WORDS);
    let mut transcribed = 0usize;
    let mut last_present = 0usize;

    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Snapshot what we need without holding the lock across transcription.
        let (config, dir, sox_running, end_silence) = {
            let mut d = daemon.lock().await;
            if d.shutting_down {
                return;
            }
            let Some(s) = d.continuous.as_mut() else {
                return; // cancelled/ended elsewhere
            };
            if s.generation != generation {
                return; // a newer session superseded this one
            }
            let sox_running = match s.recorder.as_mut() {
                Some(child) => matches!(child.try_wait(), Ok(None)),
                None => false,
            };
            let dir = s.dir.clone();
            let end_silence = Duration::from_secs_f32(d.config.audio.session_end_silence_seconds);
            (d.config.clone(), dir, sox_running, end_silence)
        };

        let present = present_clip_count(&dir);
        let finalized = ghostty_voice_core::session::finalized_clip_count(present, sox_running);

        // A new clip opening means the user spoke recently — reset the
        // session-end countdown even before that clip is transcribed, so slow
        // transcription can't make an active session look silent.
        let mut made_progress = present > last_present;
        last_present = present;

        // Transcribe any newly-finalized clips, in strict order, chaining the
        // running transcript tail into each one's initial_prompt.
        while transcribed < finalized {
            let clip = clip_path(&dir, transcribed + 1);
            let prompt_tail = session.prompt_for_next();
            match transcribe_clip(&daemon, &config, &clip, &prompt_tail).await {
                Ok(text) => session.push_clip(&text),
                Err(e) => {
                    warn!("continuous clip {} failed: {e} — skipping", transcribed + 1);
                    session.push_clip("");
                }
            }
            transcribed += 1;
            made_progress = true;
        }

        // Mark progress so the session-end countdown only fires on real silence.
        if made_progress {
            let mut d = daemon.lock().await;
            if let Some(s) = d.continuous.as_mut()
                && s.generation == generation
            {
                s.last_progress = Instant::now();
            }
        }

        // End the session once sox has finished AND every clip is transcribed,
        // or on a long silence with no new clip (sox should self-stop, but the
        // daemon owns the session-end decision as the backstop).
        let should_end = {
            let mut d = daemon.lock().await;
            let Some(s) = d.continuous.as_mut() else {
                return;
            };
            if s.generation != generation {
                return;
            }
            let silent_for = s.last_progress.elapsed();
            (!sox_running && transcribed >= present)
                || (silent_for >= end_silence && transcribed >= finalized)
        };

        if should_end {
            end_continuous(&daemon, generation, &dir, session.assembled()).await;
            return;
        }
    }
}

/// Count this session's `clip-NN.wav` files currently present in `dir`.
fn present_clip_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| n.starts_with("clip-") && n.ends_with(".wav"))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

/// The path of clip `n` (1-based) in `dir`, matching sox's zero-padded `%n`
/// naming (`clip-01.wav`); falls back across pad widths so a present file is found.
fn clip_path(dir: &Path, n: usize) -> PathBuf {
    for width in [2usize, 1, 3, 4] {
        let candidate = dir.join(format!("clip-{n:0width$}.wav"));
        if candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("clip-{n:02}.wav"))
}

/// Transcribe one continuous clip with the accuracy stack, overriding the
/// `initial_prompt` to chain the running session transcript tail for cross-clip
/// context. Empty/hallucination/sub-min clips return an empty string (dropped
/// from assembly). The clip WAV is removed after transcription.
async fn transcribe_clip(
    daemon: &Arc<Mutex<Daemon>>,
    config: &Config,
    clip: &Path,
    prompt_tail: &str,
) -> Result<String> {
    let mut params =
        ghostty_voice_io::transcribe::InferenceParams::from_whisper_config(&config.whisper);
    if !prompt_tail.is_empty() {
        params.initial_prompt = if params.initial_prompt.is_empty() {
            prompt_tail.to_owned()
        } else {
            format!("{} {prompt_tail}", params.initial_prompt)
        };
    }

    let min_duration = Duration::from_secs_f64(config.audio.min_duration_seconds);
    let audio_duration = ghostty_voice_io::audio::wav_duration(clip).unwrap_or(Duration::ZERO);
    // A zero/sub-min clip (e.g. sox's trailing header-only restart clip) is
    // skipped — it carries no speech.
    if audio_duration < min_duration {
        let _ = std::fs::remove_file(clip);
        return Ok(String::new());
    }

    let host = config.whisper.host.clone();
    let port = config.whisper.port;
    let window = Duration::from_secs(config.cache.retry_window_seconds);
    let started = Instant::now();

    let text = loop {
        let (h, p, w, pr) = (host.clone(), port, clip.to_path_buf(), params.clone());
        let result = tokio::task::spawn_blocking(move || {
            ghostty_voice_io::transcribe::post_inference(&h, p, &w, &pr)
        })
        .await?;
        match result {
            Ok(body) => {
                let raw = parse_transcript(&body).map_err(|e| anyhow::anyhow!("parse: {e:?}"))?;
                break ghostty_voice_core::pipeline::finalize_transcript(
                    &raw,
                    audio_duration,
                    min_duration,
                    &config.corrections,
                )
                .unwrap_or_default();
            }
            Err(e) => {
                if started.elapsed() >= window || daemon.lock().await.shutting_down {
                    let _ = std::fs::remove_file(clip);
                    return Err(e.context("whisper-server unreachable past the retry window"));
                }
                warn!("whisper-server unreachable for clip, retrying: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };

    let _ = std::fs::remove_file(clip);
    Ok(text)
}

/// Finish a Continuous-mode session: stop sox, clear the session state, bin the
/// clip dir, and deliver the assembled transcript exactly once through the
/// delivery queue (cache-before-type ⇒ hands-free auto-type). An empty assembly
/// (all-silence session) delivers nothing — just the done cue.
async fn end_continuous(
    daemon: &Arc<Mutex<Daemon>>,
    generation: u64,
    dir: &Path,
    assembled: String,
) {
    let config = {
        let mut d = daemon.lock().await;
        match d.continuous.as_mut() {
            Some(s) if s.generation == generation => {
                if let Some(mut child) = s.recorder.take() {
                    let _ = ghostty_voice_io::audio::stop_recorder(&mut child);
                }
            }
            _ => return, // already cancelled/superseded
        }
        d.continuous = None;
        if d.state == State::Recording {
            d.set_state(State::Idle);
        }
        d.config.clone()
    };
    let _ = std::fs::remove_dir_all(dir);
    play_stop_cue(&config);

    let trimmed = assembled.trim();
    if trimmed.is_empty() {
        info!("continuous session {generation}: no speech — nothing delivered");
        return;
    }

    // Deliver exactly once: enqueue a ready utterance and drain it through the
    // same cache-before-type ⇒ auto-type path as batch utterances.
    let seq = {
        let mut d = daemon.lock().await;
        let seq = d.queue.enqueue();
        let bound = d.sinks.active();
        d.bindings.insert(seq, bound);
        d.queue.set_ready(seq, trimmed.to_owned());
        seq
    };
    info!("continuous session {generation}: delivering assembled transcript (utterance {seq})");
    drain_queue(daemon).await;
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

    let seq = d.queue.enqueue();
    // Bind the target sink NOW (trigger time), not when the transcript is ready.
    let bound = d.sinks.active();
    d.bindings.insert(seq, bound);
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
                let mut d = daemon.lock().await;
                d.queue.resolve(seq);
                d.bindings.remove(&seq);
            }
            Err(e) => {
                error!("utterance {seq}: transcription failed: {e}");
                notify("ghostty-voice: transcription failed — recording kept, re-speak");
                let mut d = daemon.lock().await;
                d.queue.resolve(seq);
                d.bindings.remove(&seq);
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
/// interleave: cache each transcript to disk *before* delivery, then push it to
/// the bound **wrapper sink** or hold it for `replay-last` if that wrapper is
/// gone. Only one drain runs at a time.
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
        // Route by the utterance's *bound* sink (trigger-time), not whatever is
        // active now — so a bound wrapper that has since died Holds rather than
        // being redirected to a wrapper that happens to be active now.
        let next = {
            let d = daemon.lock().await;
            d.queue.next_to_type().map(|(seq, transcript)| {
                let bound = d.bindings.get(&seq).copied().unwrap_or(None);
                let route = d.sinks.route(bound);
                let sender = match route {
                    Route::Wrapper(id) => d.sink_conns.get(&id).cloned(),
                    Route::Held => None,
                };
                (
                    seq,
                    transcript.to_owned(),
                    route,
                    sender,
                    d.transcripts_dir(),
                    d.config.cache.transcript_keep,
                )
            })
        };

        let Some((seq, transcript, route, sender, tdir, tkeep)) = next else {
            break; // head is pending or queue is empty
        };

        // Cache the transcript BEFORE delivery, so it survives a failed write —
        // recoverable via `replay-last` regardless of which sink it was bound to.
        if let Err(e) = ghostty_voice_io::cache::store_transcript(&tdir, &transcript, tkeep) {
            warn!("could not cache transcript: {e}");
        }

        match route {
            // Wrapper sink: push the Transcript frame down the registered
            // connection; `talk-to` writes it into the agent's PTY (no Enter).
            Route::Wrapper(id) => {
                let pushed = sender.is_some_and(|tx| {
                    tx.send(format!(
                        "{}\n",
                        Frame::Transcript(transcript.clone()).encode()
                    ))
                    .is_ok()
                });
                if pushed {
                    info!("utterance {seq}: delivered to wrapper sink {id:?}");
                } else {
                    // The wrapper vanished between routing and the send — hold.
                    info!("utterance {seq}: wrapper sink {id:?} gone at send — held for replay");
                    notify("ghostty-voice: wrapper exited before delivery — run `replay-last`");
                }
            }
            // The bound wrapper sink died (or none was registered) before
            // delivery: Held-for-replay, recoverable via `replay-last`.
            Route::Held => {
                info!("utterance {seq}: no live bound wrapper sink — held for replay");
                notify("ghostty-voice: transcript held (no talk-to) — run `replay-last`");
            }
        }

        let mut d = daemon.lock().await;
        d.queue.resolve(seq);
        d.bindings.remove(&seq);
    }

    daemon.lock().await.draining = false;
}

/// Re-deliver the most-recent cached transcript into the active **wrapper sink**
/// (recovery-only). Errors when no `talk-to` is registered — there is nowhere to
/// deliver, and it is never redirected to a focused window.
async fn replay_last(d: &Daemon) -> Result<()> {
    let dir = d.transcripts_dir();
    let Some(text) = ghostty_voice_io::cache::latest_transcript(&dir)? else {
        anyhow::bail!("no transcript cached to replay");
    };
    let Some(id) = d.sinks.active() else {
        anyhow::bail!("no talk-to wrapper registered to replay into — launch talk-to first");
    };
    let Some(tx) = d.sink_conns.get(&id) else {
        anyhow::bail!("the active wrapper sink has no live connection");
    };
    tx.send(format!("{}\n", Frame::Transcript(text).encode()))
        .map_err(|_| anyhow::anyhow!("the active wrapper sink connection is closed"))?;
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
            d.set_state(State::Idle);
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
                    d.set_state(State::Idle);
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
    daemon.lock().await.set_state(state);
}

fn config_path() -> Result<PathBuf> {
    ghostty_voice_core::config::config_path().context("HOME is not set")
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
    ghostty_voice_core::config::socket_path().context("XDG_RUNTIME_DIR is not set")
}

/// `$XDG_CACHE_HOME/ghostty-voice/` (falling back to `~/.cache/...`).
fn cache_root() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(dir).join("ghostty-voice"));
    }
    let home = std::env::var("HOME").context("neither XDG_CACHE_HOME nor HOME is set")?;
    Ok(PathBuf::from(home).join(".cache/ghostty-voice"))
}

fn notify(message: &str) {
    let _ = std::process::Command::new("notify-send")
        .arg(message)
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_emits_only_on_a_new_whole_percent() {
        // Many network chunks land inside the same whole percent; the strip must
        // update on whole-percent changes, not flicker on every chunk. The
        // throttle passes the first sighting of each percent and suppresses
        // repeats at that same percent.
        let mut throttle = PercentThrottle::new();
        assert_eq!(
            throttle.update(Some(0)),
            Some(0),
            "first percent is emitted"
        );
        assert_eq!(throttle.update(Some(0)), None, "same percent is suppressed");
        assert_eq!(throttle.update(Some(1)), Some(1), "advancing emits");
        assert_eq!(throttle.update(Some(1)), None);
        assert_eq!(
            throttle.update(Some(42)),
            Some(42),
            "a jump still emits once"
        );
        assert_eq!(throttle.update(Some(42)), None);
    }

    #[test]
    fn throttle_passes_through_the_percent_unknown_phase() {
        // Before a Content-Length is known `percent()` is None; the throttle emits
        // nothing (the daemon stays in Downloading(None)) and does not treat the
        // absence as a change to react to.
        let mut throttle = PercentThrottle::new();
        assert_eq!(throttle.update(None), None);
        assert_eq!(throttle.update(None), None);
        // The first real percent after the unknown phase still emits.
        assert_eq!(throttle.update(Some(3)), Some(3));
    }
}
