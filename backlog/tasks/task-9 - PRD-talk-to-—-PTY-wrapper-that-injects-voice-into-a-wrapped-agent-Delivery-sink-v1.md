---
id: TASK-9
title: >-
  PRD: talk-to — PTY wrapper that injects voice into a wrapped agent (Delivery
  sink, v1)
status: Done
assignee:
  - claude
created_date: '2026-06-22 06:36'
updated_date: '2026-06-22 07:16'
labels:
  - needs-triage
  - prd
dependencies: []
references:
  - 'IDEAS.md #4'
  - >-
    CONTEXT.md (Delivery sink, Auto-type, Freshness window, Held-for-replay,
    Replay-last)
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Today I dictate by pressing a tactile trigger and the daemon types the **Transcript** into whatever window is *focused* via `ydotool` (the **Auto-type** path). That is agent-agnostic and works over SSH, but it has no idea *which* program receives the text — delivery targets "the focused window," not a specific agent. When I am driving a single coding agent (claude-code, often over SSH), I want my speech to go into *that agent*, deterministically, hands-free, without babysitting window focus.

## Solution

A foreground launcher, `talk-to <command>` (e.g. `talk-to ssh host claude`), that spawns the agent on a pseudo-terminal, passes its TUI through to my Ghostty window unchanged, and registers itself with the daemon as a **wrapper sink**. When I trigger a recording (existing evdev hotkey or `ghostty-voice-ctl`), the daemon records + transcribes on the desktop GPU as today, then **pushes** the finished **Transcript** to the wrapper sink, which writes it into the agent's PTY exactly as if typed — no trailing newline, so I still review and press Enter. A reserved bottom status row shows live voice state. It works over SSH with no extra machinery: injected bytes ride the existing ssh stdin pipe to the remote agent.

This generalizes delivery into a single **active Delivery sink**: `ydotool`-into-focused-window becomes the default *focused-window sink*; `talk-to` is a *wrapper sink*. Exactly one sink is active at a time; an utterance binds its target sink when triggered and is never silently redirected.

## User Stories

1. As a voice-dictation user, I want to run `talk-to claude` and have claude render normally in my terminal, so that wrapping the agent costs me nothing visually.
2. As a user driving a remote agent, I want `talk-to ssh host claude` to work the same as local, so that SSH is not a special case.
3. As a hands-free user, I want my trigger to record + transcribe and have the text land in the wrapped agent with no intervention, so that the happy path is truly hands-free.
4. As a careful user, I want injected text to arrive without a trailing Enter, so that I review before submitting.
5. As a user, I want a bottom status row showing idle/recording/transcribing, so that I can tell at a glance what the daemon is doing.
6. As a user, I want the wrapped agent to use the full screen except the reserved status row, so that nothing is clipped or corrupted.
7. As a user resizing my terminal, I want the wrapped agent to reflow correctly, so that resize never breaks rendering.
8. As a user, I want Ctrl-C and signals to reach the wrapped agent, not the wrapper, so that the agent behaves as if launched directly.
9. As a user, I want launching `talk-to` to make its wrapper sink active automatically, so that I do not run a separate activation step.
10. As a user, I want exiting `talk-to` to restore the focused-window sink, so that normal `ydotool` dictation resumes unchanged.
11. As a user with no wrapper running, I want today's focused-window Auto-type behavior unchanged, so that this feature is purely additive.
12. As a user, I want the daemon to deliver to exactly one active sink, so that text never goes two places.
13. As a user, I want a transcript bound to the sink active when I started speaking, so that switching contexts mid-utterance does not misroute it.
14. As a user, if my `talk-to` dies before delivery, I want the transcript held (cached) and recoverable via `replay-last`, never dumped into my current focus, so that a crash never types into the wrong place.
15. As a user, I want the transcript cached before delivery is attempted, so that no speech is ever lost to a failed write.
16. As a developer, I want the wrapper to inject into the agent's stdin pipe (not synthetic keystrokes), so that delivery is deterministic and needs no /dev/uinput.

## Implementation Decisions

Modules (boundaries confirmed with the developer):

New deep modules (pure, unit-tested in isolation):
- **Strip geometry** — `(rows, cols, strip_height) -> (child winsize, strip region)`. Encodes the bottom-strip invariant: child winsize is `(H-1, W)`, origin unchanged, so the child's bytes forward verbatim with no VT emulator. A top strip or side pane is explicitly rejected because it would require emulating + compositing the child.
- **Status-strip renderer** — `(State + detail) -> ANSI bytes` that paint the reserved row and restore the cursor without touching the child region. (Pure; visual-checked, not unit-tested in v1.)
- **Sink registry / active-sink** — encapsulates "exactly one active sink," lifecycle (wrapper registers -> active; deregisters or dies -> focused-window), target-binding-at-trigger, and dead-bound-sink -> Held-for-replay. Extends today's `delivery::decide`.
- **Push-sink protocol** — extends the control-socket vocabulary: parse `register-sink`, encode the pushed-Transcript frame. This is the first structured (non one-line request/response) message on the socket; decide framing (e.g. JSON) here.

Modified / thin boundary adapters (integration- or manually-tested):
- **Daemon connection handler** — add a persistent registered-sink connection path beside today's one-shot read-line/write-response exchange. After `register-sink` the connection stays open and the daemon writes Transcript frames to it; closing it (or process death) deregisters and reactivates the focused-window sink.
- **Daemon delivery (queue drain)** — deliver to the *active sink* instead of hard-calling `ydotool` type. Focused-window sink = today's `type_text` (unchanged, still gated by the Freshness window). Wrapper sink = write a Transcript frame to the registered connection; the Freshness window does not apply (the PTY target is exact).
- **PTY proxy plumbing** — forkpty, raw-mode passthrough, SIGWINCH -> recompute child winsize to `(H-1, W)`, signal forwarding. OS glue; not unit-tested.
- **`talk-to` binary** — new workspace crate wiring proxy + socket client + strip.

Contracts:
- Delivery binds the target sink at *trigger time*, not delivery time.
- Dead bound sink -> Held-for-replay; never silent redirect to the current focus.
- One active sink at all times; focused-window sink is the default/floor.

## Testing Decisions

**Chicago-style (classicist) TDD is required.** Every module is driven test-first in red-green-refactor; tests assert observable state/behavior through *real* collaborators rather than mocked interactions. Test doubles appear only at true external boundaries (whisper-server, `ydotool`, the OS PTY, the socket peer) — the deep pure modules use none. Good tests assert external behavior, not implementation details. Prior art: inline `#[cfg(test)]` unit tests in `ghostty-voice-core` (`protocol.rs`, `delivery.rs`, `inject.rs`) and daemon integration tests in `ghostty-voiced/tests/` (`ordered_drain.rs`, `accuracy_pipeline.rs`).

Test these modules (confirmed with developer):
- **Strip geometry** (unit) — winsize/region math across terminal sizes and edge cases (tiny terminals, 1-row reserve); asserts the `(H-1, W)`/origin invariant.
- **Sink registry / active-sink** (unit) — one-active-at-a-time, register/deregister lifecycle, target-binding-at-trigger, dead-bound-sink -> held.
- **Push-sink protocol** (unit) — parse `register-sink`, round-trip encode/decode the pushed-Transcript frame; same style as existing `protocol.rs` tests.
- **Delivery routing** (integration, daemon-level) — a registered wrapper sink receives the pushed Transcript end-to-end; mirrors `ordered_drain.rs`.

Status-strip renderer is verified visually in v1 (pure presentation), not unit-tested.

## Success Validation

The PRD is successful when the acceptance criteria pass — concretely: `talk-to ssh host claude` is launched, claude renders indistinguishably from a direct launch (including resize + Ctrl-C), the bottom strip tracks daemon state live, and pressing the trigger gets spoken text into claude's input line over SSH hands-free with no trailing Enter; with no wrapper running, today's focused-window Auto-type is unchanged; a `talk-to` killed mid-delivery holds the transcript (replay-last recoverable) and never types into the new focus; and all four test suites (three unit, one integration) pass, written test-first per the Chicago-style requirement above.

## Out of Scope

Deferred — seam kept open, NOT rejected:
- **Compositor introspection** to give the focused-window sink the same strong bound-target/hold-and-ask guarantee the wrapper sink has natively. The focused-window sink stays best-effort via the Freshness window in v1. This is a deliberate deferral, not a rejection of the DE-agnostic question.
- **Explicit `ghostty-voice-ctl sink <target>`** mid-session switching (lifecycle-implicit switching is enough for v1).
- **Transcript-history surface** — list cached transcripts newest->oldest and re-route any to any sink; the generalization of `replay-last` and the "ask me where to send it" UI.
- **Read-back / dialogue capture** — the wrapper writes *in* only; it does not parse the child's output stream (that would require a VT emulator).

## Further Notes

SSH works because `talk-to` wraps whatever command it is given; `ssh host claude` is just a command, and injected bytes flow PTY master -> ssh stdin -> remote PTY -> agent stdin over the existing pipe. The strong "deliver to the original target; if it died, hold and ask" guarantee is exact for the wrapper sink (durable PTY identity) and structurally unavailable to the focused-window sink (no compositor introspection). Thinnest tracer-bullet slice for implementation: a bare PTY proxy that forwards verbatim and injects a hardcoded string on a keypress — proves PTY + transparent passthrough + SSH with zero daemon coupling — before any sink wiring. Full design lives in IDEAS.md #4; domain language in CONTEXT.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 End-to-end happy path: with `talk-to ssh host claude` running, pressing the trigger records, transcribes on the desktop GPU, and the Transcript appears in claude's input line over SSH with no trailing Enter and no manual intervention.
- [ ] #2 Passthrough fidelity: claude's TUI under `talk-to` is visually indistinguishable from launching claude directly, including a terminal resize (child reflows to H-1 rows) and Ctrl-C reaching claude rather than the wrapper.
- [ ] #3 Status strip: the reserved bottom row reflects daemon state transitions (idle/recording/transcribing) in real time without disturbing claude's rendered region.
- [x] #4 Additivity: with no `talk-to` running, dictation behaves exactly as today (focused-window Auto-type via ydotool) and existing tests still pass.
- [x] #5 Crash safety: killing `talk-to` before delivery holds the Transcript (recoverable via replay-last) and never types into the newly focused window.
- [x] #6 Chicago-style TDD evidence: strip geometry, sink registry, and push-sink protocol have passing unit tests written test-first with no test doubles; delivery routing has a daemon-level integration test mirroring ordered_drain.rs.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Overall architecture (confirmed against the codebase)

Pure decision logic lives in `ghostty-voice-core` (Chicago-TDD with real objects); `ghostty-voice-io` holds boundary adapters; binaries are thin shells. Newline-delimited line protocol in `protocol.rs`. Delivery flows through `DeliveryQueue` + `delivery::decide` + the daemon's `drain_queue`.

### New pure modules in ghostty-voice-core (TDD, no doubles)
- `strip.rs` — **Strip geometry**: `(rows, cols, strip_height) -> child winsize (H-strip, W) + strip region`, origin unchanged. The bottom-strip invariant. + **Status-strip renderer** `render(state) -> ANSI bytes` painting the reserved row & restoring cursor (renderer visual-checked per PRD, light smoke test only).
- `sink.rs` — **Sink registry / active-sink**: `ActiveSink {FocusedWindow, Wrapper(id)}`, register→active, deregister/death→focused-window, exactly-one-active, trigger-time binding snapshot, and `route(bound, registry) -> {FocusedWindow, Wrapper(id), Held}` (dead bound wrapper → Held; focused-window still gated by Freshness window in the daemon).
- `protocol.rs` extension — `Command::RegisterSink` (`register-sink`); daemon→client `Frame {Transcript(text), State(state)}` with `encode`/`parse` (`transcript <text>`, `state <token>`), newline-delimited, no JSON (per slice-3 decision).

### New crate `crates/talk-to` (binary `talk-to`, OS glue, not unit-tested)
PTY proxy via libc: forkpty with child winsize `(H-1, W)`, raw-mode stdin (RAII restore), verbatim passthrough, SIGWINCH→TIOCSWINSZ recompute, repaint strip after child output, Ctrl-C reaches child (byte passthrough in raw mode). Socket-client thread: connect daemon socket, `register-sink`, read frames; `transcript` → write to master PTY (NO trailing newline), `state` → repaint strip. libc-only deps.

### Daemon (ghostty-voiced) changes
- `handle_conn`: detect `register-sink` first line → persistent registered-sink path (mpsc per sink; push frames; on disconnect deregister → focused-window reactivates). State pushed via a `tokio::sync::watch<State>` the daemon updates through one setter.
- `Daemon`: add `sinks: SinkRegistry`, `sink_conns: HashMap<SinkId, mpsc::Sender>`, `bindings: HashMap<seq, ActiveSink>`, `state_tx: watch::Sender<State>`.
- Trigger-time binding: capture `sinks.active()` at enqueue (`stop_and_enqueue`, continuous `end_continuous`).
- `drain_queue`: route head by bound sink — FocusedWindow → today's `type_text` (freshness applies); Wrapper(live) → push `Frame::Transcript` (no freshness); dead bound wrapper → Held-for-replay.

### Tests
- Unit (core, test-first, no doubles): strip geometry across sizes/edge cases; sink registry lifecycle + route; protocol parse/encode round-trip.
- Integration (ghostty-voiced/tests, real socket + real protocol + real registry, mirroring ordered_drain.rs): wrapper registers → receives pushed Transcript end-to-end (slice 4); wrapper drops → deregister → route→Held (slice 5).

### Sequencing
9.1 bare PTY proxy + debug injection → 9.2 strip geometry+renderer → 9.3 protocol+registration+registry → 9.4 delivery routing to active sink → 9.5 trigger-time binding + held-for-replay.

AFK execution: slices done sequentially, each fully green before the next; plans recorded per subtask. Demo/manual ACs needing GPU/mic/SSH cannot run in this environment — verified via `cargo test`/`cargo build` + faithful wiring; this limitation is reported honestly at finalization.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## talk-to — PTY voice-wrapper (Delivery sink) v1: implemented end to end

All five slices implemented test-first (Chicago-style), `cargo test` green (248 tests), clippy clean, `cargo fmt` clean.

### What shipped
**New pure modules in `ghostty-voice-core` (unit-tested, no doubles):**
- `pty.rs` — argv `split_command` (SSH is just a command) + `injection_bytes` (the review-before-Enter invariant: never a trailing newline). 5 tests.
- `strip.rs` — **Strip geometry** `geometry(rows,cols,strip_height) → child (H-strip, W) + strip_row`, the bottom-strip invariant (origin/width unchanged ⇒ verbatim passthrough, no VT emulator), saturating on degenerate sizes; + the **status-strip renderer** (DECSC → address row → clear → `● state` → DECRC). 8 tests.
- `sink.rs` — **Sink registry / active Delivery sink**: `ActiveSink{FocusedWindow,Wrapper}`, register→active, deregister/death→focused-window, exactly-one-active, trigger-time binding, and `route(bound) → {FocusedWindow, Wrapper, Held}`. 10 tests.
- `protocol.rs` — `Command::RegisterSink` + push `Frame{Transcript,State}` encode/parse + `State::parse`; newline-delimited, no JSON (slice-3 decision). 12 tests.

**New crate `crates/talk-to` (binary, libc OS glue):** forkpty + execvp, raw-mode stdin (RAII restore), `poll` verbatim passthrough, SIGWINCH→winsize recompute to (H-1,W), DECSTBM strip protection, bottom strip painted from live daemon state; a socket-client thread registers as a wrapper sink, injects pushed Transcripts into the child PTY with no trailing newline, and reflects pushed state. Connection failure is non-fatal (passthrough still works; strip reads `offline`).

**Daemon (`ghostty-voiced`):** persistent `register-sink` connection path (`serve_sink`) beside the one-shot path; `SinkRegistry` + per-sink push channels + a `watch<State>` broadcast through one `set_state` chokepoint; trigger-time binding (`bindings: seq→ActiveSink`) captured at enqueue; `drain_queue` routes the ready head by its bound sink — focused-window = today's `ydotool` Auto-type gated by the Freshness window (unchanged), wrapper = push `Frame::Transcript` to its PTY (no Freshness, exact target), dead bound wrapper = Held-for-replay (never redirected). Cache-before-deliver preserved.

**Integration tests (`ghostty-voiced/tests/`, real socket + real protocol + real registry/queue/cache, mirroring `ordered_drain.rs`):** `sink_registration.rs` (register→active→pushed frames round-trip→disconnect→focused-window), `wrapper_delivery.rs` (bound wrapper receives the pushed Transcript end-to-end, no trailing newline; no-wrapper→focused-window), `held_for_replay.rs` (wrapper crashes→Held, not redirected; cached transcript recoverable).

### Verified
- `cargo test --workspace` (248), `cargo clippy --workspace --all-targets` (clean), `cargo fmt --check` (clean).
- Headless proxy smoke: `talk-to` forwards child output verbatim, passes multi-arg commands, propagates exit codes.
- **Real daemon binary** (isolated XDG_RUNTIME_DIR): `register-sink` → pushed `state` frame; log shows the wrapper sink becoming the active Delivery sink and the focused-window sink reactivating on disconnect; one-shot `status` still works alongside.

### Acceptance criteria
- ✅ #4 Additivity (no wrapper → focused-window Auto-type unchanged; all prior tests pass), #5 Crash safety (held_for_replay), #6 Chicago-TDD evidence (3 unit suites + integration mirroring ordered_drain.rs) — all evidenced.
- ⏳ #1 hands-free spoken text into claude over SSH, #2 passthrough fidelity (live resize + Ctrl-C), #3 strip tracking state live — **fully implemented and wired**, but these require a real terminal + GPU/mic + an SSH host, which are unavailable in this headless environment. They are left for developer demo verification. Recommended checks: `talk-to bash` (resize, Ctrl-C, strip), `talk-to claude`, `talk-to ssh host claude` with the daemon running and a recording trigger.

### Notes
- Trigger-time binding was implemented from slice 4 (the correct end state), so slice 5 was test/verify-only.
- Domain language (Delivery sink, focused-window/wrapper sink, Auto-type, Held-for-replay, Replay-last, Freshness window) is used throughout code, tests, and docstrings.
- Not committed — left as working-tree changes for review (no commit was requested).
<!-- SECTION:FINAL_SUMMARY:END -->
