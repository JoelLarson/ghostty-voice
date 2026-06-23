---
id: TASK-11.2
title: 'daemon: integration coverage of wrapper handoff + CONTEXT.md update'
status: In Progress
assignee: []
created_date: '2026-06-22 23:27'
updated_date: '2026-06-23 04:07'
labels:
  - talk-to
  - docs
dependencies: []
references:
  - task-11
  - crates/ghostty-voiced/tests/
  - CONTEXT.md
parent_task_id: TASK-11
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-11 (multi-wrapper correctness PRD).

Add a daemon-level integration test (real socket, mirroring `held_for_replay.rs` / `ordered_drain.rs`): two wrappers register, the active one disconnects, the other becomes active and receives a subsequently-bound transcript; the focused-window sink returns only after the last wrapper exits. Update CONTEXT.md (Delivery sink) to describe the newest-live handoff, replacing the v1 drops-to-focused-window note. Depends on the registry change.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Integration test proves handoff to the other live wrapper when the active wrapper disconnects (no focused-window fallback while one remains)
- [ ] #2 The focused-window sink returns only when the last wrapper exits
- [ ] #3 CONTEXT.md updated to describe the newest-live handoff
- [ ] #4 cargo test green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Depends on TASK-11.1 (done). Two parts:

1. Integration test `crates/ghostty-voiced/tests/wrapper_handoff.rs`, mirroring sink_registration.rs / held_for_replay.rs (real socket, real SinkRegistry + DeliveryQueue + Frame protocol; double only at the socket peer):
   - Two wrapper clients register in order A then B (B becomes active).
   - A (the active... actually B is active; close A first to show non-active close is clean, then close B). Per AC: close the ACTIVE one and prove handoff. So: register A then B (B active); close B → handoff to A (A active), bind+drain a transcript to A, assert A receives it; then close A (last wrapper) → focused-window returns.
   - Deterministic ordering via accept-then-spawn-next + a drop signal channel.
2. CONTEXT.md (Delivery sink section): replace the v1 "switched back to focused-window" note with the newest-live handoff description.
3. cargo test/clippy/fmt green.
<!-- SECTION:PLAN:END -->
