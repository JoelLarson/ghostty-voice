---
id: TASK-13.3
title: 'docs: README first-run download progress on strip + status'
status: To Do
assignee: []
created_date: '2026-06-23 05:57'
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

## Acceptance criteria
- [ ] README first-run/model section documents `downloading <pct>` on the strip and in `status`
- [ ] README states `notify-send` no longer reports download progress (the journald log still does)
- [ ] Wording uses the domain vocabulary (State, status strip, Delivery sink)

## Blocked by
TASK-13.1, TASK-13.2
<!-- SECTION:DESCRIPTION:END -->
