---
id: TASK-9.5
title: 'talk-to slice 5: bound target + held-for-replay (crash safety)'
status: To Do
assignee: []
created_date: '2026-06-22 06:47'
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
