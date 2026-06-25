//! talk-to — a PTY wrapper that injects voice into a wrapped agent (IDEAS.md #4).
//!
//! Spawns `<command>` on a pseudo-terminal, forwards bytes verbatim in raw mode,
//! tracks terminal resize onto the child winsize, paints a bottom voice-status
//! strip, and registers with the daemon as a **wrapper sink** so finished
//! Transcripts are injected into the child's PTY with NO trailing newline
//! (review-before-Enter). The wrapped command is just a command, so
//! `talk-to ssh host claude` works unchanged: injected bytes ride the existing
//! ssh stdin pipe to the remote agent.
//!
//! This binary is the OS-glue boundary (forkpty / termios / poll); the pure
//! decisions it makes live in `ghostty_voice_core::pty` and are unit-tested.

use std::ffi::CString;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::io::RawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, bail};
use ghostty_voice_core::link::{LinkState, Registration, classify_first_line};
use ghostty_voice_core::protocol::{Frame, PROTOCOL_VERSION, State};
use ghostty_voice_core::pty::{PreviewCursor, PtyError, injection_bytes, split_command};
use ghostty_voice_core::strip;
use ghostty_voice_core::trigger::{self, Segment};

const STDIN: RawFd = libc::STDIN_FILENO;
const STDOUT: RawFd = libc::STDOUT_FILENO;

/// Rows reserved at the bottom for the voice status strip.
const STRIP_HEIGHT: u16 = 1;

/// The debug injection trigger: F12 (`ESC [ 24 ~`, the common xterm encoding).
/// (The daemon's pushed Transcript is the real injection path; this is a manual
/// aid for exercising the channel without the daemon.)
const DEBUG_KEY: &[u8] = b"\x1b[24~";
/// The hardcoded debug string, to prove the injection channel end-to-end.
const DEBUG_STRING: &str = "create a function that reverses a string";

/// Set by the SIGWINCH handler; the poll loop drains it and re-sizes the child.
static RESIZED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigwinch(_sig: libc::c_int) {
    RESIZED.store(true, Ordering::SeqCst);
}

/// State shared between the daemon-socket client thread and the proxy loop.
///
/// The client thread only ever *writes* here; the proxy loop drains it. That
/// keeps a single writer to both the terminal (strip paints) and the child PTY
/// (keystrokes + injected Transcript), so escape sequences and bytes never
/// interleave across threads.
#[derive(Default)]
struct Shared {
    /// Latest token for the strip: the daemon's voice state (from `state` frames)
    /// — `idle`, `recording`, `transcribing`, `streaming` — while cleanly
    /// registered, or a [`LinkState`] token (`unreachable`/`rejected`/`dropped`)
    /// when the link to the daemon is down, so the failure modes stay distinct.
    state: String,
    /// Bytes waiting to be written into the child PTY: a `transcript` frame's
    /// injected Transcript (trailing newline already stripped) or a streaming
    /// `live-edit` frame's erase-and-type preview revision.
    pending_inject: Vec<u8>,
    /// Tracks the streaming live preview's stable/tail boundary so each `live-edit`
    /// frame is turned into the right erase-and-type byte stream. Reset at the
    /// start of each dictation (on entering the `streaming` state).
    cursor: PreviewCursor,
    /// True while a streaming dictation is active (the daemon is in the `streaming`
    /// state). The proxy loop suppresses the user's keystrokes while this holds so
    /// nothing but our injection mutates the composer. Cleared when the dictation
    /// finalizes/cancels (the daemon returns to a non-streaming state).
    streaming: bool,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (program, rest) = match split_command(&args) {
        Ok(split) => split,
        Err(PtyError::EmptyCommand) => {
            eprintln!("usage: talk-to <command> [args...]   (e.g. talk-to ssh host claude)");
            exit(2);
        }
    };

    // Reserve the bottom strip: the child is born sized to (H - STRIP_HEIGHT, W)
    // so it never addresses the row we paint the voice state on.
    let term = get_winsize(STDIN);
    let (child_ws, strip_row) = child_layout(&term);

    let (pid, master) = fork_pty(&child_ws, program, rest)?;

    install_sigwinch();
    // Raw mode on our stdin so keystrokes (including Ctrl-C as a 0x03 byte) flow
    // straight to the child's PTY rather than being interpreted by our terminal.
    let _raw = RawGuard::enter();

    // Register as a wrapper sink with the daemon and track its pushed state /
    // Transcripts. Connection failure is non-fatal — the wrapper still works as
    // a pure passthrough, the strip just reads `offline`.
    let shared = Arc::new(Mutex::new(Shared {
        state: "idle".to_owned(),
        pending_inject: Vec::new(),
        cursor: PreviewCursor::new(),
        streaming: false,
    }));
    spawn_sink_client(shared.clone());

    // Protect the strip from line-mode scrolling and paint the initial state.
    set_scroll_region(child_ws.ws_row);
    paint_strip(strip_row, "idle");

    let status = proxy_loop(master, pid, strip_row, shared);

    // Restore the full-screen scroll region and the terminal before exiting.
    reset_scroll_region();
    drop(_raw);
    exit(status);
}

/// Derive the child PTY winsize (full width, bottom strip reserved) and the
/// 1-based strip row from the real terminal size.
fn child_layout(term: &libc::winsize) -> (libc::winsize, u16) {
    let geo = strip::geometry(term.ws_row, term.ws_col, STRIP_HEIGHT);
    let mut ws = *term;
    ws.ws_row = geo.child.rows;
    ws.ws_col = geo.child.cols;
    (ws, geo.strip_row)
}

/// The forwarding loop: stdin → child PTY, child PTY → stdout, verbatim. Returns
/// the child's exit code once its PTY closes. The bottom strip is repainted after
/// child output and recomputed on resize so it stays visible and correct.
fn proxy_loop(
    master: RawFd,
    pid: libc::pid_t,
    mut strip_row: u16,
    shared: Arc<Mutex<Shared>>,
) -> i32 {
    // The last state token we painted, so we only repaint the strip on a change.
    let mut painted_state = "idle".to_owned();
    let mut fds = [
        libc::pollfd {
            fd: STDIN,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: master,
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    let mut buf = [0u8; 8192];

    loop {
        if RESIZED.swap(false, Ordering::SeqCst) {
            // A SIGWINCH arrived: re-read our size, recompute the reserved strip,
            // push the reduced winsize onto the child so it reflows, and repaint.
            let term = get_winsize(STDIN);
            let (child_ws, new_strip_row) = child_layout(&term);
            set_winsize(master, &child_ws);
            strip_row = new_strip_row;
            set_scroll_region(child_ws.ws_row);
            paint_strip(strip_row, &painted_state);
        }

        // Apply whatever the daemon-socket client produced: inject pending
        // Transcript bytes into the child (no trailing newline — already stripped
        // by `injection_bytes`), and repaint the strip if the voice state changed.
        let (inject, state_now, streaming) = {
            let mut sh = shared.lock().unwrap();
            (
                std::mem::take(&mut sh.pending_inject),
                sh.state.clone(),
                sh.streaming,
            )
        };
        if !inject.is_empty() {
            write_all_fd(master, &inject);
        }
        if state_now != painted_state {
            painted_state = state_now;
            paint_strip(strip_row, &painted_state);
        }

        let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, 200) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue; // SIGWINCH (or similar) — loop re-checks RESIZED
            }
            break;
        }

        // stdin → child PTY. Recognize the in-terminal triggers (Shift+F10/F9):
        // they are consumed and sent to the daemon, never forwarded to the child.
        // While a streaming dictation is active, the user's keystrokes are
        // suppressed (only the trigger keys still resolve) so a stray keystroke
        // can't desync the live edits — the strip shows the dictation is live.
        // Otherwise the F12 debug key is intercepted and everything else passes
        // verbatim.
        if fds[0].revents & libc::POLLIN != 0 {
            let r = unsafe { libc::read(STDIN, buf.as_mut_ptr().cast(), buf.len()) };
            if r > 0 {
                let data = &buf[..r as usize];
                if streaming {
                    // Suppressed: dispatch the trigger combos, drop everything else.
                    for t in trigger::scan_suppressed(data) {
                        send_command(t.command_word());
                    }
                } else {
                    for segment in trigger::scan(data) {
                        match segment {
                            // A trigger fires only because the user is in this
                            // window: send the command to the daemon.
                            Segment::Trigger(t) => send_command(t.command_word()),
                            // F12 debug aid: inject a hardcoded string (no trailing
                            // Enter); the debug key itself is consumed.
                            Segment::Forward(bytes) if bytes == DEBUG_KEY => {
                                write_all_fd(master, &injection_bytes(DEBUG_STRING));
                            }
                            Segment::Forward(bytes) => write_all_fd(master, bytes),
                        }
                    }
                }
            }
        }

        // child PTY → stdout (paint the child's TUI verbatim).
        if fds[1].revents & libc::POLLIN != 0 {
            let r = unsafe { libc::read(master, buf.as_mut_ptr().cast(), buf.len()) };
            if r <= 0 {
                break; // child closed its PTY (exited)
            }
            write_all_fd(STDOUT, &buf[..r as usize]);
            // Keep the strip visible after the child paints its region.
            paint_strip(strip_row, &painted_state);
        }
        if fds[1].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            break;
        }
    }

    reap(pid)
}

/// Wait for the child and return its exit code (128+signal if it was killed).
fn reap(pid: libc::pid_t) -> i32 {
    let mut status: libc::c_int = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };
    if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else if libc::WIFSIGNALED(status) {
        128 + libc::WTERMSIG(status)
    } else {
        1
    }
}

// ---- OS glue ---------------------------------------------------------------

/// `forkpty` + `execvp` the wrapped program. Returns `(child pid, master fd)`.
/// In the child this never returns (it execs or exits).
fn fork_pty(ws: &libc::winsize, program: &str, rest: &[String]) -> Result<(libc::pid_t, RawFd)> {
    let mut master: RawFd = -1;
    // SAFETY: standard forkpty contract. We pass our desired winsize so the
    // slave is created already sized; name/termios default.
    let pid = unsafe {
        libc::forkpty(
            &mut master,
            std::ptr::null_mut(),
            std::ptr::null(),
            ws as *const libc::winsize,
        )
    };
    if pid < 0 {
        bail!("forkpty failed: {}", std::io::Error::last_os_error());
    }
    if pid == 0 {
        // Child: forkpty has already made the slave our controlling terminal and
        // wired stdin/stdout/stderr to it. Exec the wrapped command.
        exec_child(program, rest);
        // exec_child only returns on failure.
        eprintln!("talk-to: failed to exec {program}");
        unsafe { libc::_exit(127) };
    }
    Ok((pid, master))
}

/// Replace this (child) process image with the wrapped command. argv[0] is the
/// program; the rest are its arguments verbatim.
fn exec_child(program: &str, rest: &[String]) {
    let Ok(prog_c) = CString::new(program) else {
        return;
    };
    let mut argv: Vec<CString> = Vec::with_capacity(rest.len() + 1);
    argv.push(prog_c.clone());
    for a in rest {
        match CString::new(a.as_str()) {
            Ok(c) => argv.push(c),
            Err(_) => return,
        }
    }
    let mut ptrs: Vec<*const libc::c_char> = argv.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    unsafe { libc::execvp(prog_c.as_ptr(), ptrs.as_ptr()) };
}

// ---- daemon-socket client (wrapper sink) -----------------------------------

/// Connect to the daemon, `register-sink`, and stream pushed frames into the
/// shared state on a background thread. `state` frames update the strip token;
/// `transcript` frames queue bytes for the proxy loop to inject into the child.
///
/// The wrapper sink is purely additive: the proxy keeps working as a passthrough
/// regardless. When the link is down the strip shows a distinct [`LinkState`]
/// token — `unreachable` (no daemon), `incompatible` (a too-old/version-mismatched
/// daemon, e.g. a stale daemon after an upgrade), `rejected` (daemon refused for
/// another reason), or `dropped` (was registered, then EOF) — never a generic
/// "offline", and the reason is logged for diagnosis. It
/// reconnects on a 2 s cadence, re-registering when the daemon returns.
///
/// Registration carries this client's [`PROTOCOL_VERSION`] (`register-sink <v>`)
/// so the daemon can detect — and the client can report — an incompatible daemon.
fn spawn_sink_client(shared: Arc<Mutex<Shared>>) {
    std::thread::spawn(move || {
        let Some(path) = daemon_socket_path() else {
            set_shared_state(&shared, LinkState::Unreachable.token());
            log_link("no daemon socket path (XDG_RUNTIME_DIR unset)");
            return;
        };
        let register = format!("register-sink {PROTOCOL_VERSION}\n");
        loop {
            match UnixStream::connect(&path) {
                Err(e) => {
                    set_shared_state(&shared, LinkState::Unreachable.token());
                    log_link(&format!("daemon unreachable at {} ({e})", path.display()));
                }
                Ok(mut stream) => match stream.write_all(register.as_bytes()) {
                    Ok(()) => serve_link(&shared, stream),
                    Err(e) => {
                        set_shared_state(&shared, LinkState::Dropped.token());
                        log_link(&format!("register-sink write failed ({e})"));
                    }
                },
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

/// Read the daemon's reply to `register-sink`, classify it, and — if we are
/// registered — stream pushed frames into `shared` until the connection drops.
/// Sets the strip token and logs the reason on every non-registered outcome.
fn serve_link(shared: &Arc<Mutex<Shared>>, stream: UnixStream) {
    let mut lines = BufReader::new(stream).lines();
    let first = match lines.next() {
        Some(Ok(line)) => line,
        Some(Err(e)) => {
            set_shared_state(shared, LinkState::Dropped.token());
            log_link(&format!("connection error after register-sink ({e})"));
            return;
        }
        None => {
            set_shared_state(shared, LinkState::Dropped.token());
            log_link("connection closed with no reply to register-sink");
            return;
        }
    };

    match classify_first_line(&first) {
        Registration::Incompatible => {
            set_shared_state(shared, LinkState::Incompatible.token());
            log_link(&format!(
                "incompatible daemon (restart/upgrade ghostty-voiced): {first:?}"
            ));
        }
        Registration::Rejected => {
            set_shared_state(shared, LinkState::Rejected.token());
            log_link(&format!("daemon rejected register-sink: {first:?}"));
        }
        Registration::Registered => {
            apply_frame(shared, &first);
            for line in lines {
                let Ok(line) = line else { break };
                apply_frame(shared, &line);
            }
            // The frame loop ended → EOF after a previously-good registration.
            set_shared_state(shared, LinkState::Dropped.token());
            log_link("registered connection dropped (daemon EOF)");
        }
    }
}

/// Apply one pushed [`Frame`]: a `state` frame repaints the strip token; a
/// `transcript` frame queues bytes for the proxy loop to inject into the child.
fn apply_frame(shared: &Arc<Mutex<Shared>>, line: &str) {
    match Frame::parse(line) {
        Ok(Frame::State(s)) => {
            let mut sh = shared.lock().unwrap();
            // A new dictation begins on entering the streaming state — start its
            // preview from an empty cursor so edit bytes stay in sync, and suppress
            // keystrokes for the duration. Leaving the state ends suppression.
            sh.streaming = s == State::Streaming;
            if s == State::Streaming {
                sh.cursor.reset();
            }
            sh.state = s.label();
        }
        Ok(Frame::Transcript(text)) => {
            shared
                .lock()
                .unwrap()
                .pending_inject
                .extend_from_slice(&injection_bytes(&text));
        }
        // The streaming reconcile: erase the whole rough preview and type the
        // batch-accurate Transcript in its place — no double-typing, no trailing
        // newline (review-before-Enter).
        Ok(Frame::Finalize(text)) => {
            let mut sh = shared.lock().unwrap();
            let bytes = sh.cursor.finalize(&text);
            sh.pending_inject.extend_from_slice(&bytes);
        }
        // A streaming live-edit revises the preview in place: erase the previous
        // tail, type the newly-committed text, then the new tail (no trailing
        // newline — review-before-Enter). Settled words never flicker.
        Ok(Frame::LiveEdit { committed, tail }) => {
            let mut sh = shared.lock().unwrap();
            let bytes = sh.cursor.apply_edit(&committed, &tail);
            sh.pending_inject.extend_from_slice(&bytes);
        }
        Err(_) => {} // ignore frames we don't understand
    }
}

fn set_shared_state(shared: &Arc<Mutex<Shared>>, token: &str) {
    shared.lock().unwrap().state = token.to_owned();
}

/// Append a link diagnostic line to the talk-to log file (best-effort). We log to
/// a file, not stderr: talk-to runs a raw-mode full-screen proxy, so writing to
/// the terminal mid-TUI would corrupt the child's rendering.
fn log_link(reason: &str) {
    let Some(path) = link_log_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "talk-to: {reason}");
    }
}

/// The talk-to log file: `$XDG_STATE_HOME/ghostty-voice/talk-to.log`, falling
/// back to `~/.local/state/ghostty-voice/talk-to.log`.
fn link_log_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_STATE_HOME") {
        return Some(PathBuf::from(dir).join("ghostty-voice/talk-to.log"));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".local/state/ghostty-voice/talk-to.log"))
}

/// The daemon's control socket (matching the daemon and `ghostty-voice-ctl`).
fn daemon_socket_path() -> Option<PathBuf> {
    ghostty_voice_core::config::socket_path()
}

/// Send a one-shot command word (e.g. `toggle`/`vad`) to the daemon's control
/// socket — the same one-command-then-reply path `ghostty-voice-ctl` uses, so the
/// daemon's command handling is reused unchanged. Runs on a detached thread so a
/// slow or absent daemon never stalls the proxy loop; the reply is ignored (the
/// status strip already reflects the resulting state over the push link). Failures
/// are logged to the talk-to log, not the terminal (which is mid-TUI).
fn send_command(word: &'static str) {
    let Some(path) = daemon_socket_path() else {
        log_link("cannot send trigger: no daemon socket path (XDG_RUNTIME_DIR unset)");
        return;
    };
    std::thread::spawn(move || match UnixStream::connect(&path) {
        Ok(mut stream) => {
            if let Err(e) = stream.write_all(format!("{word}\n").as_bytes()) {
                log_link(&format!("trigger '{word}' write failed ({e})"));
            }
        }
        Err(e) => log_link(&format!(
            "trigger '{word}' could not reach the daemon ({e})"
        )),
    });
}

/// Paint the status strip onto the real terminal at `strip_row`, showing
/// `state`. The renderer saves/restores the cursor, so the child is undisturbed.
fn paint_strip(strip_row: u16, state: &str) {
    write_all_fd(STDOUT, &strip::render(strip_row, state));
}

/// Confine scrolling to the child's rows (`1..=child_rows`) via DECSTBM, so a
/// line-mode child (e.g. a bare shell) can't scroll our strip away. Bracketed
/// with DECSC/DECRC because DECSTBM homes the cursor. A no-op when the child has
/// no usable rows. Alt-screen TUIs (the common case) are unaffected either way.
fn set_scroll_region(child_rows: u16) {
    if child_rows == 0 {
        return;
    }
    write_all_fd(STDOUT, format!("\x1b7\x1b[1;{child_rows}r\x1b8").as_bytes());
}

/// Restore the full-screen scroll region on exit.
fn reset_scroll_region() {
    write_all_fd(STDOUT, b"\x1b[r");
}

fn get_winsize(fd: RawFd) -> libc::winsize {
    // SAFETY: TIOCGWINSZ fills a winsize; zeroed is a valid fallback if it fails
    // (e.g. fd is not a tty), yielding a 0×0 size the child treats as unknown.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) };
    ws
}

fn set_winsize(fd: RawFd, ws: &libc::winsize) {
    unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, ws as *const libc::winsize) };
}

fn install_sigwinch() {
    // SAFETY: installing a trivial flag-setting handler for SIGWINCH.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_sigwinch as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = libc::SA_RESTART;
        libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
    }
}

/// Write all of `data` to `fd`, retrying short writes. Best-effort: a closed fd
/// just ends the write (the loop will notice the peer is gone).
fn write_all_fd(fd: RawFd, mut data: &[u8]) {
    while !data.is_empty() {
        let w = unsafe { libc::write(fd, data.as_ptr().cast(), data.len()) };
        if w <= 0 {
            break;
        }
        data = &data[w as usize..];
    }
}

/// RAII raw-mode guard for stdin. Restores the saved termios on drop so the
/// terminal is always left usable, even on a panic or early exit.
struct RawGuard {
    saved: libc::termios,
}

impl RawGuard {
    /// Enter raw mode if stdin is a tty; `None` (a no-op guard) otherwise.
    fn enter() -> Option<Self> {
        if unsafe { libc::isatty(STDIN) } != 1 {
            return None;
        }
        // SAFETY: tcgetattr fills a termios we then copy + cfmakeraw + set.
        let mut term: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(STDIN, &mut term) } != 0 {
            return None;
        }
        let saved = term;
        unsafe {
            libc::cfmakeraw(&mut term);
            libc::tcsetattr(STDIN, libc::TCSANOW, &term);
        }
        Some(Self { saved })
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        unsafe { libc::tcsetattr(STDIN, libc::TCSANOW, &self.saved) };
    }
}
