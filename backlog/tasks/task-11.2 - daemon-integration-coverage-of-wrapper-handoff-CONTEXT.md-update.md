---
id: TASK-11.2
title: 'daemon: integration coverage of wrapper handoff + CONTEXT.md update'
status: To Do
assignee: []
created_date: '2026-06-22 23:27'
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
