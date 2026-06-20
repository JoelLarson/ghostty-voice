---
id: TASK-6
title: 'S6 — Continuous mode (north-star): clip pipeline + session assembly'
status: In Progress
assignee: []
created_date: '2026-06-20 07:42'
updated_date: '2026-06-20 10:21'
labels:
  - needs-triage
dependencies:
  - TASK-5
references:
  - PLAN.md
  - CONTEXT.md
  - docs/adr/0002-batch-transcription-first-segmented-pipeline-deferred.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

My real workflow is 5–10 minute dictation sessions. With batch modes I stop talking, then wait minutes for transcription — the exact opposite of a normal dialogue. I want to talk continuously, with natural pauses, and have my words flow in behind me, ending when I stop for a while. This is the experience the whole project exists to deliver.

## Solution

**Continuous mode** (its own hotkey): `sox` splits the session into silence-bounded **Clips** that batch-transcribe in the background, **pipelined** and **context-chained** (each clip's transcript tail seeds the next clip's `initial_prompt`). Clip transcripts assemble in record-order into one **Session** transcript; a long silence (~10 s) ends the session and delivers. Each clip is a full batch transcription, so `large-v3`/`beam-8` accuracy is preserved; because compute overlaps recording, the post-stop wait collapses to roughly one clip.

## User Stories

1. As a developer, I want to talk continuously with pauses, so that dictation feels like a normal dialogue, not a stop-and-wait transaction.
2. As a developer, I want short pauses to cut the audio into clips, so that each chunk can transcribe while I keep talking.
3. As a developer, I want clips to transcribe in the background as they finalize, so that most of the work is done by the time I stop.
4. As a developer, I want each clip seeded with the previous clip's transcript tail, so that cross-clip context (and accuracy) is preserved.
5. As a developer, I want clips transcribed in strict order and assembled in record-order, so that the final text reads exactly as I spoke it.
6. As a developer, I want a long silence (~10 s) to end the session and deliver the assembled transcript, so that I finish hands-free.
7. As a developer, I want a minimum clip size, so that stutters and micro-pauses don't spray tiny hallucination-prone fragments at Whisper.
8. As a developer, I want `cancel` to abort the whole session, so that I can throw away a bad take.
9. As a developer, I want the assembled transcript delivered hands-free on session end, so that the conversational flow isn't broken by a keypress.
10. As a developer, I want `clip_cut_pause`, `session_end_silence`, and `min_clip_seconds` configurable, so that I can tune segmentation to my speech rhythm.

## Implementation Decisions

- **Core (pure):** Session model (ordered Clips → assembled transcript); clip-pipeline orchestration (watch for finalized clips, enqueue serial transcription, chain prev-tail prompt, assemble in order); **dual-threshold silence semantics** (clip-cut pause vs session-end silence); min-clip-size accumulation.
- `sox` `silence ... : newfile : restart` splits the session into numbered clips; the daemon watches the clip directory and transcribes each finalized clip.
- Reuses **S4 accuracy** (per-clip `initial_prompt` + corrections) and **S3 delivery** (assembled transcript → cache-before-type → hands-free auto-type).
- This is the **seam S1–S5 must keep open**: continuous-capture-with-segmentation, not one-file-per-utterance.
- Serial transcription on the single GPU makes context-chaining **free** (clip N waits for N−1 anyway).
- Parameters: `clip_cut_pause` (~0.8–1.5 s), `session_end_silence` (~10 s), `min_clip_seconds` (~2–3 s) — config, empirically tuned.

## Testing Decisions

- **Unit (core):** clip→session assembly (record-order, with gaps), dual-threshold decision (cut vs end), min-clip accumulation, prompt-chaining (prev-tail → next `initial_prompt`), ordered serial transcription queue.
- **Integration:** real `sox` multi-clip split on a segmented sample; pipelined transcription assembles the correct ordered text; session-end on long silence delivers exactly once; `cancel` mid-session aborts cleanly.

## Out of Scope

Packaging (S7). Progressive per-clip typing (assemble-and-deliver-at-end is the model; live per-clip typing is a possible future variant). Sliding-window streaming (rejected — ADR-0002).

## Further Notes

- **The north-star deliverable.** The project isn't "done" until this lands.
- Accuracy is preserved because each clip is a full batch pass; segmentation at silence boundaries aligns with where Whisper's 30 s windows would break anyway.
- Refs: `PLAN.md`, `CONTEXT.md`, `docs/adr/0002`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Talking continuously cuts clips at short pauses; clips batch-transcribe in the background, context-chained via prev-clip tail
- [ ] #2 Clips transcribe in strict order and assemble in record-order into one Session transcript
- [ ] #3 A long silence (~10s) ends the session and delivers hands-free exactly once; cancel aborts the whole session
- [ ] #4 min_clip_seconds prevents tiny fragments; clip_cut_pause/session_end_silence/min_clip_seconds are configurable
- [ ] #5 Session assembly, dual-threshold decision, min-clip accumulation, and prompt-chaining are unit-tested with real objects
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Core-first TDD: Session model + clip-pipeline orchestration, dual-threshold silence semantics, min-clip accumulation, prompt-chaining, ordered serial transcription queue. sox 'silence ... : newfile : restart' splits clips; daemon watches the dir. Reuse S4 accuracy + S3 delivery. Integration: real multi-clip split assembles ordered text; session-end delivers once; cancel mid-session.
<!-- SECTION:PLAN:END -->
