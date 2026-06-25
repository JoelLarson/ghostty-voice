---
id: TASK-18.3
title: 'streaming slice 3: batch-accurate reconcile + Delivery / Held-for-replay'
status: To Do
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
labels:
  - streaming
  - talk-to
  - needs-triage
dependencies:
  - TASK-18.2
parent_task_id: TASK-18
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-18 — PRD: streaming dictation.

## What to build
Make the streamed text land on **batch accuracy**. When capture stops (~10s silence or Shift+F10), the daemon runs the **existing full-utterance batch transcription** (the shared `transcribe` path: beam-8, `initial_prompt`, correction dictionary) over the complete WAV, then delivers it as a **finalize/replace frame**: the wrapper backspaces the entire current streaming buffer (`stable_len + tail_len`) and types the final, jargon-corrected **Transcript** (no trailing newline). No double-typing. The final Transcript flows through **Delivery** exactly as batch does — **bound-at-trigger**, cached-before-type, and **Held-for-replay** if the bound wrapper died (replay re-injects it as a plain append, no preview to reconcile). The live preview lane stays **ephemeral** (active wrapper only, bypassing the record-order queue).

## Acceptance criteria
- [ ] On stop, the full-utterance batch transcription runs over the complete WAV via the shared transcribe path (beam-8 + initial_prompt + corrections)
- [ ] A finalize/replace `Frame` round-trips through the protocol (pure test)
- [ ] The wrapper applies finalize by erasing the whole streaming buffer (`stable_len + tail_len`) and typing the final Transcript, newline-stripped — no double-typing (proven against the stand-in PTY line editor)
- [ ] The final Transcript is delivered through Delivery: bound-at-trigger, cached-before-type, and Held-for-replay when the bound wrapper is gone (proven at the Delivery interface)
- [ ] The live preview lane is ephemeral to the active wrapper and never enters the record-order queue
- [ ] The fake-whisper-server integration test asserts the final reconcile replaces the preview with the batch text
- [ ] `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.2
<!-- SECTION:DESCRIPTION:END -->
