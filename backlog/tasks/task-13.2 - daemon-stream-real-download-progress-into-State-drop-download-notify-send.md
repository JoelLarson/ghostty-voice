---
id: TASK-13.2
title: 'daemon: stream real download progress into State, drop download notify-send'
status: In Progress
assignee:
  - claude
created_date: '2026-06-23 05:57'
updated_date: '2026-06-23 06:08'
labels:
  - needs-triage
  - talk-to
dependencies:
  - TASK-13.1
references:
  - task-13
  - crates/ghostty-voiced/src/main.rs
  - crates/ghostty-voice-io/src/download.rs
parent_task_id: TASK-13
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-13 (PRD: report model download progress on the talk-to strip + status).

## What to build
Make the daemon produce real download percentages into the new `State`. On entering the first-run fetch, `set_state(State::Downloading(None))`. The download runs in `spawn_blocking`; its progress callback sends whole-percent-throttled updates over a channel, and a concurrent async loop applies them via `set_state(Downloading(Some(pct)))` so the cached state (read by `status`) and the `watch<State>` broadcast (read by the strip) stay in lockstep. Remove the download-related `notify-send` calls (initial "downloading the ~3 GB model", the per-10% milestones, "download complete — loading", "download failed — retrying"); keep their `info!/warn!/error!` journald logs and every non-download notification.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 During the fetch the daemon advances `State::Downloading(Some(pct))` (`None` until a total is known), throttled to whole-percent changes
- [ ] #2 `ghostty-voice-ctl status` reports `downloading <pct>` and the talk-to strip shows `downloading <pct>%`, both from the single `set_state` source (never diverging)
- [ ] #3 No `notify-send` is emitted for download progress/start/complete/failure; journald logs retained; all non-download notifies unchanged
- [ ] #4 The whole-percent throttle is a pure, unit-tested helper (test-first)
- [ ] #5 cargo test/clippy/fmt green; the live first-run download (network + ~3 GB model) reported honestly as demo-only

## Blocked by
TASK-13.1
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Test-first for the pure throttle, then wire the sync→async channel and drop the download notify-send.

1. Pure helper (red→green), unit-tested in the daemon crate: `PercentThrottle { last: Option<u8> }` with `update(&mut self, pct: Option<u8>) -> Option<u8>` returning Some(p) only when the whole percent advances to a new value (dedups repeated chunks at the same percent; None passes through as no-op). Add `#[cfg(test)] mod tests` in main.rs.
2. `download_model_once(daemon, config, dest)`: set_state(Downloading(None)) at attempt start (so a retry restarts the percent); create a tokio unbounded mpsc<u8>; spawn an async applier task that does `set_state(Downloading(Some(pct)))` per received pct; run the blocking `download_model` in spawn_blocking, its on_progress closure feeding `PercentThrottle` and sending Some(pct) over the channel; on closure end the sender drops, applier task ends; await both, propagate the JoinError/result.
3. ensure_model_present: pass `daemon` to download_model_once; remove the three download notify-send calls (initial "downloading the ~3 GB model", "download complete — loading", "download failed — retrying"); keep info!/error! journald logs; remove the per-10% milestone notify (now replaced by the channel). Every non-download notify untouched.
4. cargo test/clippy/fmt green. Live ~3 GB network download not exercisable headless → report demo-only; throttle + protocol/label units + pushed-frame integration test cover the wiring.
<!-- SECTION:PLAN:END -->

<!-- AC:END -->
