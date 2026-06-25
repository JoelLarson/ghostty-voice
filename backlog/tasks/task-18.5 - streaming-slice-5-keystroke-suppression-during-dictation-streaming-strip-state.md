---
id: TASK-18.5
title: >-
  streaming slice 5: keystroke suppression during dictation + streaming strip
  state
status: Done
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
updated_date: '2026-06-25 05:18'
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
Protect the live edits. While a streaming dictation is active, the wrapper **suppresses the user's keystrokes** to the wrapped agent (the trigger keys Shift+F9/F10 are still recognized; everything else is dropped, not forwarded), so the only thing mutating the composer is our injection and the backspace char-count can never desync. Suppression is bounded to the active-dictation window — it ends when the dictation finalizes (or is cancelled). The status **strip shows the streaming state** so it's visible that dictation is live and input is suppressed. `Cancel` (via `ghostty-voice-ctl cancel`) erases the streaming buffer and delivers nothing.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 While streaming is active, ordinary keystrokes from the user are dropped (not forwarded to the child); Shift+F9/F10 still resolve as triggers (pure split test)
- [x] #2 #2 Suppression is scoped to the active dictation: before start and after finalize/cancel, input forwards verbatim again
- [x] #3 #3 The status strip renders the streaming state while a dictation is active
- [x] #4 #4 `Cancel` during streaming erases the streaming buffer in the prompt and delivers nothing
- [x] #5 #5 `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.2
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Protect the live edits: suppress keystrokes during a dictation, show the strip, cancel erases the buffer.

1. core `trigger.rs` (test-first): `scan_suppressed(buf) -> Vec<Trigger>` — while streaming, the trigger combos (Shift+F9/F10) still resolve but every other byte is dropped (not buffered). Pure split test: ordinary text drops, triggers still fire.
2. talk-to: hold a `streaming: bool` in `Shared`, set true on `State::Streaming` and false on any other state (finalize/cancel returns the daemon to Idle). The proxy loop uses `scan_suppressed` while streaming (dispatch triggers, drop the rest) and `scan` otherwise (forward verbatim) — suppression is scoped exactly to the active dictation.
3. Strip: already renders the `streaming` state token (slice 1) — confirm the wrapper repaints it from the `State::Streaming` frame.
4. daemon `main.rs`: `discard_streaming` (cancel) pushes a `Frame::Finalize("")` to the live bound wrapper so it erases the whole live preview buffer (delivering nothing), in addition to stopping sox and binning the capture.

cargo test --workspace green; clippy + fmt clean; atomic commit.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Keystroke suppression during a dictation, the streaming strip state, and cancel-erases-the-buffer.

**Changes**
- core `trigger.rs`: `scan_suppressed(buf) -> Vec<Trigger>` — while streaming, the Shift+F9/F10 combos still resolve but every other keystroke is dropped (not buffered). Pure split tests (ordinary text drops, triggers still fire, a trigger amid typed text still resolves while the text drops).
- talk-to: a `streaming: bool` in `Shared`, set from the `State::Streaming` frame and cleared on any other state (finalize/cancel returns the daemon to Idle). The proxy loop uses `scan_suppressed` while streaming (dispatch triggers, drop the rest) and the normal `scan` (forward verbatim, F12 debug) otherwise — suppression is scoped exactly to the active dictation.
- Strip: the wrapper repaints `sh.state = State::Streaming.label()` = "streaming" (slice 1 path); `strip::render` carries the token (new strip test).
- daemon `main.rs`: `discard_streaming` (cancel) pushes `Frame::Finalize("")` to the live bound wrapper so it erases the whole live preview buffer (delivers nothing), alongside stopping sox and binning the capture.

**ACs**: #1 suppressed split (trigger unit tests); #2 scoped to the active dictation (the `streaming` flag tracks `State::Streaming` only); #3 strip renders `streaming` (strip + protocol-label tests); #4 cancel erases the buffer and delivers nothing (pty empty-finalize test: 11 chars erased, nothing typed; daemon pushes the empty finalize); #5 green.

`cargo test --workspace` green (core lib 240 tests), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.
<!-- SECTION:FINAL_SUMMARY:END -->

<!-- AC:END -->

<!-- AC:END -->
