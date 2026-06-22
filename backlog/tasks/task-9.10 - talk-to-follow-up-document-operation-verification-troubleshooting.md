---
id: TASK-9.10
title: 'talk-to follow-up: document operation, verification & troubleshooting'
status: To Do
assignee: []
created_date: '2026-06-22 23:20'
labels:
  - talk-to
  - docs
dependencies: []
references:
  - task-9
  - 'IDEAS.md #4'
  - CONTEXT.md
  - README.md
parent_task_id: TASK-9
priority: medium
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (talk-to PTY wrapper, Delivery sink v1).

## Problem
Real questions during dogfooding had no documented answers: how to tell whether `talk-to` is doing PTY passthrough vs the old focused-window path; why the strip reads `offline`; and how multi-instance / trigger-time binding behaves (e.g., "the first bind seemed to win"). The behavior is correct but undocumented.

## Desired outcome
A README "talk-to" section that lets a user self-diagnose. Independent of the code follow-ups — it documents current behavior and should be updated if the related tasks land. Uses the CONTEXT.md vocabulary (Delivery sink, focused-window/wrapper sink, Auto-type, Held-for-replay).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 README explains how to verify wrapper delivery (active sink via status and/or the `delivered to wrapper sink` journal line) and lists the causes of an offline / focused-window fallback
- [ ] #2 README documents trigger-time binding: the sink active when you trigger the recording wins, and an utterance is never silently redirected
- [ ] #3 README documents multi-instance behavior (which wrapper wins, what happens on exit)
- [ ] #4 README documents the upgrade→restart requirement
- [ ] #5 Documentation uses the CONTEXT.md domain vocabulary
<!-- AC:END -->
