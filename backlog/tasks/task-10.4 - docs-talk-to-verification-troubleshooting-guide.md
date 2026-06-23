---
id: TASK-10.4
title: 'docs: talk-to verification & troubleshooting guide'
status: Done
assignee: []
created_date: '2026-06-22 23:27'
updated_date: '2026-06-23 04:24'
labels:
  - talk-to
  - docs
dependencies: []
references:
  - task-10
  - 'IDEAS.md #4'
  - CONTEXT.md
  - README.md
parent_task_id: TASK-10
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-10 (operability PRD).

Add a README "talk-to" section so users can self-diagnose (the questions raised during dogfooding). Should reference the new status output and strip tokens once those land, but is otherwise independent.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 README explains how to verify wrapper delivery (active sink via `status` and/or the `delivered to wrapper sink` journal line) and lists the causes of an offline / focused-window fallback
- [x] #2 README documents trigger-time binding: the sink active when you trigger wins; never silently redirected
- [x] #3 README documents multi-instance behavior (which wrapper wins, what happens on exit)
- [x] #4 README documents the upgrade→restart requirement
- [x] #5 Documentation uses the CONTEXT.md domain vocabulary
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add a dedicated "talk-to (wrapper sink)" README section using CONTEXT.md vocabulary, covering all five ACs:
1. Verify wrapper delivery: `ghostty-voice-ctl status` shows sink=wrapper wrappers=N; journald shows `delivered to wrapper sink SinkId(N)` vs `auto-typed (focused-window sink)`. List causes of an offline / focused-window fallback (no wrapper running → focused-window sink default; strip link tokens unreachable/incompatible/rejected/dropped).
2. Trigger-time binding: the sink active when you trigger wins; never silently redirected; a dead bound wrapper → Held-for-replay (replay-last), never dumped into the current focus.
3. Multi-instance: launching another talk-to makes its wrapper sink active; closing the active one hands off to the most-recently-registered still-live wrapper (newest-live handoff); focused-window sink returns only when the last wrapper exits.
4. Upgrade→restart requirement (restart ghostty-voiced after a package upgrade, else incompatible).
Reuse/consolidate the strip-token + status bits already added in 10.1–10.3. Docs-only; cargo gates stay green.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a dedicated README "talk-to — drive a single agent (wrapper sink)" section (committed bb552dd), using CONTEXT.md vocabulary throughout (Delivery sink, focused-window/wrapper sink, Auto-type, Freshness window, Held-for-replay, Replay-last, trigger-time binding, newest-live handoff).

Covers every AC: verifying wrapper delivery via `ghostty-voice-ctl status` (sink=wrapper wrappers=N) and the `delivered to wrapper sink SinkId(N)` vs `auto-typed (focused-window sink)` journal lines, plus the causes of an offline / focused-window fallback (#1); trigger-time binding — the sink active when you trigger wins, never silently redirected, dead bound wrapper → Held-for-replay recoverable via replay-last (#2); multi-instance behaviour — newest wrapper wins, closing the active one hands off to the most-recently-registered still-live wrapper, focused-window returns only when the last exits (#3); the upgrade→restart requirement with the exact command (#4); domain vocabulary throughout (#5). The earlier troubleshooting bullet now cross-references the section to avoid duplication.

Docs-only; `cargo test --workspace` (275), clippy, fmt remain green.
<!-- SECTION:FINAL_SUMMARY:END -->
