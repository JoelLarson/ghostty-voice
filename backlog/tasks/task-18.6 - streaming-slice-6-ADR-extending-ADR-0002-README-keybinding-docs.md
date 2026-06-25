---
id: TASK-18.6
title: 'streaming slice 6: ADR extending ADR-0002 + README/keybinding docs'
status: Done
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:29'
updated_date: '2026-06-25 05:21'
labels:
  - streaming
  - talk-to
  - docs
  - needs-triage
dependencies:
  - TASK-18.3
  - TASK-18.4
  - TASK-18.5
parent_task_id: TASK-18
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-18 — PRD: streaming dictation.

## What to build
Record the decision and document the feature. A **new ADR** captures **streaming preview + batch-accurate reconcile** as the conscious *extension* of ADR-0002 (which rejected live sliding-window streaming for accuracy): the live lane is an explicit rough preview, while every committed word is ultimately batch-accurate via the reconcile, and keystrokes are suppressed during dictation to keep in-prompt editing reliable. Update **README** and any keybinding docs for the new **streaming mode** and the **Shift+F9 (start) / Shift+F10 (force-stop = finalize)** bindings, and note VAD now lives on `ghostty-voice-ctl vad`. Update **CONTEXT.md** if the domain language needs a streaming entry. (Per CLAUDE.md, source comments reference the durable ADR/CONTEXT concepts, never ticket IDs.)

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 #1 A new `docs/adr/NNNN-*.md` records streaming-preview + batch-reconcile as the extension of ADR-0002, with context, decision, and consequences
- [x] #2 #2 README documents the streaming mode and the Shift+F9 / Shift+F10 bindings, and that VAD is reachable via `ghostty-voice-ctl vad`
- [x] #3 #3 CONTEXT.md reflects any new streaming domain language if warranted
- [x] #4 #4 `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.3, TASK-18.4, TASK-18.5
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Document the streaming feature.

1. New `docs/adr/0004-streaming-preview-with-batch-accurate-reconcile.md` — record streaming-preview + batch-reconcile as the conscious EXTENSION of ADR-0002 (which deferred live streaming for accuracy): context (the post-stop wait), decision (rough live preview via self-paced bounded-window LocalAgreement-2 decode + keystroke suppression, every committed word ultimately batch-accurate via the reconcile), consequences. Reference ADR-0002/0003 by concept.
2. README — document the streaming mode and the Shift+F9 (start) / Shift+F10 (stop/finalize) bindings, and that VAD now lives on `ghostty-voice-ctl vad`.
3. CONTEXT.md — add the streaming domain language (streaming dictation / live preview / commit engine / reconcile) consistent with the existing Utterance/Transcript/Recorder/Delivery vocabulary.

No source changes ⇒ cargo test/clippy/fmt stay green; verify and commit.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Documented the streaming feature.

**Changes**
- `docs/adr/0004-streaming-preview-with-batch-accurate-reconcile.md` (new): records streaming-preview + batch-accurate reconcile as the conscious EXTENSION of ADR-0002 (which rejected live sliding-window streaming). Addresses both of ADR-0002's objections head-on — partial-context accuracy (the live lane is openly a draft; the batch lane is the source of truth) and "can't type revising text live" (LocalAgreement-2 + codepoint-counted backspace edits + keystroke suppression). Context / decision / consequences; references ADR-0002/0003 by concept.
- `README.md`: a **Streaming dictation (Shift+F9)** section; the updated keybindings (Shift+F9 = start streaming, Shift+F10 = stop whatever runs / batch toggle); VAD now via `ghostty-voice-ctl vad`; the `[streaming]` config keys; the `streaming` strip state; ADR-0004 in the reference list; streaming added to the downloading-rejection and troubleshooting notes.
- `CONTEXT.md`: new domain entries — **Streaming dictation**, **Live preview**, **Stable prefix / Unstable tail**, **Reconcile** — consistent with the existing Utterance/Transcript/Recorder/Delivery/Held-for-replay vocabulary; the **Cancel** entry now notes erasing the live preview.

Per CLAUDE.md the new ADR/CONTEXT concepts are what source comments reference (e.g. "the conscious extension of ADR-0002"), never ticket IDs.

`cargo test --workspace` green, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean (docs-only change; the config.toml.example `[streaming]` table added in slice 4 still parses).
<!-- SECTION:FINAL_SUMMARY:END -->

<!-- AC:END -->

<!-- AC:END -->
