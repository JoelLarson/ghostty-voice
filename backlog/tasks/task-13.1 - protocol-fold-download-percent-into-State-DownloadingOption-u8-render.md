---
id: TASK-13.1
title: 'protocol: fold download percent into State::Downloading(Option<u8>) + render'
status: Done
assignee:
  - claude
created_date: '2026-06-23 05:57'
updated_date: '2026-06-23 06:07'
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 `State::Downloading(Option<u8>)` (State stays `Copy`); `encode_token`/`parse` round-trip `downloading` and `downloading 42`; a bare `downloading` still parses (backward compatible)
- [x] #2 #2 `State::label()` returns `downloading 42%` / `downloading` / the plain word for other states (unit-tested)
- [x] #3 #3 `Frame::State(Downloading(Some(42)))` and a `StatusReport` carrying a `downloading 42` state plus `sink=`/`wrappers=` round-trip
- [x] #4 #4 The talk-to strip renders a pushed `state downloading 42` as `downloading 42%` (integration test mirroring sink_registration.rs)
- [x] #5 #5 Test-first (Chicago-style), no doubles in the pure modules; cargo test/clippy/fmt green

## Blocked by
None - can start immediately
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Test-first in protocol.rs, then make compile across the workspace.

1. protocol.rs (red→green):
   - `State::Downloading(Option<u8>)` (Option<u8> keeps State Copy).
   - Replace `as_str()` with `encode_token(&self)->String`: `downloading` / `downloading 42` / single words.
   - `State::parse(&str)->Option<State>`: split_whitespace; `downloading`→None, `downloading N`→Some(N<=100), single words; reject extra tokens / N>100.
   - `State::label(&self)->String`: `downloading 42%` / `downloading` / plain word.
   - `Frame::encode`, `Response::encode`, `StatusReport::encode` use `encode_token()`.
   - `StatusReport::parse`: collect state tokens after `ok` up to first `sink=`/`wrappers=`, join, `State::parse`; then parse sink=/wrappers=. Bare `ok downloading` and `ok downloading 42` both round-trip.
   - Update/extend inline tests: token round-trips for `downloading`/`downloading 42`/bare-compat; label mapping incl `42%`; Frame::State(Downloading(Some(42))) round-trip; StatusReport carrying `downloading 42` + sink=/wrappers= round-trip.
2. machine.rs: arms `Downloading(_)`; `go(state,..)`/`reject(state,..)` preserve the carried percent. Update tests to `Downloading(None)`; add a test that status preserves `Downloading(Some(42))`.
3. talk-to apply_frame: render `&s.label()`.
4. daemon: `set_state(daemon, State::Downloading(None))`.
5. New integration test `ghostty-voiced/tests/download_progress.rs` (mirrors sink_registration.rs): fake daemon pushes `state downloading 42`; client Frame::parse → Downloading(Some(42)); label == `downloading 42%`; also bare `downloading` → label `downloading`.
6. cargo test/clippy/fmt green.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
protocol.rs: `State::Downloading(Option<u8>)` (Option<u8> keeps State `Copy`). Replaced `State::as_str()` with `encode_token()->String` (`downloading` / `downloading 42` / single words) at the three encode sites (`Frame::encode`, `Response::encode`, `StatusReport::encode`). `State::parse` now accepts the whole state substring via split_whitespace — reconstructs the optional percent, rejects >100 / non-numeric / trailing tokens, and a bare `downloading` parses to `Downloading(None)` (backward compatible). Added `State::label()->String` for the human strip string (`downloading 42%`). `StatusReport::parse` isolates the state substring (tokens after `ok` up to the first `sink=`/`wrappers=`) before reading the sink fields, so two-token `downloading 42` round-trips alongside `sink=`/`wrappers=`. `Frame::parse` needed no change — it already forwards the post-`state ` substring to `State::parse`.

machine.rs: the `Downloading` arms became `Downloading(_)` and now return `go(state, …)` / `reject(state, …)` so the carried percent is preserved across a status query. Behavior otherwise unchanged (commands rejected while downloading; status answered).

talk-to apply_frame renders `&s.label()`; daemon enters `set_state(State::Downloading(None))`. ghostty-voice-ctl unchanged (prints the raw reply).

Tests (test-first, Chicago-style, no doubles in pure modules): protocol unit tests for encode_token, the multi-token parse round-trip, bare-downloading backward-compat, malformed-percent rejection, label mapping, Frame::State(Downloading(Some(42))) round-trip, and StatusReport carrying `downloading 42`/`downloading` next to sink fields. machine unit test that status preserves `Downloading(Some(42))`. New integration test `ghostty-voiced/tests/download_progress.rs` (mirrors sink_registration.rs): a fake daemon pushes `state downloading`/`state downloading 42` over a real Unix socket; the client decodes with the real `Frame::parse` and renders via `State::label()` → `downloading` / `downloading 42%`.

cargo test --workspace, cargo clippy --workspace --all-targets, cargo fmt --check all green.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Folded the model-download percent into the daemon's observable State as `State::Downloading(Option<u8>)` — one source of truth that both the wrapper-sink `Frame::State` push (strip) and the `StatusReport` (`ghostty-voice-ctl status`) serialize automatically. Wire grammar is the additive, backward-compatible `downloading` / `downloading 42` on the existing dumb line protocol; `State::label()` renders the human `downloading 42%`. No new percentage producer yet (the daemon still only enters `Downloading(None)` — that is TASK-13.2), but a percent pushed over the socket now renders end-to-end, proven by the new pushed-frame integration test. All work test-first; workspace test/clippy/fmt green.
<!-- SECTION:FINAL_SUMMARY:END -->

<!-- AC:END -->

<!-- AC:END -->
