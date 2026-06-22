---
id: TASK-9.2
title: 'talk-to slice 2: bottom status strip (geometry + renderer)'
status: To Do
assignee: []
created_date: '2026-06-22 06:46'
labels:
  - needs-triage
  - talk-to
dependencies:
  - TASK-9.1
references:
  - task-9 (PRD)
  - 'IDEAS.md #4'
  - CONTEXT.md
parent_task_id: TASK-9
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (PRD: talk-to — PTY wrapper, Delivery sink v1).

## What to build

Add the bottom status strip to `talk-to`. Two new pure modules plus wiring:
- **Strip geometry** (pure): terminal `(rows, cols)` + reserved strip height → child winsize `(H-1, W)` + strip region, origin unchanged. This is the load-bearing invariant that lets the child's bytes forward verbatim with NO terminal emulator — a top strip or side pane would change the origin/width and force compositing, which is explicitly out of scope.
- **Status-strip renderer** (pure): `(state + detail)` → ANSI bytes that paint the reserved bottom row and restore the cursor without ever touching the child's region.

Wire into the proxy: reserve the bottom row, set the child winsize to `(H-1, W)`, render a PLACEHOLDER state (real daemon state arrives in slice 4), and recompute correctly on resize.

Chicago-style (classicist) TDD is required: strip geometry is driven test-first with no test doubles.

## Validation / success

Demoable: claude runs in H-1 rows with a `● idle` placeholder on the bottom row; resizing keeps both correct.

## Blocked by

task-9.1 (the PTY proxy this builds on).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The wrapped child occupies H-1 rows; the reserved bottom row shows a placeholder state indicator.
- [ ] #2 The child's region and the strip never overwrite each other.
- [ ] #3 Resizing recomputes geometry; both child and strip stay correct (child winsize tracks to (H-1, W)).
- [ ] #4 Tiny/short-terminal edge cases are handled without panic or corruption.
- [ ] #5 Chicago-style TDD: unit tests written test-first (no doubles) for strip geometry across sizes and edge cases, asserting the (H-1, W)/origin invariant; `cargo test` green.
<!-- AC:END -->
