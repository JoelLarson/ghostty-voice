---
id: TASK-18.1
title: >-
  streaming slice 1: live append plumbing — Shift+F9 streams raw text into the
  prompt end-to-end
status: Done
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:27'
updated_date: '2026-06-25 04:48'
labels:
  - streaming
  - talk-to
  - needs-triage
dependencies: []
parent_task_id: TASK-18
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-18 — PRD: streaming dictation (live self-editing preview + batch-accurate reconcile).

## What to build
The thinnest **end-to-end** streaming path: append-only, no editing yet. Pressing **Shift+F9** in `talk-to` is recognized as a new `Trigger::Streaming`, **consumed** (not forwarded to the agent), and sends a new `streaming` control command. The daemon starts a **streaming capture** (`sox` into a growing WAV) under the Recorder's one-mouth invariant (a `Capture::Streaming` variant) and runs a **self-paced** decode loop: decode the growing WAV via the existing whisper-server (`post_inference`, live-lane `beam_size=1`) and push the raw Whisper text as live frames to the **active wrapper sink**, which **appends** it into the wrapped program's PTY (no trailing newline — the Auto-type / review-before-Enter invariant). No stable/unstable editing this slice — commit-everything/append-only, so it proves the whole path and the self-paced cadence. The dictation ends on **~10s trailing silence** (sox long-silence auto-stop) or **Shift+F10** force-stop (SIGINT the child). A new `State::Streaming` shows while active.

Decisions locked: **self-paced, ship regardless of measured latency** (no human gate); **no wrapper registered → capture proceeds, live preview no-ops, final result Held-for-replay** (consistent with batch). VAD relinquishes the F9 start slot but stays reachable via `ghostty-voice-ctl vad`.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 Shift+F9 → `Trigger::Streaming`, consumed not forwarded, sends the `streaming` command (pure `trigger::scan` test)
- [x] #2 #2 `Command::Streaming` and `State::Streaming` added to the protocol and round-trip through encode/parse (pure tests)
- [x] #3 #3 The daemon starts a `Capture::Streaming` under the one-mouth invariant; a second concurrent recording is refused
- [x] #4 #4 A self-paced decode loop decodes the growing WAV via `post_inference` (`beam_size=1`) and pushes live raw text to the active wrapper; proven by an integration test against the fake whisper-server
- [x] #5 #5 The wrapper appends the live text into the child PTY with no trailing newline
- [x] #6 #6 The dictation ends on ~10s trailing silence or Shift+F10; both stop capture cleanly (SIGINT-then-wait)
- [x] #7 #7 With no wrapper registered, the live preview no-ops and the final result is Held-for-replay
- [x] #8 #8 `cargo test --workspace` green; clippy and fmt clean

## Blocked by
None - can start immediately.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Thinnest end-to-end streaming path: append-only live preview + final delivery through Delivery.

1. `trigger.rs` (core, test-first): remap Shift+F9 from `Trigger::Vad` to `Trigger::Streaming` (command word "streaming"); remove the `Vad` trigger variant (VAD relinquishes the F9 start slot — `Command::Vad` stays, reachable via `ghostty-voice-ctl vad`). Shift+F10 → `Trigger::Toggle` unchanged. Update tests.
2. `protocol.rs` (core, test-first): add `Command::Streaming` (parse "streaming"); add `State::Streaming` (encode/parse/label token "streaming"); round-trip tests. (LiveEdit/Finalize frames land in slices 2/3; slice 1 reuses the existing `Frame::Transcript` append + a plain text live frame.)
3. `machine.rs` (core, test-first): `Action::StartStreaming` / `StopStreaming` / `DiscardStreaming`; transitions — Idle+Streaming→(Streaming,StartStreaming); Recording+Streaming→ignored; Streaming+Streaming→ignored; Transcribing+Streaming→start; Streaming+Toggle→(Idle,StopStreaming) [F10 force-stop finalize]; Streaming+Cancel→(Idle,DiscardStreaming); Downloading/Loading reject Streaming.
4. io `audio.rs`: `spawn_streaming_recorder` (sox, ~10s trailing-silence auto-stop, same WAV contract) — real-sox test mirrors the VAD auto-stop test.
5. daemon `main.rs`: `StreamingSession` (generation, wav, recorder, committed offset, bound sink); `Action::StartStreaming` spawns the recorder + `drive_streaming`; the self-paced loop decodes the growing WAV via `post_inference` (beam_size=1) and pushes raw live text to the active wrapper sink (ephemeral, bypassing the queue); ends on sox auto-stop (~10s silence) or F10 SIGINT; on end delivers the final transcript through the Delivery queue (bound-at-trigger, Held-for-replay if no wrapper).
6. talk-to: append live text into the child PTY with no trailing newline (existing `injection_bytes`); apply the final transcript like a Transcript frame.
7. Integration test `tests/streaming_decode.rs`: stdlib fake whisper-server serving a scripted sequence of growing hypotheses; reconstruct the self-paced loop with real `post_inference`; assert live raw text is pushed and the final result routes through Delivery (Held when no wrapper).

cargo test --workspace green; clippy + fmt clean; atomic commits.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Shipped the thinnest end-to-end streaming-dictation path (append-only live preview + batch reconcile through Delivery).

**Changes**
- `trigger.rs`: Shift+F9 → `Trigger::Streaming` (command word "streaming"); removed the `Vad` trigger variant (VAD relinquishes the F9 start slot; `Command::Vad` stays, reachable via `ghostty-voice-ctl vad`). Shift+F10 → `Trigger::Toggle` unchanged.
- `protocol.rs`: `Command::Streaming`, `State::Streaming`, and an append-only `Frame::Live(text)` live-preview frame — all round-trip tested.
- `machine.rs`: `Action::StartStreaming`/`StopStreaming`/`DiscardStreaming` with the full transition table (Idle/Transcribing→start; Recording/Streaming start ignored — one-mouth; Streaming+Toggle→force-stop finalize; Streaming+Cancel→discard; Downloading/Loading reject).
- io `audio.rs`: `spawn_streaming_recorder` (sox with the long session-end-silence auto-stop), factored with the VAD recorder via a shared `spawn_silence_stopped_recorder`.
- daemon `main.rs`: `StreamingSession` + self-paced `drive_streaming` loop (live-lane `beam_size=1`, raw text, ephemeral push to the trigger-bound wrapper, bypassing the record-order queue); `finalize_streaming` runs the batch-accurate reconcile (beam-8 + initial_prompt + corrections) over the complete WAV and delivers through the `DeliveryQueue` against the trigger-time binding (Held-for-replay if the bound wrapper died). `discard_streaming` + `arm_streaming_cap` + teardown cleanup. Atomic finalize via a generation guard so force-stop and the hands-free silence path can't double-deliver.
- talk-to: `apply_frame` appends `Frame::Live` into the child PTY with no trailing newline (review-before-Enter).

**Tests (Chicago-style, no GPU/mic)**: pure trigger/protocol/machine units; `tests/streaming_decode.rs` reconstructs the decode loop against a stdlib fake whisper-server — growing hypotheses produce append-only live deltas (raw, uncorrected), the batch reconcile applies the dictionary, and the final Transcript routes through `SinkRegistry`+`DeliveryQueue` (to the bound wrapper, or Held when none).

`cargo test --workspace` green (all suites), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

**Note (deviation-of-record)**: built on `main`, which does not contain the TASK-17 module extraction (`Recorder`/`Delivery`/`Capture`/`managed_child`), per the AFK goal's "branch from main". The one-mouth invariant and Delivery/Held-for-replay are honoured structurally within the existing monolithic daemon and the CONTEXT.md domain vocabulary. The append-only preview will double-type against the final Transcript until slice 3 introduces the finalize/replace frame (the documented interim per the slice plan).
<!-- SECTION:FINAL_SUMMARY:END -->

<!-- AC:END -->

<!-- AC:END -->
