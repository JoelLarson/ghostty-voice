---
id: TASK-10.1
title: 'status: report the active Delivery sink + registered wrapper count'
status: Done
assignee: []
created_date: '2026-06-22 23:26'
updated_date: '2026-06-23 04:12'
labels:
  - talk-to
dependencies: []
references:
  - task-10
  - crates/ghostty-voice-core/src/protocol.rs
  - crates/ghostty-voiced/src/main.rs
  - crates/ghostty-voice-ctl/src/main.rs
parent_task_id: TASK-10
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-10 (operability PRD).

Extend the `status` response so `ghostty-voice-ctl status` reports which **Delivery sink** is active (focused-window vs wrapper) and how many wrapper sinks are registered. Today status returns only `ok <state>`; add the sink info **additively** (backward-compatible) by reading the daemon's `SinkRegistry`. This lets a user confirm routing without tailing journald.

Keep the deliberately-dumb newline line protocol (no JSON). Chicago-style TDD.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 status output includes the active sink kind (focused-window vs wrapper) and the registered wrapper-sink count
- [x] #2 Additive and backward-compatible with the existing `ok <state>` response
- [x] #3 Works correctly with and without a wrapper registered
- [x] #4 Test-first unit tests for protocol encode/parse + a daemon-level integration test (mirroring ordered_drain.rs) showing a registered wrapper reported active; cargo test green
- [x] #5 README documents the new status output
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Chicago-style TDD. Additive, backward-compatible extension of the `status` reply (keep the line protocol, no JSON).

1. protocol.rs (RED→GREEN): add `SinkKind {FocusedWindow, Wrapper}` (as_str/parse) and `StatusReport {state, active_sink, wrapper_count}` with encode → `ok <state> sink=<kind> wrappers=<n>` and a backward-compatible parse (`ok <state>` alone parses with sink defaulting to focused-window / 0; sink=/wrappers= tokens optional). Unit tests: encode round-trip, parse of the full line, backward-compat parse of a bare `ok idle`, SinkKind round-trip.
2. daemon (ghostty-voiced/src/main.rs): in handle_conn, special-case `status` to build a StatusReport from `d.state` + `d.sinks` (active kind via ActiveSink, count via wrapper_count()) and write its encoded line. (Status is always allowed and read-only, so bypassing the machine is equivalent to its no-op status arm.)
3. Integration test `status_report.rs` (mirroring ordered_drain.rs): real SinkRegistry + real StatusReport over a real socket — with a wrapper registered, status reports sink=wrapper wrappers=1; with none, sink=focused-window wrappers=0.
4. README: document the new status output.
5. cargo test/clippy/fmt green.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
`status` now reports the active Delivery sink + registered wrapper count, additively and backward-compatibly. Committed as 1044c6e.

- protocol.rs (Chicago-style TDD): `SinkKind {FocusedWindow, Wrapper}` (as_str/parse) and `StatusReport {state, active_sink, wrapper_count}` encoding `ok <state> sink=<kind> wrappers=<n>`. Parse requires a leading `ok <state>` and treats `sink=`/`wrappers=` as optional, so a bare `ok idle` from an older daemon still parses (defaults focused-window / 0). Unit tests cover encode, round-trip, backward-compat, and rejection of err/unknown-state lines.
- ghostty-voiced: `handle_conn` special-cases `status`, building the `StatusReport` from the live `SinkRegistry` (active kind via `ActiveSink`, count via `wrapper_count()`). Status is read-only and always allowed, so bypassing the state machine matches its no-op status arm.
- Integration test `status_report.rs` (real `SinkRegistry` + real `StatusReport` over a real Unix socket, mirroring ordered_drain.rs): a registered wrapper reports `sink=wrapper wrappers=N`; none reports `sink=focused-window wrappers=0`.
- README documents the new output.

AC #1–#5 met. `cargo test --workspace` (261), clippy, fmt green.
<!-- SECTION:FINAL_SUMMARY:END -->
