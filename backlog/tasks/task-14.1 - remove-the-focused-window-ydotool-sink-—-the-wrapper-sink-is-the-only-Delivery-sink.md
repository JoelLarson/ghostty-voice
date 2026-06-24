---
id: TASK-14.1
title: >-
  remove the focused-window/ydotool sink — the wrapper sink is the only Delivery
  sink
status: In Progress
assignee: []
created_date: '2026-06-24 00:45'
updated_date: '2026-06-24 00:53'
labels:
  - needs-triage
dependencies: []
references:
  - CONTEXT.md
  - crates/ghostty-voice-core/src/sink.rs
  - crates/ghostty-voice-core/src/delivery.rs
  - crates/ghostty-voice-io/src/inject.rs
parent_task_id: TASK-14
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Part of TASK-14 (talk-to as the sole interface). Removes the **focused-window Delivery sink** so a `talk-to` **wrapper sink** is the only place a Transcript can go.

## What changes

- Delete `ydotool` injection: `ghostty-voice-io/src/inject.rs`, the `[inject]` config (`InjectConfig`), and the daemon's `health_check_ydotoold`.
- Collapse the sink model to wrapper-only: remove the `FocusedWindow` variants from `ActiveSink`, `Route`, and `SinkKind`; the registry's floor when no wrapper is registered becomes "no active sink" rather than focused-window.
- Remove the **Freshness window** concept (it only ever gated the focused-window sink): the per-utterance freshness deadline and the `Delivery::AutoType`/`HoldForReplay` staleness gate. A wrapper delivery either pushes to the live PTY or is **Held-for-replay** because the bound wrapper is gone — no time-based staleness.
- `replay-last` re-routes the latest cached transcript to the **active wrapper sink** (error if none registered), never to a focused window.
- The walking-skeleton `ghostty-voice` binary's `ydotool` typing is removed/retired (it exists only to prove ydotool injection end-to-end).
- Drop `ydotool`/`ydotoold` from packaging depends, doctor checks, and config example.
- Update CONTEXT.md (Delivery sink / Freshness window / Held-for-replay / Auto-type entries) to the wrapper-only model, and add the ADR recording the whole TASK-14 architecture shift (sole interface, no global input, no focused-window typing).

## Testing (classicist TDD, no mocks)

- `sink.rs` unit tests rewritten for the wrapper-only registry (register/deregister/newest-live handoff; no focused-window floor; a bound-but-dead wrapper routes to Held).
- Daemon integration tests updated: `status` no longer reports `sink=focused-window`; a delivery with no live wrapper holds; the wrapper-delivery and held-for-replay flows still pass.
- Full workspace `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check` green.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ydotool injection removed: io/inject.rs, [inject] config, and the daemon ydotoold health check are deleted; the workspace no longer references ydotool
- [ ] #2 Sink model is wrapper-only: FocusedWindow removed from ActiveSink/Route/SinkKind; with no wrapper registered there is no active sink (deliveries hold)
- [ ] #3 The Freshness window is removed; a wrapper delivery either pushes to the live PTY or is Held-for-replay because the bound wrapper is gone — no time-based staleness gate
- [ ] #4 replay-last re-routes the latest cached transcript to the active wrapper sink, and errors clearly when none is registered
- [ ] #5 Packaging/doctor/config.example no longer mention ydotool/ydotoold; the walking-skeleton ydotool path is retired
- [ ] #6 CONTEXT.md updated to the wrapper-only sink model (sink / freshness / held-for-replay / auto-type entries); an ADR records the TASK-14 architecture shift
- [ ] #7 sink.rs unit tests and daemon integration tests updated for wrapper-only delivery; full cargo test, clippy --all-targets, fmt --check are green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Core-first TDD; classicist (real objects, no mocks).

CORE (ghostty-voice-core):
- sink.rs: rewrite wrapper-only. SinkRegistry{ live:Vec<u64>, active:Option<u64>, next_id }. active()->Option<SinkId>. Route{ Wrapper(SinkId), Held } (drop FocusedWindow). route(bound:Option<SinkId>): Some(live)->Wrapper, else Held. Rewrite tests (no focused-window floor; newest-live handoff; dead/None bound -> Held).
- delete delivery.rs + lib.rs `pub mod delivery;`.
- queue.rs: drop freshness. Item loses record_end; enqueue() only; replace head_delivery/next_to_type with next_to_type()->Option<(u64,&str)> (head if ready). Drop freshness tests.
- delete inject.rs (type_command) + lib.rs `pub mod inject;`.
- protocol.rs: drop SinkKind + StatusReport.active_sink. StatusReport{ state, wrapper_count } -> wire `ok <state> wrappers=<n>`; keep backward-compat parse. Update tests.
- config.rs: remove `pub inject`, InjectConfig struct+Default+doc; retry_window_seconds stays (transcription-retry only) — update its doc (drop freshness). Update config tests (remove [inject]/key_delay assertions).
- doctor.rs: remove ydotool_socket_exists probe + "ydotoold socket" check + its tests/module-doc line. (uinput/input-group/trigger-device stay for 14.2.)

IO (ghostty-voice-io):
- delete inject.rs + lib.rs `pub mod inject;`; update lib.rs module doc (drop ydotool).

DAEMON (ghostty-voiced):
- remove `use delivery::Delivery`, drop ActiveSink+SinkKind from imports.
- Daemon struct: bindings: HashMap<u64,Option<SinkId>>; remove clock_base + now_offset.
- remove health_check_ydotoold() call + fn.
- stop_and_enqueue/end_continuous: enqueue() (no record_end); bound = sinks.active() (Option<SinkId>).
- drain_queue: next_to_type(); route Wrapper/Held only; no freshness/AutoType/ydotool arm.
- replay_last: push latest cached transcript to the active wrapper sink (err if none/closed).
- status_report: StatusReport{ state, wrapper_count }.

BINARIES/PACKAGING:
- ghostty-voice/src/main.rs: drop inject import + type_text call (skeleton prints transcript only); update doc.
- ctl/main.rs: doctor() drop ydotool_socket probe + YDOTOOL_SOCKET lookup; Doctor doc; ReplayLast doc (delivers into active talk-to).
- PKGBUILD: drop 'ydotool' depend + update the deps comment/pkgdesc.
- config.toml.example: remove [inject]; update [cache] retry_window doc.

TESTS (ghostty-voiced/tests): rewrite wrapper_delivery, held_for_replay, status_report, sink_registration, wrapper_handoff, ordered_drain, download_progress to wrapper-only API (enqueue/next_to_type; active()->Option; no FocusedWindow; StatusReport without active_sink). The "no wrapper -> focused-window" assertions become "no wrapper -> Held".

DOCS: CONTEXT.md (Delivery sink/Freshness window/Held-for-replay/Auto-type/Replay-last entries) -> wrapper-only; docs/adr/0003 records the TASK-14 shift.

VERIFY: cargo test (workspace) + clippy --all-targets + fmt --check green.
<!-- SECTION:PLAN:END -->
