---
id: TASK-10.3
title: 'protocol: version handshake to detect & report an incompatible daemon'
status: Done
assignee: []
created_date: '2026-06-22 23:26'
updated_date: '2026-06-23 04:22'
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
- [x] #1 The protocol carries a version; talk-to detects a version/compat mismatch and reports it as `incompatible`, distinct from unreachable
- [x] #2 An older daemon that doesn't speak the handshake is treated as incompatible (not unreachable)
- [x] #3 Test-first unit tests cover the version parse/encode and mismatch handling; cargo test green
- [x] #4 README documents the incompatible state and its remedy (restart/upgrade the daemon)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Chicago-style TDD. Keep the line protocol — a version token, not JSON.

1. protocol.rs: `PROTOCOL_VERSION: u32 = 1`; `Command::RegisterSink(Option<u32>)` (None = legacy bare register-sink); `parse` accepts `register-sink [version]`; `version_compatible(client, daemon)`. Update unit tests (bare → RegisterSink(None), versioned → Some(v), bad version → error; compatible 1/1 true, 0/1 & 2/1 false).
2. machine.rs: arm `(s, Command::RegisterSink(_))`.
3. link.rs: add `LinkState::Incompatible` (token "incompatible"); make `classify_first_line` 3-way {Registered, Incompatible, Rejected} — a Frame ⇒ Registered; an `err` whose message starts with "incompatible" (new daemon) or "unknown command" (old daemon that errs on the versioned command) ⇒ Incompatible; any other err/junk ⇒ Rejected. Update/extend tests (old-daemon err now Incompatible).
4. daemon handle_conn: match `RegisterSink(version)`; if Some(v) and !version_compatible → reply `err incompatible protocol version ...` and close; else serve_sink. Update all integration-test register-sink matches to `RegisterSink(_)`.
5. talk-to: send `register-sink <PROTOCOL_VERSION>`; handle Registration::Incompatible → strip token "incompatible" + log.
6. README: document the incompatible state + remedy (restart/upgrade the daemon).
7. cargo test/clippy/fmt green.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a `register-sink` protocol version handshake so an incompatible daemon is legible. Committed as 4a4df88.

- protocol.rs (Chicago-style TDD): `PROTOCOL_VERSION = 1`, `Command::RegisterSink(Option<u32>)` (None = legacy bare register-sink, still accepted), `parse` of `register-sink [version]` (non-numeric version → error), and `version_compatible(client, daemon)` (exact match). Unit tests cover all of these.
- link.rs: `LinkState::Incompatible` (token "incompatible", distinct from the other three); `classify_first_line` is now 3-way — a pushed Frame ⇒ Registered, an `err` starting with "incompatible" (new daemon refusal) or "unknown command" (old daemon that errs on the versioned command) ⇒ Incompatible, any other err/junk ⇒ Rejected.
- ghostty-voiced handle_conn: parses `RegisterSink(version)`; an incompatible Some(v) is refused with an explicit `err incompatible protocol version … — restart/upgrade the daemon` and the connection closes; legacy/compatible registrations serve as before.
- talk-to: sends `register-sink <PROTOCOL_VERSION>` and maps Registration::Incompatible to the `incompatible` strip token + a log line.
- Integration test `incompatible_daemon.rs` (real Command::parse + version_compatible + Response + classify_first_line over a real socket): a version mismatch, an old daemon's `err unknown command`, and a compatible registration.
- README documents the incompatible state and the restart/upgrade remedy.

AC #1–#4 met. An old daemon is reported as `incompatible`, never `unreachable` (AC #2). `cargo test --workspace` (275), clippy, fmt green.
<!-- SECTION:FINAL_SUMMARY:END -->
