---
id: TASK-17.4
title: protocol State table — map the State enum once; drop needless allocations
status: To Do
assignee: []
created_date: '2026-06-24 04:53'
labels:
  - architecture
  - refactor
dependencies: []
parent_task_id: TASK-17
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-17 — PRD: architecture deepening — name the domain in code.

## What to build
Collapse the three hand-written matches over the `State` enum in `protocol.rs` (`encode_token`, `label`, `parse`) into one source of truth so adding a state is a single edit, and return `&'static str` where literals are returned instead of the ~20 needless `.to_owned()` calls on string-literal tokens. The wire format is unchanged — same tokens on the wire, same `downloading <pct>` two-token form, same parse results. This is the smallest, purely-internal slice; the protocol interface does not change.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The State enum's token, human label, and parse all derive from one mapping (no triple-maintained match)
- [ ] #2 Adding a hypothetical new State variant requires editing one place, not three
- [ ] #3 String-literal tokens are returned as &'static str (the needless .to_owned() calls are gone)
- [ ] #4 Wire format is byte-identical: same tokens, same `downloading <pct>` form, same parse outputs
- [ ] #5 Existing protocol tests stay green (test-first for any new mapping behaviour); cargo test --workspace green, clippy + fmt clean

## Blocked by
None — can start immediately.

## Working agreement
- **Chicago-style (classicist) TDD**, every change: red → green → refactor, test-first; assert observable behaviour through real collaborators; test doubles only at true external boundaries.
- **Tidy after every green**: once a test passes, do the small structural cleanups (rename, dedupe, extract) as a distinct step before moving on.
- **Atomic commits**: one logical change per commit, suite green at each; keep the test-first commit and the tidy commit separate where it reads cleanly.
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
