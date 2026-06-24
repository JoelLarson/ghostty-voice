---
id: TASK-17.2
title: >-
  Recorder module — single mic-capture facility with a structural one-mouth
  invariant
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
Give the **Recorder** the module CONTEXT.md already names: *"the single mic-capture facility. There is only ever one (you have one mouth); its state is `idle` or `recording`."* Today that state is smeared across `Daemon.current_wav`, `recorder`, `continuous`, `continuous_gen`, and the one-mouth invariant is only an implicit assumption (a batch `recorder` and a `continuous.recorder` being mutually exclusive is never enforced). Extract a Recorder owning `idle | recording`, with the active capture's output as **one sum type** (batch WAV + enqueue-seq *or* continuous session) so "both at once" is unrepresentable. The three mic-stop paths (`DiscardRecording`, `stop_and_enqueue`, `teardown`) collapse into one. Scope is **capture-state only** — the continuous-mode driver loop stays in the daemon this round. Sits on the managed-child seam from TASK-17.1 for spawning. Behaviour unchanged: toggle/VAD/continuous start+stop, discard, and shutdown teardown act exactly as before.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A Recorder module owns idle|recording; the active capture's output is one sum type (batch OR continuous), so a second concurrent recording is unrepresentable/refused
- [ ] #2 The three mic-stop paths (DiscardRecording, stop_and_enqueue, teardown) collapse to one stop path on the Recorder
- [ ] #3 The daemon no longer holds raw current_wav/recorder/continuous recorder fields as independent state
- [ ] #4 Recorder uses the TASK-17.1 managed-child seam to spawn capture
- [ ] #5 Chicago-style unit tests written test-first prove the one-mouth invariant and one-Utterance-per-stop
- [ ] #6 No behaviour change for toggle/VAD/continuous start+stop, discard, teardown; cargo test --workspace green, clippy + fmt clean

## Blocked by
TASK-17.1 (managed-child seam) — so Recorder capture is built on the seam rather than rewritten twice.

## Working agreement
- **Chicago-style (classicist) TDD**, every change: red → green → refactor, test-first; assert observable behaviour through real collaborators; test doubles only at true external boundaries (deep pure modules use none).
- **Tidy after every green**: once a test passes, do the small structural cleanups (rename, dedupe, extract) as a distinct step before moving on.
- **Atomic commits**: one logical change per commit, suite green at each; keep the test-first commit and the tidy commit separate where it reads cleanly.
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
