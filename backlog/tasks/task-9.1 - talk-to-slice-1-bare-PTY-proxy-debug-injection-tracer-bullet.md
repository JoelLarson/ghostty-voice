---
id: TASK-9.1
title: 'talk-to slice 1: bare PTY proxy + debug injection (tracer bullet)'
status: To Do
assignee: []
created_date: '2026-06-22 06:45'
labels:
  - needs-triage
  - talk-to
dependencies: []
references:
  - task-9 (PRD)
  - 'IDEAS.md #4'
  - CONTEXT.md
parent_task_id: TASK-9
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (PRD: talk-to — PTY wrapper, Delivery sink v1).

## What to build

The thinnest end-to-end tracer bullet for `talk-to`: a new workspace crate/binary `talk-to <command>` that spawns the command on a pseudo-terminal, forwards bytes verbatim (raw mode), propagates terminal resize to the child winsize, and forwards signals so Ctrl-C reaches the wrapped child rather than the wrapper. On a debug keypress it injects a HARDCODED string into the child's PTY input with no trailing newline. NO daemon coupling at all — this slice proves PTY + transparent passthrough + injection + SSH in isolation.

Works for both `talk-to claude` (local) and `talk-to ssh host claude` (remote) — over SSH the injected bytes ride the existing ssh stdin pipe, no extra machinery.

Chicago-style (classicist) TDD is required: drive the testable logic test-first (red-green-refactor), assert observable behavior through real collaborators, use a test double only at the OS PTY boundary.

## Validation / success

Demoable on its own: launch claude through the wrapper, locally and over SSH, and inject the hardcoded string with the debug key.

Run `cargo test` and (manually) `talk-to claude` / `talk-to ssh host claude`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Running `talk-to claude` renders claude's TUI indistinguishably from launching claude directly.
- [ ] #2 Running `talk-to ssh host claude` behaves identically over SSH.
- [ ] #3 Resizing the terminal reflows the wrapped child correctly (child winsize tracks the terminal).
- [ ] #4 Ctrl-C and signals reach the wrapped child, not the wrapper.
- [ ] #5 The debug keypress injects a hardcoded string into the child's input line with no trailing Enter.
- [ ] #6 Chicago-style TDD: passing tests written test-first for the proxy's testable logic, no mocked collaborators beyond the OS PTY boundary; `cargo test` green.
<!-- AC:END -->
