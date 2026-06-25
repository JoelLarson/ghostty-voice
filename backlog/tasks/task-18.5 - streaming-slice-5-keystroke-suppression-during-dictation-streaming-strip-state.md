---
id: TASK-18.5
title: >-
  streaming slice 5: keystroke suppression during dictation + streaming strip
  state
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
Protect the live edits. While a streaming dictation is active, the wrapper **suppresses the user's keystrokes** to the wrapped agent (the trigger keys Shift+F9/F10 are still recognized; everything else is dropped, not forwarded), so the only thing mutating the composer is our injection and the backspace char-count can never desync. Suppression is bounded to the active-dictation window — it ends when the dictation finalizes (or is cancelled). The status **strip shows the streaming state** so it's visible that dictation is live and input is suppressed. `Cancel` (via `ghostty-voice-ctl cancel`) erases the streaming buffer and delivers nothing.

## Acceptance criteria
- [ ] While streaming is active, ordinary keystrokes from the user are dropped (not forwarded to the child); Shift+F9/F10 still resolve as triggers (pure split test)
- [ ] Suppression is scoped to the active dictation: before start and after finalize/cancel, input forwards verbatim again
- [ ] The status strip renders the streaming state while a dictation is active
- [ ] `Cancel` during streaming erases the streaming buffer in the prompt and delivers nothing
- [ ] `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.2
<!-- SECTION:DESCRIPTION:END -->
