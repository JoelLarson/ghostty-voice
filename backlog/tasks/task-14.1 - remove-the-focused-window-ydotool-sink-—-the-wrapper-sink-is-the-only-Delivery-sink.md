---
id: TASK-14.1
title: >-
  remove the focused-window/ydotool sink — the wrapper sink is the only Delivery
  sink
status: To Do
assignee: []
created_date: '2026-06-24 00:45'
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
