# Streaming dictation: a rough live preview with a batch-accurate reconcile

This **extends ADR-0002**, which built batch transcription first and *rejected live
sliding-window streaming outright* for two reasons: streaming decodes on partial context
(measurably worse accuracy), and "you can't type revising text into the prompt live anyway."
Both objections still stand on their own terms — and this decision does not overturn them. It
adds a streaming **mode** that sidesteps both by separating two lanes: a deliberately *rough*
live preview, and the unchanged batch path as the source of truth.

## Context

Batch decoding is the most accurate, so it stays the source of the delivered **Transcript**.
But for the user's 5–10 minute workflow the post-stop wait is minutes long and **nothing**
shows while you talk. ADR-0002's answer to the latency was Continuous mode (background clip
transcription); it does not give you *words landing in the prompt as you speak*. The user
wants that live feedback **without** giving up batch accuracy — which is exactly the tension
ADR-0002 declared unresolvable for streaming.

## Decision

Add a **streaming dictation** mode (Shift+F9) that runs two lanes over one capture:

- **Live lane — a rough preview.** A self-paced loop decodes a **bounded sliding window** of
  the growing capture on the single existing whisper-server (cheap `beam_size=1`, no
  `initial_prompt`, no correction dictionary) and pushes the raw text into the active wrapper's
  prompt. It revises in place via **LocalAgreement-2**: a word is committed (locked into a
  stable prefix that never flickers) only once two consecutive decodes agree on it; the unstable
  tail is rewritten as Whisper firms up. This is openly *partial-context* decoding — ADR-0002's
  first objection — but here it is explicitly a draft, not the delivered text.
- **Batch lane — the reconcile.** On stop (a ~10 s trailing silence, or Shift+F10 force-stop)
  the **existing full-utterance batch transcription** (beam-8 + `initial_prompt` + correction
  dictionary) runs over the *complete* capture and **replaces** the preview with the
  jargon-corrected **Transcript**. Every committed word is therefore ultimately batch-accurate;
  the live lane never costs accuracy, it only buys immediacy.

ADR-0002's second objection — that you can't type revising text into the prompt live — is
answered by making the wrapper own the edit: it tracks the stable/tail boundary in **Unicode
codepoints** and applies each revision as backspaces (`0x7f`) + retype, and it **suppresses the
user's keystrokes** for the duration of a dictation so nothing but our injection mutates the
composer (the backspace count can't desync). The reconcile is a single replace (erase the whole
preview, type the Transcript) — no double-typing, and, like every delivery, **no trailing
Enter** (review-before-Enter). `talk-to` remains the **sole interface** (ADR-0003): the live
preview and the final Transcript both go to the bound wrapper sink, and the final Transcript
flows through **Delivery** (bound-at-trigger, cached-before-type, **Held-for-replay** if the
bound wrapper died).

Cost stays bounded by the window: each live decode covers only the last `window_seconds`, so a
multi-minute dictation never grows the per-decode cost, and **no second model** is introduced —
the live lane is self-paced against the one warm `large-v3` server and is *allowed to lag* on a
busy GPU (the preview degrades; it never blocks).

## Consequences

- A fourth dictation mode joins Toggle / VAD / Continuous. Shift+F9 now starts **streaming**;
  hands-free batch **VAD** relinquishes that key but stays reachable via `ghostty-voice-ctl vad`.
  Shift+F10 keeps one meaning — stop whatever runs (a streaming dictation force-stops and
  finalizes), else toggle a batch recording.
- The trickiest logic is pure and unit-tested without a GPU: the LocalAgreement-2 **commit
  engine**, the **window-PCM** math, and the **pty edit-bytes** (beside `injection_bytes`). The
  daemon decode loop is thin glue, like the Continuous driver. Backspace edit-fidelity is proven
  in CI against a stand-in line editor; the real Claude Code composer is a one-time manual
  smoke-test.
- For a dictation **longer than the window** the live preview is a draft of only the trailing
  window (the commit engine sees windowed hypotheses) — acceptable precisely because the batch
  reconcile is the accurate one. The correction dictionary is deliberately **not** applied to the
  live preview; corrections land only in the reconcile.
- This does not reverse ADR-0002: batch remains the accuracy source and Continuous mode remains
  the latency-hiding long-form mode. Streaming is the *immediacy* mode layered on top, paid for
  by a rough preview that the batch pass always corrects.
