---
id: TASK-11.1
title: 'sink: newest-live handoff on deregister (pure, test-first)'
status: To Do
assignee: []
created_date: '2026-06-22 23:27'
labels:
  - talk-to
dependencies: []
references:
  - task-11
  - crates/ghostty-voice-core/src/sink.rs
parent_task_id: TASK-11
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-11 (multi-wrapper correctness PRD).

Change `SinkRegistry` so deregistering the **active** wrapper reactivates the **most-recently-registered still-live** wrapper sink, falling back to the focused-window sink only when none remain. The registry must track registration order + liveness to pick the newest survivor (today it tracks only a live set + a single active value). Pure module, test-first, no doubles. Trigger-time binding and dead-bound→Held are unchanged.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 deregister(active wrapper) reactivates the most-recently-registered still-live wrapper; focused-window only when none remain
- [ ] #2 deregister(non-active wrapper) never changes which sink is active
- [ ] #3 route() for a dead-bound wrapper still returns Held (unchanged)
- [ ] #4 Test-first unit tests cover multi-wrapper register/deregister ordering, the handoff, and the empty fallback; cargo test green
<!-- AC:END -->
