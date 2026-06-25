---
id: TASK-18.2
title: >-
  streaming slice 2: self-editing tail — LocalAgreement-2 in-place revision in
  the prompt
status: In Progress
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
updated_date: '2026-06-25 04:53'
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A pure streaming commit engine applies LocalAgreement-2 over successive hypotheses: stable prefix grows monotonically (never retracts), unstable tail is rewritten; idempotent re-emit when the hypothesis is unchanged (unit-tested with real word lists)
- [ ] #2 A live-edit `Frame` carrying *(newly-committed, current tail)* round-trips through the protocol (pure test)
- [ ] #3 Pure pty edit-bytes turn *(old-tail-len, newly-committed, new-tail)* into the correct backspaces+text, counting by Unicode codepoint (unit-tested)
- [ ] #4 The wrapper applies live-edit frames so the prompt shows a stable prefix that never flickers and a tail that revises in place; proven against a stand-in PTY line editor
- [ ] #5 The daemon emits live-edit frames from the commit engine; proven by the fake-whisper-server integration test feeding successive hypotheses and asserting committed/tail evolution
- [ ] #6 `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.1
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Upgrade slice 1's append-only stream into self-editing in-place revision.

1. core `streaming.rs` (new, test-first): `CommitEngine` applying LocalAgreement-2. `observe(&[&str]) -> LiveEdit { committed: String, tail: String }`: a word is committed once two consecutive decodes agree on it at the same position beyond the committed offset; the stable prefix grows monotonically (never retracts); the unstable tail is the rendering past the committed boundary. Rendered strings carry the joining spaces (committed delta leads with a space when it follows committed text; tail leads with a space when any committed text precedes it). Idempotent: re-observing the already-committed hypothesis yields empty committed+tail. Unit-tested over real word lists.
2. core `pty.rs` (extend, test-first): `edit_bytes(old_tail_len, newly_committed, new_tail) -> Vec<u8>` = `old_tail_len` × `0x7f` (DEL erases one logical char) then `newly_committed` bytes then `new_tail` bytes — counting by Unicode codepoint. Plus a `PreviewCursor` holding `stable_len`/`tail_len` (codepoints) that applies each LiveEdit via `edit_bytes` (stable grows by the committed chars; tail_len becomes the new tail chars) and resets between dictations. Unit-tested with multibyte text.
3. core `protocol.rs`: replace slice 1's append-only `Frame::Live(text)` with `Frame::LiveEdit { committed, tail }` carrying the two fields (US `\x1f` separator on the dumb line protocol — no JSON); round-trip tested including empty fields.
4. daemon `main.rs`: the StreamingSession holds a `CommitEngine` instead of `appended_words`; `drive_streaming` feeds each decode's words to the engine and pushes `Frame::LiveEdit` from its output.
5. talk-to: hold a `PreviewCursor`; apply `Frame::LiveEdit` via `edit_bytes`; reset on entering `State::Streaming`.
6. Integration test `streaming_decode.rs`: feed successive growing/revised hypotheses through the real `CommitEngine`, assert the committed prefix grows monotonically and the tail revises in place, and that applying the edits to a stand-in line editor (DEL = delete one logical char) reproduces the expected preview without ever deleting into the stable prefix; the final reconcile still replaces via the batch path.

cargo test --workspace green; clippy + fmt clean; atomic commit.
<!-- SECTION:PLAN:END -->

<!-- AC:END -->
