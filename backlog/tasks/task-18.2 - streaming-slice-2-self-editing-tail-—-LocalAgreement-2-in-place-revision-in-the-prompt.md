---
id: TASK-18.2
title: >-
  streaming slice 2: self-editing tail — LocalAgreement-2 in-place revision in
  the prompt
status: To Do
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
labels:
  - streaming
  - talk-to
  - needs-triage
dependencies:
  - TASK-18.1
parent_task_id: TASK-18
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-18 — PRD: streaming dictation.

## What to build
Upgrade slice 1's append-only stream into **self-editing** text. Introduce the **streaming commit engine** (core, pure): feed it successive Whisper hypotheses and it applies **LocalAgreement-2** to split a **stable prefix** (a word is confirmed once two consecutive decodes agree) from the **unstable tail**, tracking a committed offset. The daemon emits a **live-edit frame** *(newly-committed text, current unstable tail)* instead of raw append. The wrapper holds `stable_len`/`tail_len` and applies each frame via **pure pty edit-bytes** (beside `injection_bytes`): backspace (`0x7f`) the previous tail, type the newly-committed text, then type the new tail — stable text, once committed, is never touched. End result: words self-correct in the prompt as Whisper hears more, settled words never flicker.

Decisions locked: count **Unicode codepoints** for backspaces (ASCII-dominant); edit-fidelity proven in CI against a **stand-in PTY line editor** (readline/bash), with the real Claude composer left as a one-time manual smoke-test.

## Acceptance criteria
- [ ] A pure streaming commit engine applies LocalAgreement-2 over successive hypotheses: stable prefix grows monotonically (never retracts), unstable tail is rewritten; idempotent re-emit when the hypothesis is unchanged (unit-tested with real word lists)
- [ ] A live-edit `Frame` carrying *(newly-committed, current tail)* round-trips through the protocol (pure test)
- [ ] Pure pty edit-bytes turn *(old-tail-len, newly-committed, new-tail)* into the correct backspaces+text, counting by Unicode codepoint (unit-tested)
- [ ] The wrapper applies live-edit frames so the prompt shows a stable prefix that never flickers and a tail that revises in place; proven against a stand-in PTY line editor
- [ ] The daemon emits live-edit frames from the commit engine; proven by the fake-whisper-server integration test feeding successive hypotheses and asserting committed/tail evolution
- [ ] `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.1
<!-- SECTION:DESCRIPTION:END -->
