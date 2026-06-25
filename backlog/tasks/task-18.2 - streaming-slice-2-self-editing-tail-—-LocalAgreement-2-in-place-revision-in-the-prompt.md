---
id: TASK-18.2
title: >-
  streaming slice 2: self-editing tail — LocalAgreement-2 in-place revision in
  the prompt
status: Done
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
updated_date: '2026-06-25 05:01'
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
- [x] #1 #1 A pure streaming commit engine applies LocalAgreement-2 over successive hypotheses: stable prefix grows monotonically (never retracts), unstable tail is rewritten; idempotent re-emit when the hypothesis is unchanged (unit-tested with real word lists)
- [x] #2 #2 A live-edit `Frame` carrying *(newly-committed, current tail)* round-trips through the protocol (pure test)
- [x] #3 #3 Pure pty edit-bytes turn *(old-tail-len, newly-committed, new-tail)* into the correct backspaces+text, counting by Unicode codepoint (unit-tested)
- [x] #4 #4 The wrapper applies live-edit frames so the prompt shows a stable prefix that never flickers and a tail that revises in place; proven against a stand-in PTY line editor
- [x] #5 #5 The daemon emits live-edit frames from the commit engine; proven by the fake-whisper-server integration test feeding successive hypotheses and asserting committed/tail evolution
- [x] #6 #6 `cargo test --workspace` green; clippy and fmt clean

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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Upgraded slice 1's append-only preview into a self-editing one via LocalAgreement-2.

**Changes**
- core `streaming.rs` (new): pure `CommitEngine` applying LocalAgreement-2 — a word commits once two consecutive decodes agree on it at the same position beyond the committed offset; the committed prefix grows monotonically and never retracts; re-observing an unchanged hypothesis is idempotent. `observe(&[&str]) -> LiveEdit { committed, tail }` renders both fields with their joining spaces.
- core `pty.rs`: `edit_bytes(old_tail_len, newly_committed, new_tail)` (erase the old tail by Unicode codepoint via `0x7f`, type committed + new tail), `finalize_bytes(buffer_len, final_text)` (erase the whole preview, type the newline-stripped Transcript), `codepoint_len`, and a `PreviewCursor` tracking the stable/tail boundary that the wrapper drives.
- core `protocol.rs`: replaced the append-only `Frame::Live` with `Frame::LiveEdit { committed, tail }` (US `\x1f`-separated on the dumb line protocol — no JSON); round-trip tested including empty fields.
- daemon `main.rs`: the `StreamingSession` holds a `CommitEngine` + `last_tail`; `drive_streaming` feeds each decode to the engine and pushes `Frame::LiveEdit`, suppressing a no-op re-decode (empty commit + unchanged tail) so the tail never needlessly flickers.
- talk-to: holds a `PreviewCursor`, applies `Frame::LiveEdit` via `edit_bytes`, resets it on entering `State::Streaming`.

**Tests (Chicago-style)**: `CommitEngine` units (commit-one-decode-behind, monotone non-retracting prefix, tail-revision isolation, idempotent re-emit, rendering/spacing); pty edit-bytes + a stand-in line editor (DEL = delete one logical char) proving the stable prefix never flickers, the tail revises in place, deletes count codepoints not bytes, and finalize replaces the whole preview; the integration test now feeds growing **and revised** hypotheses through the real engine + cursor + line editor over the real `Frame::LiveEdit` wire round-trip, asserting the committed prefix grows monotonically (the mishear never enters it) and the preview reconciles to the batch-accurate, corrected Transcript routed through Delivery.

`cargo test --workspace` green (core lib 223 tests), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

**Note**: the finalize-replace via the wrapper (`PreviewCursor::finalize`) is exercised by the pty/integration tests but is not yet wired into the live delivery path — the daemon still delivers the final Transcript as a plain append; slice 3 switches finalize to the replace frame so there is no double-typing end to end.
<!-- SECTION:FINAL_SUMMARY:END -->

<!-- AC:END -->

<!-- AC:END -->
