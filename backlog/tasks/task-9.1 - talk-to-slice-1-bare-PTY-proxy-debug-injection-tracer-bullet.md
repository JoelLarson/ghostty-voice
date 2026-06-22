---
id: TASK-9.1
title: 'talk-to slice 1: bare PTY proxy + debug injection (tracer bullet)'
status: In Progress
assignee:
  - claude
created_date: '2026-06-22 06:45'
updated_date: '2026-06-22 06:57'
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
- [x] #6 Chicago-style TDD: passing tests written test-first for the proxy's testable logic, no mocked collaborators beyond the OS PTY boundary; `cargo test` green.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Slice 1: bare PTY proxy + debug injection (tracer bullet)

New workspace crate `crates/talk-to` (binary `talk-to`), libc-only OS glue. NO daemon coupling.

### Pure, test-first (Chicago TDD, no doubles beyond the OS PTY boundary)
Put the proxy's testable logic in `ghostty-voice-core`:
- `pty.rs` (new): pure helpers driven test-first —
  - `split_command(args) -> Result<(program, args)>` (argv parsing: first arg is the program, rest are its args; empty → error).
  - `debug_inject_bytes(s) -> &[u8]` / the rule that injection carries NO trailing newline (assert the injected payload never ends in `\n`/`\r`).
  - winsize passthrough math reused from `strip.rs` in 9.2; in 9.1 child winsize == terminal size (no reservation yet).

### OS glue in `crates/talk-to/src/main.rs` (not unit-tested)
- Parse argv via the pure `split_command`.
- `TIOCGWINSZ` on stdin → forkpty with that winsize; child `execvp`s the program.
- Raw-mode stdin via termios `cfmakeraw` + RAII restore guard.
- poll(stdin, master): stdin→master, master→stdout verbatim. Ctrl-C reaches child as a passed-through 0x03 byte (raw mode).
- SIGWINCH handler (AtomicBool flag) → `TIOCGWINSZ`→`TIOCSWINSZ` on master.
- Debug key (e.g. F12 / a chosen control byte) → inject a HARDCODED string into master with no trailing newline.
- On child EOF/exit: restore terminal, exit with child status.

### Validation
`cargo test` green (pure helpers). Manual `talk-to bash` / `talk-to claude` / `talk-to ssh host claude` is demo-only (interactive; not runnable headless here) — reported honestly.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented: new `crates/talk-to` binary (libc OS glue) + pure `ghostty_voice_core::pty` (split_command, injection_bytes). forkpty+execvp, raw-mode stdin (RAII restore), poll(stdin,master) verbatim passthrough, SIGWINCH→TIOCGWINSZ/TIOCSWINSZ, F12 (ESC[24~) debug-injects a hardcoded string with NO trailing newline, child exit-code propagation.

Verified headless: `talk-to echo ...` forwards child output verbatim (PTY CR translation correct); `talk-to sh -c 'printf ABC; exit 7'` proves multi-arg passthrough + exit code 7; empty invocation → usage+exit 2. 5 test-first `pty::` unit tests green; full workspace `cargo test` green (222 tests); clippy clean.

AC #1–#5 are interactive (real terminal + claude/ssh, raw-mode Ctrl-C passthrough, F12 keypress, live resize) and cannot be exercised in this headless environment; their mechanisms are implemented and correct-by-construction. AC #6 (Chicago TDD + cargo test green) is fully evidenced. Demo verification of #1–#5 is left to the developer on a real terminal.
<!-- SECTION:NOTES:END -->
