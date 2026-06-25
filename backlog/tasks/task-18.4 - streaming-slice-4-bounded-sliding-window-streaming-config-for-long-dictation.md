---
id: TASK-18.4
title: >-
  streaming slice 4: bounded sliding window + [streaming] config for long
  dictation
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
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-18 — PRD: streaming dictation.

## What to build
Keep a 5–10 minute dictation cheap. Introduce **window-PCM math** (core, pure): given the growing capture file's bytes + format + a committed offset + `window_seconds`, produce the **bounded PCM window** to decode, so audio whose words are already committed drops out and each decode stays cheap regardless of total length (reuses the existing RIFF `data`-chunk scan; the file read stays I/O). The daemon's decode loop feeds this bounded window to the commit engine instead of the whole growing file. Add the `[streaming]` config table with the locked defaults.

Decisions locked (defaults, all overridable in `config.toml`): `window_seconds=15`, live-lane `beam_size=1`, `session_end_silence_seconds=10`, `silence_threshold_pct` = the existing VAD default.

## Acceptance criteria
- [ ] Pure window-PCM math returns the last `window_seconds` of PCM from a committed offset, clamped to bytes present, for full and sub-window inputs (unit-tested)
- [ ] The daemon decode loop decodes only the bounded window; per-decode audio length stays bounded as the dictation grows (proven via the integration test with a long synthetic stream)
- [ ] A `[streaming]` config table is parsed with the locked defaults and is upgrade-tolerant (missing table → defaults)
- [ ] Committed audio drops out of the window so the stable prefix is never re-decoded
- [ ] `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.1
<!-- SECTION:DESCRIPTION:END -->
