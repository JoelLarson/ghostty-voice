---
id: TASK-13.1
title: 'protocol: fold download percent into State::Downloading(Option<u8>) + render'
status: To Do
assignee: []
created_date: '2026-06-23 05:57'
labels:
  - needs-triage
  - talk-to
dependencies: []
references:
  - task-13
  - crates/ghostty-voice-core/src/protocol.rs
  - crates/ghostty-voice-core/src/machine.rs
  - crates/talk-to/src/main.rs
  - crates/ghostty-voiced/tests/sink_registration.rs
parent_task_id: TASK-13
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-13 (PRD: report model download progress on the talk-to strip + status).

## What to build
Make the daemon's `Downloading` **State** carry an optional percent and render it on every surface that already serializes `State` — one source of truth. `State::Downloading` becomes `State::Downloading(Option<u8>)` (`None` = underway, percent unknown; `Some(p)` = p%). Wire token on the deliberately-dumb newline line protocol: `downloading` or `downloading 42`; all other state words unchanged. Add `State::label()` for the human strip string (`"downloading 42%"`, `"downloading"`, `"idle"`, …). `Frame::State` and `StatusReport` carry/parse the new token (`StatusReport::parse` isolates the state substring — tokens after `ok` up to the first `sink=`/`wrappers=`). Update the `machine.rs` `Downloading` arms to `Downloading(_)` and the daemon/talk-to call sites so the workspace compiles, with the talk-to strip rendering via `label()`. No new percentage producer yet (the daemon still only enters `Downloading(None)`), but a percent pushed over the socket renders end-to-end.

## Acceptance criteria
- [ ] `State::Downloading(Option<u8>)` (State stays `Copy`); `encode_token`/`parse` round-trip `downloading` and `downloading 42`; a bare `downloading` still parses (backward compatible)
- [ ] `State::label()` returns `downloading 42%` / `downloading` / the plain word for other states (unit-tested)
- [ ] `Frame::State(Downloading(Some(42)))` and a `StatusReport` carrying a `downloading 42` state plus `sink=`/`wrappers=` round-trip
- [ ] The talk-to strip renders a pushed `state downloading 42` as `downloading 42%` (integration test mirroring sink_registration.rs)
- [ ] Test-first (Chicago-style), no doubles in the pure modules; cargo test/clippy/fmt green

## Blocked by
None - can start immediately
<!-- SECTION:DESCRIPTION:END -->
