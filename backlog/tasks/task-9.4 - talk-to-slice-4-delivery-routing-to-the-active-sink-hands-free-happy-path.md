---
id: TASK-9.4
title: 'talk-to slice 4: delivery routing to the active sink (hands-free happy path)'
status: To Do
assignee: []
created_date: '2026-06-22 06:46'
labels:
  - needs-triage
  - talk-to
dependencies:
  - TASK-9.2
  - TASK-9.3
references:
  - task-9 (PRD)
  - 'IDEAS.md #4'
  - CONTEXT.md
  - crates/ghostty-voiced/src/main.rs (drain_queue)
parent_task_id: TASK-9
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (PRD: talk-to — PTY wrapper, Delivery sink v1).

## What to build

Wire the real voice path end to end so dictation lands in the wrapped agent with no intervention. Change the daemon's queue drain (`drain_queue`) to deliver each Transcript to the ACTIVE sink instead of hard-calling ydotool:
- **focused-window sink** = today's `ghostty_voice_io::inject::type_text` (unchanged, still gated by the Freshness window) — used when no wrapper is registered.
- **wrapper sink** = push the Transcript frame down the registered connection.

`talk-to` writes the received Transcript into the child PTY with NO trailing newline (review-before-Enter survives). The status strip now reflects REAL daemon state (idle/recording/transcribing) via the state-update frames from slice 3.

Chicago-style (classicist) TDD is required: a daemon-level integration test (real daemon, no mocks, mirroring ghostty-voiced/tests/ordered_drain.rs) asserts a registered wrapper sink receives the pushed Transcript end-to-end.

## Validation / success

Demoable hands-free: `talk-to ssh host claude` running → trigger a recording → spoken text appears in claude's input line over SSH with no Enter; strip tracks recording/transcribing live; with no wrapper running, focused-window Auto-type is exactly as today.

## Blocked by

task-9.2 (status strip) and task-9.3 (push-sink protocol + registry).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 With `talk-to ssh host claude` running, triggering a recording gets the spoken text into claude's input line over SSH, hands-free, with no trailing Enter.
- [ ] #2 The bottom status strip reflects recording/transcribing/idle in real time.
- [ ] #3 With no wrapper registered, focused-window Auto-type behaves exactly as today and existing tests still pass.
- [ ] #4 The Transcript is cached before delivery is attempted (write-before-deliver preserved).
- [ ] #5 Chicago-style TDD: a daemon-level integration test (real daemon, no mocks, mirroring ordered_drain.rs) proves a registered wrapper sink receives the pushed Transcript end-to-end; `cargo test` green.
<!-- AC:END -->
