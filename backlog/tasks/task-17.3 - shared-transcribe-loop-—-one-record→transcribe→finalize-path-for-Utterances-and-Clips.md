---
id: TASK-17.3
title: >-
  shared transcribe loop — one record→transcribe→finalize path for Utterances
  and Clips
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
Unify `transcribe_clip` and `transcribe_with_retry` (~127 lines, two copies that have already drifted) into **one simple retry loop** owning the transcribe step: build `InferenceParams`, duration-filter, retry-until-window on `post_inference`, then `finalize_transcript`. Parameterise only the genuine differences — the optional `initial_prompt` tail (Clips chain context; Utterances don't) and sub-min-duration handling (Clip deletes + empty vs Utterance `Ok(None)`). **Keep it a simple loop** (the user's explicit instruction) — no state machine, no executor abstraction. Both the batch **Utterance** path and the Continuous **Clip** path call it. Behaviour unchanged: same retry window, same finalize, same prompt-chaining for Clips.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One shared retry-until-window + finalize function is the only transcribe loop; transcribe_clip / transcribe_with_retry no longer duplicate it
- [ ] #2 The genuine differences (initial_prompt tail, sub-min handling) are explicit parameters, not forked copies
- [ ] #3 Both the Utterance and the Clip paths call the shared loop
- [ ] #4 It remains a plain loop — no new state-machine or executor abstraction
- [ ] #5 Chicago-style unit tests written test-first cover: retry within the window, give-up past the window, finalize returns text or None
- [ ] #6 No behaviour change to batch or continuous transcription; cargo test --workspace green, clippy + fmt clean

## Blocked by
None — can start immediately.

## Working agreement
- **Chicago-style (classicist) TDD**, every change: red → green → refactor, test-first; assert observable behaviour through real collaborators; test doubles only at true external boundaries.
- **Tidy after every green**: once a test passes, do the small structural cleanups (rename, dedupe, extract) as a distinct step before moving on.
- **Atomic commits**: one logical change per commit, suite green at each; keep the test-first commit and the tidy commit separate where it reads cleanly.
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
