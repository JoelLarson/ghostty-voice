---
id: TASK-9.5
title: 'talk-to slice 5: bound target + held-for-replay (crash safety)'
status: In Progress
assignee:
  - claude
created_date: '2026-06-22 06:47'
updated_date: '2026-06-22 07:14'
labels:
  - needs-triage
  - talk-to
dependencies:
  - TASK-9.4
references:
  - task-9 (PRD)
  - 'IDEAS.md #4'
  - 'CONTEXT.md (Held-for-replay, Replay-last)'
parent_task_id: TASK-9
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (PRD: talk-to — PTY wrapper, Delivery sink v1).

## What to build

Make delivery safe when the wrapped agent dies. Bind an utterance's target sink at TRIGGER time (when recording starts), not at delivery time. If the bound wrapper sink is gone when the Transcript is ready, hold it for replay (it is already cached) — recoverable via `replay-last` — and NEVER silently redirect it into whatever window is focused now. The focused-window sink reactivates when the wrapper disconnects.

This is the guarantee the wrapper sink can honor exactly (durable PTY identity), distinct from the focused-window sink's best-effort Freshness-window behavior.

Chicago-style (classicist) TDD is required: trigger-time binding and dead-bound-sink → held are driven test-first with no doubles, plus integration coverage of the crash path.

## Validation / success

Demoable: kill `talk-to` mid-flight (after triggering, before delivery) → the Transcript is held and `replay-last` recovers it; nothing is typed into the newly focused window.

## Blocked by

task-9.4 (delivery routing to the active sink).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A Transcript is bound to the sink that was active when its recording started, not the sink active at delivery time.
- [ ] #2 Killing `talk-to` before delivery holds the Transcript (Held-for-replay) and `replay-last` recovers it.
- [ ] #3 Nothing is typed into the newly focused window when the bound wrapper sink has died.
- [ ] #4 The focused-window sink reactivates after the wrapper disconnects.
- [ ] #5 Chicago-style TDD: test-first unit tests (no doubles) for trigger-time binding and dead-bound-sink → held, plus integration coverage of the crash path; `cargo test` green.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Slice 5: bound target + held-for-replay (crash safety)

Trigger-time binding + dead-bound-sink→Held are ALREADY implemented in slices 3–4 (the correct end state): bindings captured at enqueue; `drain_queue` routes `Route::Held` → held-for-replay; `serve_sink` deregisters a dead wrapper so `is_live` is false at delivery; focused-window reactivates on disconnect. This slice adds the explicit TEST coverage the AC requires and verifies the crash path.

### Tests (Chicago TDD, test-first where logic is new)
1. Unit (sink.rs already has): trigger-time binding snapshot + dead-bound-sink→Held + never-redirected — present (10 tests). Confirm they cover AC #1/#3.
2. Integration `held_for_replay.rs` (real socket + real SinkRegistry + real DeliveryQueue, mirrors ordered_drain.rs):
   - Bind an utterance to a registered wrapper; the wrapper DIES (connection drops → deregister) BEFORE the transcript is ready; route(bound) → `Route::Held`; assert NOT delivered to the wrapper and NOT redirected to focused-window; the cached transcript is recoverable (assert `latest_transcript` returns it — write-before-deliver). After disconnect, registry.active() == FocusedWindow.
   - A second utterance bound to a now-dead wrapper while a DIFFERENT focus is active still Holds (never silent-redirect to current focus).

### Verify
Real-daemon smoke: register a wrapper sink, drop it, confirm the daemon logs deregistration + focused-window reactivation (already shown in slice 3). The full kill-mid-flight→replay-last demo needs GPU/mic and is demo-only; the crash routing + cache recovery is proven by the integration test.
<!-- SECTION:PLAN:END -->
