---
id: TASK-17.1
title: managed-child seam — unify external-process spawning with clean termination
status: To Do
assignee: []
created_date: '2026-06-24 04:52'
labels:
  - architecture
  - refactor
dependencies: []
parent_task_id: TASK-17
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-17 — PRD: architecture deepening — name the domain in code.

## What to build
Consolidate the scattered external-process spawning into one **managed-child** seam covering `sox` (the 3 recorder variants), `pw-record`, `whisper-server`, and `paplay`. Today each call site hand-rolls device/env pinning, bespoke `.context(...)` error messages, and termination differs (the recorder does SIGINT-then-wait via `audio::stop_recorder`, whisper-server uses `start_kill()`). Factor the policy — argv assembly, device pinning, error context, and clean SIGINT-then-wait termination — into a small seam. Because `audio.rs` runs on `std::process` and the daemon on `tokio::process`, expect a **pure sync core** (argv/pinning/termination policy, fully unit-testable) with thin sync and async adapters. Behaviour is unchanged: the same processes launch with the same args and the same observable cleanup.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single seam owns external-process argv assembly + device/env pinning + error context + clean SIGINT-then-wait termination
- [ ] #2 sox (all 3 variants), pw-record, whisper-server, and paplay spawn through it; the 3 duplicated device-pinning blocks in audio.rs are gone
- [ ] #3 whisper-server and the recorder share the same clean-termination policy (no more start_kill vs SIGINT-then-wait split)
- [ ] #4 The pure policy core has focused Chicago-style unit tests written test-first (argv, pinning on/off, termination decision)
- [ ] #5 No behaviour change: processes launch with identical args; cargo test --workspace green, clippy + fmt clean

## Blocked by
None — can start immediately.

## Working agreement
- **Chicago-style (classicist) TDD**, every change: red → green → refactor, test-first; assert observable behaviour through real collaborators; test doubles only at true external boundaries (the pure policy core uses none).
- **Tidy after every green**: once a test passes, do the small structural cleanups (rename, dedupe, extract) as a distinct step before moving on.
- **Atomic commits**: one logical change per commit, suite green at each; keep the test-first commit and the tidy commit separate where it reads cleanly.
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
