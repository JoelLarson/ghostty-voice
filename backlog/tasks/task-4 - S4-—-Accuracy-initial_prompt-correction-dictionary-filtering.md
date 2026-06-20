---
id: TASK-4
title: 'S4 — Accuracy: initial_prompt, correction dictionary, filtering'
status: To Do
assignee: []
created_date: '2026-06-20 07:42'
labels:
  - needs-triage
dependencies:
  - TASK-3
references:
  - PLAN.md
  - CONTEXT.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Technical jargon — "ydotool", "Ghostty", "useEffect", "kubectl", "rebase" — is reliably mistranscribed, and silence or very short clips produce hallucinations ("Thank you.", `[BLANK_AUDIO]`) that get typed into my prompt. Accuracy on my vocabulary is the single hardest part of this tool, and I need it to type nothing when I said nothing.

## Solution

A layered, stacking accuracy stack: `initial_prompt` vocabulary biasing (**bounded** to Whisper's token cap), a deterministic case-insensitive **correction dictionary** (a jargon spell-fixer, not a code-symbol munger), `beam-8`, `temperature 0`, and empty/hallucination/sub-min-duration filtering that discards garbage and types nothing.

## User Stories

1. As a developer, I want the decoder seeded with a domain vocab list via `initial_prompt`, so that rare technical terms are biased toward correct spelling.
2. As a developer, I want to grow the vocab list in config as I notice misses, so that accuracy improves over time without a rebuild.
3. As a developer, I want the vocab **bounded** so it can't silently overflow the ~224-token `initial_prompt` cap, so that later terms don't quietly stop biasing — with a warning when truncation would occur.
4. As a developer, I want a correction dictionary of case-insensitive find/replace pairs, so that terms Whisper reliably mishears the same way ("why do tool" → "ydotool") are fixed deterministically.
5. As a developer, I want correction ordering and word-boundary semantics well-defined, so that replacements are predictable and don't corrupt adjacent text.
6. As a developer, I want `beam-8` and `temperature 0`, so that ambiguous audio resolves accurately and deterministically.
7. As a developer, I want `[BLANK_AUDIO]`, silence-"Thank you", and other known hallucinations filtered, so that they're never typed.
8. As a developer, I want sub-0.3 s recordings discarded, so that accidental blips produce nothing.
9. As a developer, I want an empty transcript to fire the done-cue and type nothing, so that saying nothing is silent.
10. As a developer, I want `reload` to re-read vocab + corrections + key-delay without a model reload, so that I tune accuracy live.

## Implementation Decisions

- **Core (pure):** correction-dictionary engine (case-insensitive, deterministic ordering + word-boundary rules); `initial_prompt` builder (assembles vocab and **bounds to ~224 tokens**, logs/warns on truncation); hallucination/empty/min-duration filter (pure predicate over transcript text + duration + a known-hallucination set).
- Request params (`beam_size`, `temperature`, `initial_prompt`) wired into the S1 transcription transport.
- `reload` applies vocab/corrections/key-delay without reloading the model (uses the S2 `reload` command path).
- Corrections live in `config.toml` `[corrections]`; vocab in `[whisper].initial_prompt` / a vocab list.
- Explicitly **not** building: code-symbol substitution, camelCase/snake_case formatting, shell-vs-code detection — these corrupt natural-language prose (English prose only).

## Testing Decisions

- **Unit (core):** correction dictionary (case-insensitivity, multi-term, ordering, word boundaries, no over-match); `initial_prompt` builder (under cap passes; over cap truncates + warns); filter (blank-audio, known hallucinations, sub-min-duration discarded; real speech passes).
- **Integration:** end-to-end on sample WAVs from the S3 cache corpus — jargon terms come out corrected; a silence WAV types nothing.

## Out of Scope

Code-symbol substitution / camelCase / shell-detection (removed by design); VAD (S5); Continuous mode (S6); packaging (S7).

## Further Notes

- The S3 WAV cache doubles as the accuracy-debugging corpus — replay the exact audio Whisper misheard.
- The token-cap bounding is the silent-trap fix flagged during grilling.
- Refs: `PLAN.md`, `CONTEXT.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 initial_prompt vocab biasing wired, BOUNDED to the ~224-token cap with a warning on truncation
- [ ] #2 Correction dictionary: case-insensitive deterministic find/replace with defined ordering/word-boundary semantics
- [ ] #3 beam-8 and temperature 0 applied; [BLANK_AUDIO]/known-hallucination/sub-0.3s discarded and never typed
- [ ] #4 reload re-reads vocab+corrections+key-delay without a model reload
- [ ] #5 Correction engine, initial_prompt builder (cap+truncation), and the filter predicate are unit-tested with real objects
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Core-first TDD: correction-dictionary engine, initial_prompt builder (token-cap bound + warn), hallucination/empty/min-duration filter. Wire beam/temp/initial_prompt into the transcription transport. Integration: sample WAVs from the cache corpus -> corrected jargon; silence -> nothing typed. No code-symbol substitution.
<!-- SECTION:PLAN:END -->
