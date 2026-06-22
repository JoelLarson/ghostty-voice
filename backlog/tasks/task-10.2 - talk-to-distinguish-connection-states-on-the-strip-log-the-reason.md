---
id: TASK-10.2
title: 'talk-to: distinguish connection states on the strip + log the reason'
status: To Do
assignee: []
created_date: '2026-06-22 23:26'
labels:
  - talk-to
dependencies: []
references:
  - task-10
  - crates/talk-to/src/main.rs
parent_task_id: TASK-10
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-10 (operability PRD).

Replace the single `offline` strip token with distinct states the client can detect without any protocol change: **unreachable** (connect failed), **dropped** (was registered, then EOF), and **rejected** (daemon returned an error to `register-sink`). Log the reason to stderr/log for diagnosis. (Explicit version-based "incompatible" detection is the sibling issue.)

Strip presentation stays visually verified per task-9; any pure condition→token mapping is unit-tested (test-first, no doubles).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The strip shows distinct tokens for unreachable, dropped, and rejected connection states
- [ ] #2 talk-to logs the failure reason for diagnosis
- [ ] #3 Any pure condition→token mapping is covered by test-first unit tests; cargo test green
- [ ] #4 The token meanings are documented (here or folded into the troubleshooting docs issue)
<!-- AC:END -->
