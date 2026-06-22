---
id: TASK-9.3
title: 'talk-to slice 3: push-sink protocol + registration + sink registry'
status: To Do
assignee: []
created_date: '2026-06-22 06:46'
labels:
  - needs-triage
  - talk-to
dependencies:
  - TASK-9.1
references:
  - task-9 (PRD)
  - 'IDEAS.md #4'
  - CONTEXT.md
  - crates/ghostty-voice-core/src/protocol.rs
parent_task_id: TASK-9
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (PRD: talk-to — PTY wrapper, Delivery sink v1).

## What to build

Make `talk-to` a registered push-sink the daemon can target. End to end:
- **Protocol** (`protocol.rs`): add a `register-sink` command and a daemon→client push frame for a Transcript, plus a state-update line. DECISION (resolved, revisit at review): keep the existing newline-delimited line protocol — transcripts are already newline-free, so no JSON is needed yet, consistent with protocol.rs's "no JSON until a field needs it." Use a one-line framing such as `transcript <text>` and `state <token>`.
- **Daemon connection handler**: add a PERSISTENT registered-sink connection path beside today's one-shot read-line/write-response. After `register-sink`, the connection stays open and the daemon writes frames to it; closing it (or the client dying) deregisters.
- **Sink registry / active-sink** (pure module): exactly one active sink; wrapper becomes active on register; focused-window sink becomes active on disconnect/death. Extends the spirit of `delivery::decide`.
- **`talk-to` client**: open a persistent connection and `register-sink` on launch; deregister on exit.

No real Transcript delivery yet (that is slice 4) — a test/echo frame is enough to prove the channel.

Chicago-style (classicist) TDD is required: protocol parse/encode and the sink-registry lifecycle are driven test-first with no doubles (same style as existing protocol.rs tests).

## Validation / success

Demoable: launch `talk-to` → daemon reports the wrapper sink active; exit → focused-window sink active; a frame round-trips over the socket.

## Blocked by

task-9.1 (provides the `talk-to` binary).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Launching `talk-to` registers a wrapper sink and the daemon reports it as the active sink.
- [ ] #2 Exiting `talk-to` (or dropping the connection) reactivates the focused-window sink.
- [ ] #3 At most one sink is active at any moment.
- [ ] #4 A `register-sink` command and a Transcript push frame round-trip over the control socket.
- [ ] #5 Chicago-style TDD: test-first unit tests (no doubles) for the protocol parse/encode and the sink-registry lifecycle/active-sink rules; `cargo test` green.
<!-- AC:END -->
