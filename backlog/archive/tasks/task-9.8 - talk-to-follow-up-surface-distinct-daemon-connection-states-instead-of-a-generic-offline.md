---
id: TASK-9.8
title: >-
  talk-to follow-up: surface distinct daemon connection states instead of a
  generic "offline"
status: To Do
assignee: []
created_date: '2026-06-22 23:20'
labels:
  - talk-to
dependencies: []
references:
  - task-9
  - crates/talk-to/src/main.rs
  - crates/ghostty-voice-core/src/protocol.rs
parent_task_id: TASK-9
priority: medium
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (talk-to PTY wrapper, Delivery sink v1).

## Problem
`talk-to`'s status strip shows `offline` for three distinct conditions: (a) no daemon reachable, (b) daemon reached but registration was rejected (e.g., an older daemon that does not understand `register-sink` returns an error and closes), and (c) a previously-good connection dropped. Collapsing these into one token made a real incompatibility (stale daemon, see the upgrade/restart follow-up) very hard to diagnose — it looked identical to "no daemon".

## Desired outcome
The strip shows a distinct, accurate token per condition, and `talk-to` logs the failure reason, so a user can immediately tell WHY dictation is not reaching the wrapper. Optionally introduce a protocol version/handshake so an incompatible daemon is detected explicitly rather than inferred from a rejected command.

This is the deliberately-dumb line protocol (no JSON until needed) — keep that style; a version token is fine if added.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The strip distinguishes at least: daemon unreachable, registration rejected/incompatible, and connection dropped — with distinct tokens
- [ ] #2 A daemon that rejects register-sink yields an incompatible/version status, not a generic offline
- [ ] #3 talk-to logs the failure reason (stderr or a log) for diagnosis
- [ ] #4 If a protocol version/handshake field is added, it round-trips and a mismatch is reported; covered by unit tests (test-first, no doubles)
- [ ] #5 Unit tests cover any protocol/parse changes; cargo test green
- [ ] #6 README documents the meaning of each strip status token
<!-- AC:END -->
