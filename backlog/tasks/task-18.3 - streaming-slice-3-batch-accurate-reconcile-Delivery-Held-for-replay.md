---
id: TASK-18.3
title: 'streaming slice 3: batch-accurate reconcile + Delivery / Held-for-replay'
status: Done
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
updated_date: '2026-06-25 05:05'
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
- [x] #1 #1 On stop, the full-utterance batch transcription runs over the complete WAV via the shared transcribe path (beam-8 + initial_prompt + corrections)
- [x] #2 #2 A finalize/replace `Frame` round-trips through the protocol (pure test)
- [x] #3 #3 The wrapper applies finalize by erasing the whole streaming buffer (`stable_len + tail_len`) and typing the final Transcript, newline-stripped — no double-typing (proven against the stand-in PTY line editor)
- [x] #4 #4 The final Transcript is delivered through Delivery: bound-at-trigger, cached-before-type, and Held-for-replay when the bound wrapper is gone (proven at the Delivery interface)
- [x] #5 #5 The live preview lane is ephemeral to the active wrapper and never enters the record-order queue
- [x] #6 #6 The fake-whisper-server integration test asserts the final reconcile replaces the preview with the batch text
- [x] #7 #7 `cargo test --workspace` green; clippy and fmt clean

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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The streaming finalize now REPLACES the live preview with the batch-accurate Transcript (no double-typing), still flowing through Delivery.

**Changes**
- core `protocol.rs`: `Frame::Finalize(String)` — the finalize/replace frame pushed to a *live* bound wrapper; round-trip tested (precedence-checked so a Transcript beginning with "finalize" stays a Transcript).
- talk-to: `apply_frame` handles `Frame::Finalize` via `PreviewCursor::finalize` — erase the whole streaming buffer (`stable_len + tail_len`) and type the newline-stripped Transcript.
- daemon `main.rs`: `finalize_streaming` runs the shared batch transcribe (beam-8 + initial_prompt + corrections) over the complete WAV, enqueues against the trigger-time binding, and marks the seq in a new `streaming_seqs` set. `drain_queue` pushes `Frame::Finalize` (not `Frame::Transcript`) for a marked seq routed to a live wrapper, clearing the marker on resolve. Cache-before-type and Held-for-replay are unchanged — a held streaming finalize replays as a plain `Frame::Transcript` append (no preview to erase).

**Behaviour vs the ACs**
- #1 batch transcribe over the complete WAV via the shared path — done (slice 1, retained).
- #2 finalize/replace frame round-trips — `Frame::Finalize` test.
- #3 wrapper erases the whole buffer and types the Transcript, newline-stripped, no double-typing — proven against the stand-in line editor (pty unit + integration test; `preview_len()==0` after).
- #4 delivered through Delivery: bound-at-trigger, cached-before-type, Held-for-replay — the existing `DeliveryQueue`/`SinkRegistry`/cache path, exercised in the integration test (wrapper route → Finalize; no wrapper → Held → plain replay).
- #5 the live preview lane is ephemeral (direct push to the bound wrapper, bypassing the queue); only the final Transcript enters the queue.
- #6 the integration test asserts the reconcile replaces the preview with the batch text over a real `Frame::Finalize` wire round-trip.

`cargo test --workspace` green, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.
<!-- SECTION:FINAL_SUMMARY:END -->

<!-- AC:END -->

<!-- AC:END -->
