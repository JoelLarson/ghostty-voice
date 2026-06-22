---
id: TASK-9.4
title: 'talk-to slice 4: delivery routing to the active sink (hands-free happy path)'
status: In Progress
assignee:
  - claude
created_date: '2026-06-22 06:46'
updated_date: '2026-06-22 07:10'
labels:
  - needs-triage
  - talk-to
dependencies:
  - TASK-9.2
  - TASK-9.3
references:
  - task-9 (PRD)
  - 'IDEAS.md #4'
  - CONTEXT.md
  - crates/ghostty-voiced/src/main.rs (drain_queue)
parent_task_id: TASK-9
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (PRD: talk-to — PTY wrapper, Delivery sink v1).

## What to build

Wire the real voice path end to end so dictation lands in the wrapped agent with no intervention. Change the daemon's queue drain (`drain_queue`) to deliver each Transcript to the ACTIVE sink instead of hard-calling ydotool:
- **focused-window sink** = today's `ghostty_voice_io::inject::type_text` (unchanged, still gated by the Freshness window) — used when no wrapper is registered.
- **wrapper sink** = push the Transcript frame down the registered connection.

`talk-to` writes the received Transcript into the child PTY with NO trailing newline (review-before-Enter survives). The status strip now reflects REAL daemon state (idle/recording/transcribing) via the state-update frames from slice 3.

Chicago-style (classicist) TDD is required: a daemon-level integration test (real daemon, no mocks, mirroring ghostty-voiced/tests/ordered_drain.rs) asserts a registered wrapper sink receives the pushed Transcript end-to-end.

## Validation / success

Demoable hands-free: `talk-to ssh host claude` running → trigger a recording → spoken text appears in claude's input line over SSH with no Enter; strip tracks recording/transcribing live; with no wrapper running, focused-window Auto-type is exactly as today.

## Blocked by

task-9.2 (status strip) and task-9.3 (push-sink protocol + registry).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 With `talk-to ssh host claude` running, triggering a recording gets the spoken text into claude's input line over SSH, hands-free, with no trailing Enter.
- [ ] #2 The bottom status strip reflects recording/transcribing/idle in real time.
- [ ] #3 With no wrapper registered, focused-window Auto-type behaves exactly as today and existing tests still pass.
- [ ] #4 The Transcript is cached before delivery is attempted (write-before-deliver preserved).
- [ ] #5 Chicago-style TDD: a daemon-level integration test (real daemon, no mocks, mirroring ordered_drain.rs) proves a registered wrapper sink receives the pushed Transcript end-to-end; `cargo test` green.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Slice 4: delivery routing to the active sink (hands-free happy path)

Route each drained Transcript to the ACTIVE sink instead of hard-calling ydotool. Trigger-time binding is implemented here (it's the correct end state and keeps slice-5 honest) — utterance binds `sinks.active()` at enqueue.

### Daemon (ghostty-voiced)
- Add `Daemon.bindings: HashMap<u64, ActiveSink>` (seq → bound sink). Import `ActiveSink, Route`.
- At enqueue (`stop_and_enqueue` and continuous `end_continuous`): `bindings.insert(seq, sinks.active())`.
- `drain_queue`: keep `head_delivery(now, window)` for readiness + the focused-window Freshness decision, then route by the bound sink via `sinks.route(bound)`:
  - `Route::FocusedWindow` → today's `type_text` gated by the freshness `Delivery` (unchanged behavior when no wrapper registered).
  - `Route::Wrapper(id)` → look up `sink_conns[id]`, send `Frame::Transcript(text).encode()+"\n"` (NO freshness — exact PTY). If the sender is missing (race) → hold.
  - `Route::Held` → held-for-replay (notify). (Dead-bound-sink fully exercised in slice 5.)
  - Remove the binding on resolve; also clear bindings in the empty/err resolve paths of `spawn_transcription`.
- Cache-before-deliver preserved (write transcript to cache before any delivery), unchanged.
- talk-to already writes received `transcript` frames into the child PTY with no trailing newline (slice 3); the strip already reflects real `state` frames (slice 3 watch). So "strip tracks recording/transcribing live" is already wired — confirm.

### Integration test (ghostty-voiced/tests, real daemon-style composition)
`wrapper_delivery.rs` mirroring ordered_drain.rs: a real `DeliveryQueue` + real `SinkRegistry` + real socket wrapper peer; enqueue an utterance bound to a registered wrapper, set it ready, drain → assert the wrapper peer receives the exact pushed `transcript <text>` frame end-to-end (no trailing newline in the payload). Also assert: with NO wrapper (bound FocusedWindow), route is FocusedWindow (today's path), and existing tests still pass.
<!-- SECTION:PLAN:END -->
