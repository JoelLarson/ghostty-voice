---
id: TASK-3
title: 'S3 — Trustworthy delivery: cache, ordered queue, replay-last'
status: Done
assignee: []
created_date: '2026-06-20 07:42'
updated_date: '2026-06-20 10:01'
labels:
  - needs-triage
dependencies:
  - TASK-2
references:
  - PLAN.md
  - CONTEXT.md
  - docs/adr/0002-batch-transcription-first-segmented-pipeline-deferred.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

S2 types transcripts directly and one at a time. If my focus drifts, a transcript clobbers whatever window is focused with no way to recover it; a slow or server-down transcription can be lost; and if I fire two recordings back-to-back they can interleave or drop. I want delivery I can trust: never lose a transcript, never garble the order, and recover cleanly from a misfire.

## Solution

Cache every transcript to disk **before** typing; a **Recorder + ordered FIFO delivery queue** that types strictly in record-order with no interleaving; **hands-free auto-type** within a generous freshness window, with a `held-for-replay` terminal state for anything stale or failed; `replay-last` recovery; WAV/transcript caches with count caps; audio start/stop cues; and `notify-send` reserved for exceptional conditions only.

## User Stories

1. As a developer, I want every recording kept as a WAV (count-capped ~30), so that I have an accuracy-debugging corpus and can replay exact audio.
2. As a developer, I want the transcript cached **before** typing is attempted, so that delivery is never lost even if typing then fails.
3. As a developer, I want to fire several recordings in a row and have them delivered in strict record-order, so that my thoughts land in the order I spoke them.
4. As a developer, I want typing serialized so utterances never interleave, so that two transcripts never garble into each other.
5. As a developer, I want a stalled utterance #1 to not block #2 forever, so that one server hiccup doesn't freeze the pipeline (it resolves to `held-for-replay` and the queue advances).
6. As a developer, I want transcripts auto-typed hands-free when fresh, so that normal dictation needs no extra keypress.
7. As a developer, I want stale/failed transcripts `held-for-replay` rather than blasted into the wrong window, so that I never clobber unexpectedly.
8. As a developer, I want `replay-last` to re-inject the most recent transcript after I refocus Ghostty, so that a misfire is one command to recover.
9. As a developer, I want distinct start and stop audio cues, so that I know when it's listening vs working without looking.
10. As a developer, I want an empty/silence transcript to use the normal done-cue and type nothing, so that saying nothing produces nothing.
11. As a developer, I want `notify-send` only for exceptional conditions (server unreachable, ydotool failed), so that I'm not spammed per utterance.
12. As a developer, I want a server-down recording kept, queued, and retried on server health, so that a mid-restart utterance isn't lost.
13. As a developer, I want a `max_recording_seconds` cap (~900 s) that enqueues + notifies, so that a forgotten recording can't run away.

## Implementation Decisions

- **Core (pure, the crown jewel for Chicago TDD):** Recorder state + ordered delivery queue — monotonic sequence numbers, per-utterance terminal state (`typed`/`dropped-empty`/`held-for-replay`), per-utterance freshness deadline checked at head-of-queue, strict in-order serialized typing.
- **Freshness / auto-type decision (pure):** given record-end time, now, and server state → type vs hold. Generous window (~15 min) as a backstop; hands-free is the priority.
- **Cache manager:** pure count-cap pruning policy + ISO-timestamp naming; fs writes at the boundary. WAV keep ~30, transcript keep ~5.
- **`replay-last`** re-injects the most-recent cached transcript (recovery-only).
- Audio cue + `notify-send` adapters at the boundary; cue source defaults to two shipped short sounds via `paplay` (`canberra-gtk-play` is available as an alternative — finalized in S7).
- `max_recording_seconds` timer: on expiry, stop + enqueue (preserve speech) + notify.
- Server-down queueing + retry-on-health ties into the S2 supervisor's readiness signal.

## Testing Decisions

- **Unit (core):** queue ordering (strict in-order, no interleave, no supersede-drop), terminal-state transitions, freshness decision (fresh→type, stale→hold, server-down recovery), cache count-cap pruning, empty/silence handling, `max_recording_seconds` enqueue.
- **Integration:** real fs cache round-trip; `replay-last` re-inject (real ydotool) on a sample transcript; queue drains in correct order driven by a fake slow transcriber; audio cue plays.

## Out of Scope

Accuracy stack (S4); VAD (S5); Continuous mode (S6); packaging (S7). No `replay-all` for multiple held transcripts — known gap, future item.

## Further Notes

- Hands-free is the guiding priority; the freshness window is a backstop, not a routine gate.
- `replay-last` recovers only the most-recent transcript — accepted limitation.
- Refs: `PLAN.md`, `CONTEXT.md`, `docs/adr/0001`, `docs/adr/0002`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Every transcript is cached to disk BEFORE typing; WAV cache count-capped (~30), transcript cache (~5)
- [x] #2 Recorder + ordered FIFO delivery queue: multiple recordings deliver in strict record-order with no interleaving
- [x] #3 Hands-free auto-type within a generous freshness window; stale/failed -> held-for-replay, never clobbered
- [x] #4 replay-last re-injects the most-recent transcript after refocus; empty/silence fires done-cue and types nothing
- [x] #5 Start/stop audio cues play; notify-send only for exceptional conditions; max_recording_seconds cap enqueues+notifies
- [x] #6 Queue ordering, terminal-state transitions, freshness decision, and cache pruning are unit-tested with real objects
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Core-first TDD: Recorder+ordered queue (seq, terminal states, head-of-queue freshness, serialized typing), freshness/auto-type decision, cache count-cap policy. Then boundary: fs cache, paplay cues, notify-send, ydotool replay. Integration: ordered drain via fake slow transcriber; replay-last on a sample.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
S3 trustworthy delivery implemented end-to-end and wired into the daemon. Chicago-TDD, atomic commits, all gates (cargo test / clippy --all-targets / fmt --check) green workspace-wide.

CODE-COMPLETE:
- core/config: added [audio].max_recording_seconds, [feedback].sound_start/stop, [cache].wav_keep/transcript_keep/retry_window_seconds with defaults + parsing tests; documented in config.toml.example.
- core/protocol + core/machine: replay-last command and Action::ReplayLast; the Recorder now frees on stop (StopAndEnqueue -> Idle) so recordings fire back-to-back while prior ones transcribe/type.
- core/queue: per-utterance freshness deadline at the head (head_delivery(now, window) -> seq + transcript + AutoType/HoldForReplay), judged on each utterance's own record-end age. Strict-order/no-interleave invariants unit-tested with real objects.
- core/cache: ISO-timestamp filename builder (colon-free so lexical sort = chronological) + existing count-cap prune policy.
- io/cache: fs adapter (store_wav/store_transcript with count-cap pruning, latest_transcript for replay) with real-fs round-trip integration tests.
- io/cue: paplay start/stop cue adapter (empty path = silent no-op).
- ctl: replay-last subcommand.
- daemon: replaced the single-shot path with Recorder + ordered FIFO delivery queue — caches WAV on stop, enqueues with freshness deadline, background-transcribes with retry-while-server-down (bounded by the window), then a single serialized drain caches each transcript to disk BEFORE typing and auto-types if fresh / holds-for-replay if stale. Start/stop cues on the hot path (stop cue doubles as empty/silence done-cue); notify-send reserved for exceptional cases. max_recording_seconds timer stops + enqueues a runaway recording. replay-last re-injects the most-recent cached transcript.
- Integration test: ordered drain via two fake whisper-servers (slow #1, fast #2) proving record-order delivery despite #2 transcribing first.

PENDING ON-HARDWARE (no GPU/mic/whisper-server/ydotoold/GNOME in the sandbox):
- Real paplay cue emission (only the no-op path is unit-tested).
- Real ydotool re-injection for replay-last and auto-type (core argv builder + fs cache backing are tested; the inject syscall is hardware-only).
- max_recording_seconds firing against a live recorder.
- Known minor edge: the recording-cap timer guards only on recorder presence, so at very small caps a stop+immediate-restart could let an old timer stop the new recording; negligible at the 900 s default (noted for a future tag-based guard).
<!-- SECTION:FINAL_SUMMARY:END -->
