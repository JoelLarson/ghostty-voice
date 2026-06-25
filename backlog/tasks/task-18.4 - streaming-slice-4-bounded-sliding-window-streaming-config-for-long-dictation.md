---
id: TASK-18.4
title: >-
  streaming slice 4: bounded sliding window + [streaming] config for long
  dictation
status: In Progress
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
updated_date: '2026-06-25 05:08'
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Pure window-PCM math returns the last `window_seconds` of PCM from a committed offset, clamped to bytes present, for full and sub-window inputs (unit-tested)
- [ ] #2 The daemon decode loop decodes only the bounded window; per-decode audio length stays bounded as the dictation grows (proven via the integration test with a long synthetic stream)
- [ ] #3 A `[streaming]` config table is parsed with the locked defaults and is upgrade-tolerant (missing table → defaults)
- [ ] #4 Committed audio drops out of the window so the stable prefix is never re-decoded
- [ ] #5 `cargo test --workspace` green; clippy and fmt clean

## Blocked by
TASK-18.1
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Keep long dictations cheap with a bounded sliding window + a [streaming] config table.

1. core `window.rs` (new, test-first): pure byte math. `window_range(total_data_len, committed_offset, window_len) -> (start, len)` = `start = max(committed_offset, total - window_len)` aligned down to an s16 sample boundary; `len = total - start`. Per-decode length is bounded by `window_len`; audio before the committed offset never re-enters the window. Plus `seconds_to_bytes` for the 16 kHz mono s16 format. Unit-tested (full window, sub-window, committed-offset excludes earlier, alignment).
2. io `audio.rs` (extend): `write_window_wav(src, dest, window_bytes, committed_offset) -> Result<u64>` — read src, find the RIFF data chunk (reuse the chunk scan), compute the byte window via core math, write a fresh canonical WAV of just that window to dest, return the window start (the daemon's new monotonic floor). Real-file-I/O tested: a long synthetic WAV windows to ≤ window_bytes; a short one passes whole; a committed offset excludes earlier audio.
3. core `config.rs`: a `[streaming]` table — `window_seconds=15`, `beam_size=1`, `session_end_silence_seconds=10`, `silence_threshold_pct = the VAD default (3)`; serde default + deny_unknown_fields so a missing table → defaults (upgrade-tolerant). Tested; add it to config.toml.example.
4. daemon `main.rs`: `drive_streaming` computes `window_bytes` from `config.streaming.window_seconds`, maintains a monotonic `window_offset`, writes the windowed WAV to a per-session temp file and decodes THAT (live-lane `beam_size` from `[streaming]`); `start_streaming` uses the `[streaming]` session-end-silence + threshold for sox. The batch reconcile still decodes the complete WAV.
5. Integration/io test: a long synthetic stream proves per-decode window length stays bounded as the capture grows.

cargo test --workspace green; clippy + fmt clean; atomic commit.
<!-- SECTION:PLAN:END -->

<!-- AC:END -->
