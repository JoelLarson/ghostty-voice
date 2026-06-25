---
id: TASK-18.4
title: >-
  streaming slice 4: bounded sliding window + [streaming] config for long
  dictation
status: Done
assignee:
  - '@Joel Larson'
created_date: '2026-06-25 04:28'
updated_date: '2026-06-25 05:13'
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
- [x] #1 #1 Pure window-PCM math returns the last `window_seconds` of PCM from a committed offset, clamped to bytes present, for full and sub-window inputs (unit-tested)
- [x] #2 #2 The daemon decode loop decodes only the bounded window; per-decode audio length stays bounded as the dictation grows (proven via the integration test with a long synthetic stream)
- [x] #3 #3 A `[streaming]` config table is parsed with the locked defaults and is upgrade-tolerant (missing table → defaults)
- [x] #4 #4 Committed audio drops out of the window so the stable prefix is never re-decoded
- [x] #5 #5 `cargo test --workspace` green; clippy and fmt clean

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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Bounded the live decode to a sliding window and added the `[streaming]` config table.

**Changes**
- core `window.rs` (new): pure `window_range(total_data_len, committed_offset, window_len) -> (start, len)` — the last `window_len` bytes, never starting before `committed_offset`, sample-aligned, clamped to bytes present; `len ≤ window_len` always. Plus `seconds_to_bytes` for the 16 kHz mono s16 format. Unit-tested (full window, sub-window, committed exclusion, offset+bound composition, alignment, clamp).
- io `audio.rs`: `write_window_wav(src, dest, window_bytes, committed_offset)` reuses the RIFF data-chunk scan (refactored to `wav_data_span`) to slice the window into a fresh canonical WAV, returning the window start. Real-file-I/O tests: a 60 s capture windows to exactly 15 s (bounded), a 2 s capture passes whole, a committed offset excludes the earlier audio.
- core `config.rs`: a `[streaming]` table (`window_seconds=15`, `beam_size=1`, `session_end_silence_seconds=10`, `silence_threshold_pct=3`) with `serde(default)` + `deny_unknown_fields` — a missing table loads from defaults (upgrade-tolerant). Tested (defaults, missing-table, partial-override, full-config); added to `config.toml.example` with the corrected Shift+F9/F10 trigger note.
- daemon `main.rs`: `drive_streaming` computes `window_bytes` from `config.streaming.window_seconds`, writes the windowed WAV to a per-session temp file (`*.window.wav`) and decodes that at `config.streaming.beam_size`, advancing a monotonic `window_offset` (committed audio drops out, never re-decoded); falls back to the whole capture if windowing fails. `start_streaming` arms sox with the `[streaming]` session-end-silence + threshold. The batch reconcile still decodes the complete capture. Window temp cleaned up on finalize/discard/teardown.

**ACs**: #1 pure window math (unit tests); #2 bounded per-decode (io long-stream test — 60 s → 15 s) and the daemon decodes only the window; #3 `[streaming]` defaults + upgrade-tolerant (config tests); #4 committed audio drops out (window math + io test + monotonic offset); #5 green.

`cargo test --workspace` green (core lib 235 tests), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

**Note**: the live lane keeps `committed_offset` aligned to the sliding-window left edge (not a word→audio timestamp map, which the JSON path doesn't expose) — so for dictations longer than the window the live preview is a rough draft of the trailing window, while the batch reconcile remains fully accurate (the locked "rough preview, batch-accurate reconcile" design).
<!-- SECTION:FINAL_SUMMARY:END -->

<!-- AC:END -->

<!-- AC:END -->
