---
id: TASK-13.2
title: 'daemon: stream real download progress into State, drop download notify-send'
status: To Do
assignee: []
created_date: '2026-06-23 05:57'
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

## Acceptance criteria
- [ ] During the fetch the daemon advances `State::Downloading(Some(pct))` (`None` until a total is known), throttled to whole-percent changes
- [ ] `ghostty-voice-ctl status` reports `downloading <pct>` and the talk-to strip shows `downloading <pct>%`, both from the single `set_state` source (never diverging)
- [ ] No `notify-send` is emitted for download progress/start/complete/failure; journald logs retained; all non-download notifies unchanged
- [ ] The whole-percent throttle is a pure, unit-tested helper (test-first)
- [ ] cargo test/clippy/fmt green; the live first-run download (network + ~3 GB model) reported honestly as demo-only

## Blocked by
TASK-13.1
<!-- SECTION:DESCRIPTION:END -->
