---
id: TASK-18.3
title: 'streaming slice 3: batch-accurate reconcile + Delivery / Held-for-replay'
status: In Progress
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
updated_date: '2026-06-25 05:02'
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On stop, the full-utterance batch transcription runs over the complete WAV via the shared transcribe path (beam-8 + initial_prompt + corrections)
- [ ] #2 A finalize/replace `Frame` round-trips through the protocol (pure test)
- [ ] #3 The wrapper applies finalize by erasing the whole streaming buffer (`stable_len + tail_len`) and typing the final Transcript, newline-stripped — no double-typing (proven against the stand-in PTY line editor)
- [ ] #4 The final Transcript is delivered through Delivery: bound-at-trigger, cached-before-type, and Held-for-replay when the bound wrapper is gone (proven at the Delivery interface)
- [ ] #5 The live preview lane is ephemeral to the active wrapper and never enters the record-order queue
- [ ] #6 The fake-whisper-server integration test asserts the final reconcile replaces the preview with the batch text
- [ ] #7 `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.2
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Make the finalize REPLACE the live preview (no double-typing), still through Delivery.

1. core `protocol.rs`: add `Frame::Finalize(String)` — the finalize/replace frame the daemon pushes to a *live* bound wrapper (erase the whole streaming buffer, type the batch Transcript). Round-trip tested. (Held→replayed delivery stays a plain `Frame::Transcript` append — no preview to erase.)
2. talk-to: `apply_frame` handles `Frame::Finalize(text)` via `PreviewCursor::finalize` (erase preview_len chars, type the newline-stripped Transcript) — no double-typing.
3. daemon `main.rs`: `finalize_streaming` already runs the shared batch transcribe (beam-8 + initial_prompt + corrections) over the complete WAV and enqueues against the trigger-time binding; mark the enqueued seq as a streaming finalize (a `HashSet<u64>`). `drain_queue` pushes `Frame::Finalize` instead of `Frame::Transcript` for a marked seq routed to a live wrapper; `resolve` clears the marker. Held-for-replay path unchanged (cached, later replayed as a plain append).
4. Integration test `streaming_decode.rs`: route the finalize through a `Frame::Finalize` wire round-trip and assert the stand-in line editor's preview is replaced wholesale by the batch-accurate Transcript (no double-typing); the Held case stays a plain Transcript.

cargo test --workspace green; clippy + fmt clean; atomic commit.
<!-- SECTION:PLAN:END -->

<!-- AC:END -->
