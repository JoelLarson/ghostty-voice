---
id: TASK-10.4
title: 'docs: talk-to verification & troubleshooting guide'
status: To Do
assignee: []
created_date: '2026-06-22 23:27'
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
- [ ] #1 README explains how to verify wrapper delivery (active sink via `status` and/or the `delivered to wrapper sink` journal line) and lists the causes of an offline / focused-window fallback
- [ ] #2 README documents trigger-time binding: the sink active when you trigger wins; never silently redirected
- [ ] #3 README documents multi-instance behavior (which wrapper wins, what happens on exit)
- [ ] #4 README documents the upgrade→restart requirement
- [ ] #5 Documentation uses the CONTEXT.md domain vocabulary
<!-- AC:END -->
