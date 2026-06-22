---
id: TASK-10.1
title: 'status: report the active Delivery sink + registered wrapper count'
status: To Do
assignee: []
created_date: '2026-06-22 23:26'
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
- [ ] #1 status output includes the active sink kind (focused-window vs wrapper) and the registered wrapper-sink count
- [ ] #2 Additive and backward-compatible with the existing `ok <state>` response
- [ ] #3 Works correctly with and without a wrapper registered
- [ ] #4 Test-first unit tests for protocol encode/parse + a daemon-level integration test (mirroring ordered_drain.rs) showing a registered wrapper reported active; cargo test green
- [ ] #5 README documents the new status output
<!-- AC:END -->
