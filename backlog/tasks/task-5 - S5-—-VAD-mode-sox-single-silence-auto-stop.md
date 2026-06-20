---
id: TASK-5
title: 'S5 — VAD mode: sox single silence auto-stop'
status: Done
assignee: []
created_date: '2026-06-20 07:42'
updated_date: '2026-06-20 10:20'
labels:
  - needs-triage
dependencies:
  - TASK-4
references:
  - PLAN.md
  - CONTEXT.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Toggle mode requires a deliberate second press to stop recording. For quick single utterances I want it hands-free: I start, and it stops on its own when I stop talking — without me reaching for a key again.

## Solution

A **VAD mode** bound to `Super+Shift+D`: `sox` records and self-terminates on detected trailing silence, then the utterance flows through the same delivery pipeline as any other. Pressing `toggle` during a VAD recording is a manual early stop. Silence thresholds are configurable.

## User Stories

1. As a developer, I want `Super+Shift+D` to start a VAD recording, so that I can dictate a quick utterance hands-free.
2. As a developer, I want `sox` to auto-stop after ~2 s of trailing silence, so that I don't press anything to finish.
3. As a developer, I want the VAD transcript delivered through the S3 pipeline (cache, order, auto-type), so that it behaves like every other utterance.
4. As a developer, I want `toggle` during a VAD recording to act as a manual early stop, so that I can cut it short when I'm done early.
5. As a developer, I want `vad_silence_seconds` and `vad_threshold_pct` configurable, so that I can tune to my mic and room.
6. As a developer, I want a never-speak VAD recording backstopped by `max_recording_seconds`, so that a muted mic or dead silence can't record forever.
7. As a developer, I want `install-hotkeys` to bind `vad`, so that the mode is reachable from a hotkey out of the box.

## Implementation Decisions

- The Recorder gains a **VAD strategy**: `sox` with `silence 1 0.1 <thr> 1 <sec> <thr>` (config-driven), producing the **same WAV contract** as `pw-record` so the rest of the pipeline is unchanged.
- `sox` becomes a hard dependency — **currently missing on the machine**, must be installed.
- VAD threshold config (`vad_silence_seconds`, `vad_threshold_pct`); tuning is empirical (real-mic).
- `install-hotkeys` extended to bind `vad`.
- The never-speak hang is covered by S3's `max_recording_seconds` cap (the leading-silence trigger may never arm if no speech ever rises above threshold).

## Testing Decisions

- **Unit (core):** `sox` argument construction from threshold config (pure); recorder-strategy selection (toggle vs VAD).
- **Integration:** real `sox` auto-stop on a silence-trailing sample; manual early-stop via `toggle` during VAD; never-speak → `max_recording_seconds` cap fires.

## Out of Scope

Continuous mode's multi-clip segmentation (S6 — VAD is single-stop, one utterance); packaging (S7).

## Further Notes

- VAD is conceptually "Continuous mode, but stop at the first silence instead of segmenting." It shares silence-detection groundwork with S6 but stays single-utterance — keep that seam in mind.
- VAD threshold defaults need real-mic tuning (deferred open item).
- Refs: `PLAN.md`, `CONTEXT.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Super+Shift+D starts a VAD recording; sox auto-stops after ~2s trailing silence; transcript flows through the S3 pipeline
- [x] #2 toggle during a VAD recording acts as a manual early stop; vad_silence_seconds/vad_threshold_pct are configurable
- [x] #3 A never-speak VAD recording is backstopped by max_recording_seconds; install-hotkeys binds vad; sox installed as a dependency
- [x] #4 sox arg construction and recorder-strategy selection are unit-tested; real-sox auto-stop covered by integration
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add a VAD recorder strategy (sox silence ...) producing the same WAV contract as pw-record. Core-first TDD: sox arg builder + strategy selection. Integration: real sox auto-stop on a silence-trailing sample; manual early stop; never-speak cap. Install sox; extend install-hotkeys.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Code-complete; all sandbox gates green (cargo test / clippy --all-targets / fmt --check).

Implemented (Chicago TDD, real objects, atomic commits):
- protocol: Command::Vad ("vad" wire word).
- machine: Idle+Vad -> Recording via Action::StartVadRecording; toggle during VAD = manual early stop (existing Recording+Toggle=StopAndEnqueue); vad-while-recording ignored; vad rejected while loading.
- config: vad_silence_seconds (2.0) + vad_threshold_pct (3) in [audio], defaults/parsing/tests + config.toml.example.
- io: spawn_vad_recorder launches sox with core::vad's `silence 1 0.1 <thr> 1 <sec> <thr>` effect, same 16k/mono/s16 WAV contract as pw-record.
- daemon: StartVadRecording branch spawns the sox recorder, plays start cue, arms max_recording_seconds cap, and starts watch_vad_autostop which enqueues the utterance when sox self-terminates on silence -> flows through the S3 delivery pipeline. Never-speak hang backstopped by the cap.
- ctl: Cmd::Vad + bind vad to <Super><Shift>d in install-hotkeys.

Tests: machine (vad start / manual early stop / ignored-while-recording / rejected-while-loading), protocol parse, config defaults+full-parse, ctl vad-word; real-sox integration (skip if sox absent): trailing-silence auto-stop trim, never-speak retains no speech, SIGINT clean stop + readable WAV.

sox is present on the box (/bin/sox) and already declared in PKGBUILD depends.

Commits: feat(config): VAD silence threshold settings; feat(io): sox VAD recorder strategy with real-sox tests; feat(vad): wire VAD command through the daemon and ctl.

Pending on-hardware validation (no GPU/mic/whisper-server/ydotoold/GNOME in sandbox): live-mic auto-stop tuning of vad_silence_seconds/vad_threshold_pct; the never-speak->time-cap path on a real muted mic (file-based proxy only here); Super+Shift+D binding under a real GNOME session; full end-to-end speak->type via VAD.
<!-- SECTION:NOTES:END -->
