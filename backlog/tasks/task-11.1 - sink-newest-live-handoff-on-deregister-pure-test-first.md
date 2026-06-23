---
id: TASK-11.1
title: 'sink: newest-live handoff on deregister (pure, test-first)'
status: In Progress
assignee: []
created_date: '2026-06-22 23:27'
updated_date: '2026-06-23 04:04'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Chicago-style TDD on the pure `SinkRegistry` (crates/ghostty-voice-core/src/sink.rs), no doubles.

1. RED: add unit tests asserting the newest-live handoff:
   - deregister(active wrapper) with another live wrapper → active becomes the most-recently-registered still-live wrapper (3-wrapper case proves it picks the newest survivor, not the oldest).
   - chain of deregistrations peels back newest→oldest, focused-window only when none remain.
   - deregister(non-active) never changes active (already covered; extend to 3 wrappers).
   - a transcript bound to a now-dead wrapper still routes to Held even though a handoff made another wrapper active (proves handoff never redirects a bound utterance).
2. GREEN: change `live: HashSet<u64>` → `live: Vec<u64>` tracking registration order (push on register, retain on deregister). On deregister of the active wrapper, set active to Wrapper(last live) or FocusedWindow when empty.
3. Keep `register`, `active`, `is_live`, `route` semantics; add `wrapper_count()` for TASK-10.1.
4. cargo test/clippy/fmt green.
<!-- SECTION:PLAN:END -->
