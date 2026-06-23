---
id: TASK-13.3
title: 'docs: README first-run download progress on strip + status'
status: In Progress
assignee:
  - claude
created_date: '2026-06-23 05:57'
updated_date: '2026-06-23 06:12'
labels:
  - needs-triage
  - talk-to
  - docs
dependencies:
  - TASK-13.1
  - TASK-13.2
references:
  - task-13
  - README.md
  - CONTEXT.md
parent_task_id: TASK-13
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-13 (PRD: report model download progress on the talk-to strip + status).

## What to build
Update the README first-run / model section to say download progress now shows on the talk-to **status strip** (`downloading 42%`) and in `ghostty-voice-ctl status` (`downloading <pct>`), and that `notify-send` no longer reports download progress (journald still logs it). Use CONTEXT.md domain vocabulary.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 README first-run/model section documents `downloading <pct>` on the strip and in `status`
- [ ] #2 README states `notify-send` no longer reports download progress (the journald log still does)
- [ ] #3 Wording uses the domain vocabulary (State, status strip, Delivery sink)

## Blocked by
TASK-13.1, TASK-13.2
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Docs-only. Update README in three places using CONTEXT.md vocabulary (State, status strip, Delivery sink):
1. First-run **Model** section: replace "notify-send`s progress at 10% milestones" with the `downloading <pct>` State reported live by `ghostty-voice-ctl status` (`downloading 42`) and the talk-to status strip (`downloading 42%`) from one source of truth; bare `downloading` until a total is known; progress no longer via notify-send (journald still logs start/progress/complete/failure).
2. Strip/fallback states paragraph: add `downloading <pct>%` to the daemon States the strip shows.
3. Troubleshooting "Stuck in downloading": point at the strip / status / journald instead of progress notifications.
Re-run cargo test/clippy/fmt (unchanged by docs) to confirm green.
<!-- SECTION:PLAN:END -->

<!-- AC:END -->
