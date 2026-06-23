---
id: TASK-9.9
title: 'talk-to follow-up: report the active Delivery sink in ghostty-voice-ctl status'
status: To Do
assignee: []
created_date: '2026-06-22 23:20'
labels:
  - talk-to
dependencies: []
references:
  - task-9
  - crates/ghostty-voice-ctl/src/main.rs
  - crates/ghostty-voiced/src/main.rs
  - crates/ghostty-voice-core/src/protocol.rs
parent_task_id: TASK-9
priority: medium
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (talk-to PTY wrapper, Delivery sink v1).

## Problem
There is no way to query which **Delivery sink** is active. Verifying that dictation routes to a **wrapper sink** vs the **focused-window sink** currently requires tailing journald for the `delivered to wrapper sink SinkId(N)` vs `auto-typed (focused-window sink)` log lines. Users asked exactly this ("how do I make sure talk-to is doing passthrough and not the old setup").

## Desired outcome
`ghostty-voice-ctl status` reports the active sink (focused-window vs wrapper, with the wrapper id and the count of registered wrapper sinks), so verification is a single command. The change must be additive/backward-compatible with the existing `ok <state>` response line.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ghostty-voice-ctl status reports the active sink kind (focused-window vs wrapper) and the number of registered wrapper sinks
- [ ] #2 Output is additive and backward-compatible with the existing `ok <state>` response
- [ ] #3 Works correctly with and without a wrapper registered
- [ ] #4 protocol/daemon/ctl changes covered by unit tests (encode/parse, test-first) and a daemon-level integration test (mirroring ghostty-voiced/tests style) showing a registered wrapper reported as active; cargo test green
- [ ] #5 README documents the new status output
<!-- AC:END -->
