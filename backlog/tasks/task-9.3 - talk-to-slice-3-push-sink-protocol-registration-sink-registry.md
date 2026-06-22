---
id: TASK-9.3
title: 'talk-to slice 3: push-sink protocol + registration + sink registry'
status: Done
assignee:
  - claude
created_date: '2026-06-22 06:46'
updated_date: '2026-06-22 07:10'
labels:
  - needs-triage
  - talk-to
dependencies:
  - TASK-9.1
references:
  - task-9 (PRD)
  - 'IDEAS.md #4'
  - CONTEXT.md
  - crates/ghostty-voice-core/src/protocol.rs
parent_task_id: TASK-9
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (PRD: talk-to — PTY wrapper, Delivery sink v1).

## What to build

Make `talk-to` a registered push-sink the daemon can target. End to end:
- **Protocol** (`protocol.rs`): add a `register-sink` command and a daemon→client push frame for a Transcript, plus a state-update line. DECISION (resolved, revisit at review): keep the existing newline-delimited line protocol — transcripts are already newline-free, so no JSON is needed yet, consistent with protocol.rs's "no JSON until a field needs it." Use a one-line framing such as `transcript <text>` and `state <token>`.
- **Daemon connection handler**: add a PERSISTENT registered-sink connection path beside today's one-shot read-line/write-response. After `register-sink`, the connection stays open and the daemon writes frames to it; closing it (or the client dying) deregisters.
- **Sink registry / active-sink** (pure module): exactly one active sink; wrapper becomes active on register; focused-window sink becomes active on disconnect/death. Extends the spirit of `delivery::decide`.
- **`talk-to` client**: open a persistent connection and `register-sink` on launch; deregister on exit.

No real Transcript delivery yet (that is slice 4) — a test/echo frame is enough to prove the channel.

Chicago-style (classicist) TDD is required: protocol parse/encode and the sink-registry lifecycle are driven test-first with no doubles (same style as existing protocol.rs tests).

## Validation / success

Demoable: launch `talk-to` → daemon reports the wrapper sink active; exit → focused-window sink active; a frame round-trips over the socket.

## Blocked by

task-9.1 (provides the `talk-to` binary).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Launching `talk-to` registers a wrapper sink and the daemon reports it as the active sink.
- [x] #2 Exiting `talk-to` (or dropping the connection) reactivates the focused-window sink.
- [x] #3 At most one sink is active at any moment.
- [x] #4 A `register-sink` command and a Transcript push frame round-trip over the control socket.
- [x] #5 Chicago-style TDD: test-first unit tests (no doubles) for the protocol parse/encode and the sink-registry lifecycle/active-sink rules; `cargo test` green.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Slice 3: push-sink protocol + registration + sink registry

### Pure, test-first in ghostty-voice-core (no doubles)
1. `protocol.rs` extension:
   - `Command::RegisterSink` parsed from `register-sink` (case-insensitive, trimmed).
   - `Frame { Transcript(String), State(State) }` daemon→client push frames with `encode()`/`parse()`: lines `transcript <text>` (text = remainder of line, internal spaces preserved, newline-free) and `state <token>`. Newline-delimited, no JSON (per slice-3 decision). Round-trip tests.
2. `sink.rs` (new) — Sink registry / active-sink:
   - `SinkId(u64)`, `ActiveSink { FocusedWindow, Wrapper(SinkId) }`, `Route { FocusedWindow, Wrapper(SinkId), Held }`.
   - `SinkRegistry`: `new()` (active=FocusedWindow), `register() -> SinkId` (active=Wrapper(id), monotonic ids), `deregister(id)` (if active was id → FocusedWindow), `active()`, `is_live(id)`.
   - `route(bound: ActiveSink, &registry) -> Route`: FocusedWindow→FocusedWindow; Wrapper(id)→ live?Wrapper(id):Held.
   - Tests: register makes wrapper active; deregister reactivates focused-window; exactly-one-active; route live vs dead; trigger-time binding snapshot survives a later registration of a different sink.

### Daemon wiring (ghostty-voiced) — channel proven, real delivery is slice 4
- `Daemon`: add `sinks: SinkRegistry`, `sink_conns: HashMap<SinkId, mpsc::UnboundedSender<String>>` (pre-encoded frame lines), `state_tx: watch::Sender<State>`; route all state writes through one `set_state_field` setter that also `state_tx.send`s.
- `handle_conn`: if first line parses to `RegisterSink`, enter persistent path: register sink, insert sender, push initial `state` frame, then `select!` { rx.recv() → write line to socket ; socket read 0/err → break }. On break: deregister + remove sender (focused-window reactivates). Else: existing one-shot path.
- talk-to client: connect to the daemon socket (XDG_RUNTIME_DIR/ghostty-voice.sock), send `register-sink\n`, spawn a reader thread that parses `Frame`s — `Transcript` → write injection_bytes to master PTY (no newline); `State` → update strip state. Daemon-unreachable is non-fatal (passthrough still works; strip shows offline).

### Integration test (ghostty-voiced/tests, real socket + real protocol + real registry)
`sink_registration.rs`: client connects, `register-sink`; server registers (active=Wrapper) + pushes `Frame::Transcript` and `Frame::State`; client parses them back; assert active() is the wrapper while connected and a frame round-trips. Mirrors ordered_drain.rs (real collaborators, doubles only at the socket peer).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented: protocol `Command::RegisterSink` + `Frame{Transcript,State}` encode/parse + `State::parse` (12 protocol tests); pure `sink::{SinkId,ActiveSink,Route,SinkRegistry}` with register/deregister/active/is_live/route (10 tests). Daemon: `Daemon.sinks/sink_conns/state_tx`, single `set_state` chokepoint broadcasting via watch, `handle_conn` intercepts `register-sink` → persistent `serve_sink` (registers, pushes initial state, select over mpsc transcript frames / watch state changes / EOF detection, deregisters on disconnect). talk-to client thread registers, parses pushed frames → strip state + pending PTY injection (single-writer via Shared).

Verified against the REAL daemon binary (isolated XDG_RUNTIME_DIR): `register-sink` → pushed `state downloading`; log shows 'wrapper sink SinkId(0) registered — now the active Delivery sink' then 'deregistered — focused-window sink reactivated' on disconnect; one-shot `ctl status` still works alongside. Integration test sink_registration.rs (real socket + real Frame + real SinkRegistry, mirrors ordered_drain.rs) green. Full workspace cargo test green; clippy clean. All ACs evidenced.
<!-- SECTION:NOTES:END -->
