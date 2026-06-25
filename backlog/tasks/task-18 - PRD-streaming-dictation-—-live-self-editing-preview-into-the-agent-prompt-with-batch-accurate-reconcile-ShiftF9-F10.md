---
id: TASK-18
title: >-
  PRD: streaming dictation — live self-editing preview into the agent prompt
  with batch-accurate reconcile (Shift+F9/F10)
status: To Do
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:21'
labels:
  - prd
  - streaming
  - talk-to
  - needs-triage
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Today dictation is **batch only**: you speak an **Utterance**, stop, then wait while whisper-server transcribes the whole thing before a single word reaches the agent's prompt. For the user's 5–10 minute workflow that post-stop wait is minutes long, and nothing shows while you talk. The user wants to *see their words land in Claude Code's prompt as they speak*, self-correcting as Whisper gains context, **without** giving up the batch-accurate, jargon-corrected final text that makes technical dictation usable (ADR-0002's whole point).

## Solution

A new **streaming mode** on **Shift+F9**. As you talk, a **live preview** flows into the wrapped agent's prompt and **revises in place** — a stable prefix stays put while the unstable tail is rewritten as Whisper firms it up. You dictate hands-free with pauses; a ~10s silence ends it, or **Shift+F10** force-stops and finalizes now. On stop, the existing full-utterance **batch** transcription (beam-8, `initial_prompt`, correction dictionary) **replaces** the preview with the accurate, jargon-corrected **Transcript** — live immediacy *and* batch accuracy. While streaming, the wrapper **suppresses your keystrokes** to the agent so live edits can't desync (you're dictating, not typing).

This consciously **extends ADR-0002**: the live lane is an explicit *rough preview* (a self-paced sliding-window decode); every committed word is ultimately batch-accurate via the reconcile. `talk-to` stays the **sole interface** (ADR-0003).

## User Stories

1. As a hands-free dictator, I want to press Shift+F9 and start talking, so that words appear in the prompt with no further keypress.
2. As a dictator, I want words to appear within a second or two of speaking, so that it feels live, not batch.
3. As a dictator, I want the tail to correct itself as Whisper hears more, so that early mistakes get fixed before I finalize.
4. As a dictator, I want already-settled words to stay put and never flicker, so that the growing preview stays readable.
5. As a dictator, I want a ~10s silence to end the dictation on its own, and Shift+F10 to finalize immediately, so that I can finish hands-free or in a hurry.
6. As a dictator, I want the final prompt text to be the batch-accurate, jargon-corrected Transcript (e.g. "ydotool", not "why do tool") injected without a trailing newline, so that streaming never costs accuracy and I still press Enter myself.
7. As a dictator, I want my keystrokes ignored while dictating, so that live edits to the prompt stay coherent.
8. As a user, I want streaming to use my one existing whisper-server (self-paced, lagging gracefully on a busy GPU) at bounded per-decode cost over a 10-minute dictation, so that I run no second model and it degrades instead of breaking.
9. As a user whose wrapper died mid-dictation, I want the final Transcript Held-for-replay, so that I recover it later (never silently misrouted).
10. As a user, I want VAD still reachable via `ghostty-voice-ctl vad` and the strip to show streaming in progress, so that I don't lose batch hands-free mode and I know keystrokes are suppressed.
11. As a maintainer, I want the commit policy as a pure unit-tested module and the daemon loop covered by an integration test, so that the trickiest logic is provable without a GPU.

## Implementation Decisions

**New deep modules (pure, isolation-tested):**
- **streaming commit engine (core)** — feed successive Whisper hypotheses; applies **LocalAgreement-2**, returns *(newly-committed words, current unstable tail)*, tracks the committed offset. Pure word-list math.
- **window-PCM math (core/io)** — given the growing capture file's bytes + format + committed offset + `window_seconds`, produce the bounded PCM window to decode (reuses the RIFF `data`-chunk scan; file read stays I/O).
- **pty edit-bytes (core, beside `injection_bytes`)** — pure: *(old-tail-len, newly-committed, new-tail) → bytes* (backspaces `0x7f` + text), and the finalize **replace** bytes (erase `stable_len + tail_len`, type the final newline-stripped Transcript).

**Extended pure modules (additive):**
- **trigger** — `Trigger::Streaming` (Shift+F9 → `"streaming"`). VAD relinquishes the F9 start slot (still via `ghostty-voice-ctl vad`). F10 keeps one meaning: stop whatever runs (streaming → force-stop = finalize), else start a Toggle batch when idle.
- **protocol** — `Command::Streaming`; `State::Streaming` (one-line add via `WORD_TOKENS`); two new live `Frame`s — a live-edit frame *(newly-committed, current tail)* and a finalize/replace frame.

**Glue (thin, like today's Continuous driver loop):**
- **streaming decoder loop (daemon)** — read window → build WAV → `post_inference` (live-lane profile, default `beam_size=1`) → run the engine → emit frames; **self-paced** (next decode on the last's return); single existing whisper-server.
- **capture (io) + Recorder** — `spawn_streaming_recorder` = `sox` with a long (~10s) trailing-silence auto-stop tolerating short pauses; a `Capture::Streaming` variant under the one-mouth invariant. Shift+F10 SIGINTs the child early. Either path ends → final WAV complete → reconcile.
- **Delivery** — the live preview lane is **ephemeral**, pushed to the *active* wrapper, bypassing the queue. The **final batch Transcript** flows through Delivery as today (bound-at-trigger, cached-before-type, **Held-for-replay**), delivered as the *finalize/replace* frame, not an append.
- **wrapper edit-application** — tracks `stable_len`/`tail_len`, applies frames via the pure pty logic, **suppresses keystrokes** while streaming (multi-byte counted by character; wrapping irrelevant — the composer deletes logical chars).

**Config** — a `[streaming]` table: `window_seconds` (~15–20), live-lane `beam_size` (default 1), `session_end_silence_seconds` (~10), `silence_threshold_pct`. **Corrections** apply in the **batch reconcile only**; the live preview shows raw Whisper text.

## Testing Decisions

**Chicago-style (classicist) TDD is required** — test-first red-green-refactor, asserting observable behaviour through *real* collaborators; doubles only at the true external boundary (whisper-server over a localhost socket). Tests assert external behaviour, not implementation detail.

Unit-tested (real objects, no GPU): the **streaming commit engine** (LocalAgreement-2 newly-committed/tail evolution, idempotent re-emit, retraction-free stable prefix); **window-PCM math** (offset/window byte math, sub-window inputs); **pty edit-bytes** (old-tail→new-tail emission, finalize replace); **trigger** (Shift+F9 → Streaming, consumed not forwarded; F10 still stops); **protocol** (`Command`, `State::Streaming`, both new `Frame`s round-trip).

Integration-tested: the **daemon streaming decoder loop** against the existing fake whisper-server (the `transcribe.rs` real-socket style), feeding successive hypotheses and asserting the emitted committed/tail frames and the final reconcile. Prior art: pure style of `session.rs`/`sink.rs`/`queue.rs`/`machine.rs`/`pty.rs`/`trigger.rs`; real-socket style of `transcribe.rs`.

## Success Validation

1. Shift+F9 starts streaming; words appear live in the prompt; the unstable tail self-revises while the stable prefix never flickers.
2. A ~10s silence ends it hands-free; Shift+F10 force-stops and finalizes now; both run the reconcile.
3. After finalize the prompt holds the **batch-accurate, jargon-corrected** Transcript (preview fully replaced, no double-typing, no trailing newline).
4. Keystrokes are suppressed during an active dictation; live edits never desync.
5. Streaming uses the single existing whisper-server, self-paced, bounded per-decode cost across a multi-minute dictation (no second model).
6. The final Transcript is delivered through Delivery and **Held-for-replay** if its bound wrapper died.
7. LocalAgreement-2, window-PCM, pty edit-bytes, trigger, and protocol additions proven by Chicago-style unit tests; the streaming loop proven by an integration test against the fake whisper-server.
8. A new ADR records the streaming-preview + batch-reconcile extension of ADR-0002; README/docs cover the new mode and Shift+F9/F10. `cargo test --workspace` green, clippy + fmt clean.

## Issues (vertical slices)

Each slice independently shippable and green on its own. Recommended order:

1. **Feasibility spike** — growing-window decode + LocalAgreement-2 editing live in a real Claude Code composer; measure self-paced cadence and backspace fidelity (throwaway code allowed).
2. **streaming commit engine + window-PCM math (core)** — the two deep pure modules, test-first.
3. **protocol + trigger** — `Command::Streaming`, `State::Streaming`, the two live `Frame`s, `Trigger::Streaming`; pure, round-trip tested.
4. **capture + Recorder + streaming decoder loop (daemon)** — `spawn_streaming_recorder`, `Capture::Streaming`, the self-paced loop, plus the integration test.
5. **wrapper edit-application + keystroke suppression** — apply live-edit/finalize frames via the pure pty edit-bytes; suppress keystrokes; strip shows streaming.
6. **batch reconcile + Delivery integration + ADR/docs** — finalize replaces the preview via Delivery (Held-for-replay); new ADR + README/keybinding docs.

## Out of Scope

- True in-place editing inside arbitrary wrapped agents — the target is Claude Code's composer; the `cat -v` debug sink is not a goal.
- Applying the correction dictionary to the *live* preview (deferred to the reconcile).
- A second/smaller streaming model or streaming-capable server — the single existing `large-v3` server is used.
- Buffering/replaying suppressed keystrokes (dropped; the strip indicates dictation).
- Re-litigating ADR-0001/0003; removing batch Toggle/VAD/Continuous (they remain; VAD via the ctl CLI).

## Further Notes

The spike is first because the two real risks — sustained decode cadence on one large-model server, and backspace edit fidelity in the composer — are empirical and could reshape later slices.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Shift+F9 starts a streaming dictation when idle: a new `streaming` command and `Trigger::Streaming` are recognized and consumed by the wrapper (not forwarded to the agent)
- [ ] #2 While streaming, a live preview of the spoken words appears in the wrapped agent's prompt and revises in place — stable prefix committed and never flickering, unstable tail rewritten as Whisper firms up — and never presses Enter
- [ ] #3 The streaming decoder is self-paced against the single existing whisper-server (no second model) and decodes a bounded sliding window so per-decode cost stays bounded across a multi-minute dictation
- [ ] #4 Stable-vs-unstable splitting uses LocalAgreement-2 and is proven by pure, real-object unit tests over word sequences
- [ ] #5 While a streaming dictation is active, the user's keystrokes to the wrapped agent are suppressed so live edits cannot desync (characters counted, not bytes)
- [ ] #6 A streaming dictation ends hands-free on ~10s of trailing silence, or immediately on Shift+F10 (force-stop); both paths run the finalize/reconcile
- [ ] #7 On finalize, the full-utterance batch transcription (beam-8 + initial_prompt + correction dictionary) replaces the live preview with the batch-accurate, jargon-corrected Transcript — no double-typing, no trailing newline
- [ ] #8 The final Transcript flows through Delivery (bound-at-trigger, cached-before-type, Held-for-replay if the bound wrapper died); the live preview lane is ephemeral to the active wrapper and bypasses the record-order queue
- [ ] #9 Each deep/extended module (streaming commit engine, window-PCM math, pty edit-bytes, trigger, protocol command/state/frames) has Chicago-style unit tests written test-first; the daemon streaming decoder loop has an integration test against the fake whisper-server asserting committed/tail evolution and the final reconcile
- [ ] #10 A new ADR records streaming-preview + batch-reconcile as the conscious extension of ADR-0002; README/docs cover the new mode and the Shift+F9/F10 bindings; cargo test --workspace is green and clippy + fmt are clean
<!-- AC:END -->
