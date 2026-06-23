---
id: TASK-10.2
title: 'talk-to: distinguish connection states on the strip + log the reason'
status: In Progress
assignee: []
created_date: '2026-06-22 23:26'
updated_date: '2026-06-23 04:13'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Chicago-style TDD on a new pure module; talk-to wiring is OS glue (not unit-tested), strip presentation stays visually verified.

1. core: new `link.rs` with `LinkState {Unreachable, Rejected, Dropped}` (`.token()` → "unreachable"/"rejected"/"dropped") and `classify_first_line(&str) -> Registration {Registered, Rejected}` (a parseable Frame ⇒ Registered; an `err`/garbage first reply ⇒ Rejected — an old daemon that doesn't know register-sink replies `err unknown command`). Unit tests, no doubles. (Incompatible + version is the sibling issue 10.3.)
2. talk-to spawn_sink_client: connect-fail → Unreachable; after register-sink, classify the first line → Registered (process frames; set strip token from Frame::State) or Rejected; EOF after being registered → Dropped. Log the detailed reason (connect error / rejection) to a best-effort log file (writing to stderr would corrupt the raw-mode TUI). Strip token comes from LinkState when not cleanly connected.
3. Document token meanings (brief here; fuller in 10.4).
4. cargo test/clippy/fmt green.
<!-- SECTION:PLAN:END -->
