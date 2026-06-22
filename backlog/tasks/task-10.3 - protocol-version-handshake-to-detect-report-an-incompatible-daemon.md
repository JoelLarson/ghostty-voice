---
id: TASK-10.3
title: 'protocol: version handshake to detect & report an incompatible daemon'
status: To Do
assignee: []
created_date: '2026-06-22 23:26'
labels:
  - talk-to
dependencies: []
references:
  - task-10
  - crates/ghostty-voice-core/src/protocol.rs
  - crates/talk-to/src/main.rs
  - crates/ghostty-voiced/src/main.rs
parent_task_id: TASK-10
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-10 (operability PRD).

Add a protocol version exchanged at `register-sink` time so `talk-to` can tell an old/incompatible daemon apart from an unreachable one and surface a clear **incompatible** status. Today an old daemon simply rejects `register-sink` and looks identical to "no daemon" — this was the exact confusion during dogfooding (a stale daemon after upgrade). Keep the newline line protocol (a version token, not JSON).

Builds on the distinct-strip-states issue (it adds the `incompatible` token). Chicago-style TDD for the version parse/encode + mismatch handling.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The protocol carries a version; talk-to detects a version/compat mismatch and reports it as `incompatible`, distinct from unreachable
- [ ] #2 An older daemon that doesn't speak the handshake is treated as incompatible (not unreachable)
- [ ] #3 Test-first unit tests cover the version parse/encode and mismatch handling; cargo test green
- [ ] #4 README documents the incompatible state and its remedy (restart/upgrade the daemon)
<!-- AC:END -->
