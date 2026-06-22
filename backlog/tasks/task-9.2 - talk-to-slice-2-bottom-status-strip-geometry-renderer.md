---
id: TASK-9.2
title: 'talk-to slice 2: bottom status strip (geometry + renderer)'
status: In Progress
assignee:
  - claude
created_date: '2026-06-22 06:46'
updated_date: '2026-06-22 06:57'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Slice 2: bottom status strip (geometry + renderer)

### Pure, test-first in ghostty-voice-core (Chicago TDD, no doubles)
New `strip.rs`:
- `StripGeometry { child: Winsize{rows,cols}, strip_row: u16 }` (1-based strip_row).
- `fn geometry(term_rows, term_cols, strip_height) -> StripGeometry`: child winsize = `(rows - strip_height, cols)`, origin unchanged; strip occupies the bottom `strip_height` rows, top strip row = `rows - strip_height + 1`. Load-bearing invariant: cols unchanged (no width change), child origin still (1,1). Edge cases: term too short (rows <= strip_height) → child rows = 0 (caller keeps the child but reserves nothing usable) — never underflow/panic; rows==0/cols==0 handled.
- **Renderer** `render(state_token: &str) -> Vec<u8>`: ANSI that saves cursor (ESC 7), moves to the strip row col 1, clears line, paints `● <state>`, restores cursor (ESC 8) — never touches the child region. Per PRD the renderer is visual-checked; add ONE light smoke test (targets the right row, contains the token, restores cursor) — kept minimal.

Tests: invariant `(H-strip, W)` + origin across normal/tiny/1-row/zero sizes; strip_row math; no panic on degenerate sizes.

### Wire into talk-to (OS glue)
- `STRIP_HEIGHT = 1`. Compute child winsize via `strip::geometry` at startup and on SIGWINCH; forkpty/TIOCSWINSZ use the reduced `(H-1, W)`.
- Set DECSTBM scroll region to rows `1..H-1` so line-mode scrolling can't eat the strip (best-effort; alt-screen TUIs unaffected); reset on resize.
- Paint a PLACEHOLDER `● idle` after child output each loop iteration and on resize (real daemon state arrives in slice 4).

Validation: `cargo test` green for geometry; headless geometry demo (print computed winsizes). Live visual is demo-only.
<!-- SECTION:PLAN:END -->
