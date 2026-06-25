---
id: TASK-18.1
title: >-
  streaming slice 1: live append plumbing — Shift+F9 streams raw text into the
  prompt end-to-end
status: To Do
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:27'
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

## Acceptance criteria
- [ ] Shift+F9 → `Trigger::Streaming`, consumed not forwarded, sends the `streaming` command (pure `trigger::scan` test)
- [ ] `Command::Streaming` and `State::Streaming` added to the protocol and round-trip through encode/parse (pure tests)
- [ ] The daemon starts a `Capture::Streaming` under the one-mouth invariant; a second concurrent recording is refused
- [ ] A self-paced decode loop decodes the growing WAV via `post_inference` (`beam_size=1`) and pushes live raw text to the active wrapper; proven by an integration test against the fake whisper-server
- [ ] The wrapper appends the live text into the child PTY with no trailing newline
- [ ] The dictation ends on ~10s trailing silence or Shift+F10; both stop capture cleanly (SIGINT-then-wait)
- [ ] With no wrapper registered, the live preview no-ops and the final result is Held-for-replay
- [ ] `cargo test --workspace` green; clippy and fmt clean

## Blocked by
None - can start immediately.
<!-- SECTION:DESCRIPTION:END -->
