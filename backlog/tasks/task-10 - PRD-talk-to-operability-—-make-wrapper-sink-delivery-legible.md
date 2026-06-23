---
id: TASK-10
title: 'PRD: talk-to operability — make wrapper-sink delivery legible'
status: In Progress
assignee: []
created_date: '2026-06-22 23:25'
updated_date: '2026-06-23 04:10'
labels:
  - prd
  - talk-to
dependencies: []
references:
  - task-9
  - 'IDEAS.md #4'
  - CONTEXT.md
  - crates/ghostty-voice-core/src/protocol.rs
  - crates/ghostty-voice-ctl/src/main.rs
  - crates/talk-to/src/main.rs
  - README.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
With the talk-to **wrapper sink** shipped (task-9), it is hard to tell what delivery is doing. `talk-to`'s status strip collapses three very different conditions into one `offline` token: (a) no daemon reachable, (b) daemon reachable but registration rejected by an incompatible/old daemon, (c) a previously-good connection dropped. And there is no way to ask the daemon which **Delivery sink** is active — confirming dictation routes to a wrapper sink vs the **focused-window sink** currently requires tailing journald for `delivered to wrapper sink SinkId(N)` vs `auto-typed (focused-window sink)`. These gaps caused slow, real diagnosis during dogfooding (a stale daemon looked identical to "no daemon").

## Solution
Make the delivery path observable, end to end:
1. `ghostty-voice-ctl status` reports the active Delivery sink and the count of registered wrapper sinks.
2. `talk-to` shows distinct, accurate connection-state tokens and logs the reason on failure.
3. A protocol version/handshake lets the client detect and clearly report an incompatible daemon instead of silently showing `offline`.
4. A README verification/troubleshooting section so users self-diagnose.
All additive to the deliberately-dumb newline line protocol — no JSON unless a field needs it.

## Issues (vertical slices)
- Report the active Delivery sink + registered wrapper count in `status`.
- Distinguish talk-to connection states on the strip (+ client logging).
- Protocol version handshake → detect/report an incompatible daemon.
- Docs: verification & troubleshooting guide.

## Testing Decisions
**Chicago-style (classicist) TDD is required**, consistent with task-9 and the existing core tests. Drive protocol parse/encode and any pure decision logic test-first with real collaborators and no doubles (doubles only at true boundaries — the socket peer, the OS). Observability additions get unit tests for encode/parse plus a daemon-level integration test mirroring `ghostty-voiced/tests/ordered_drain.rs` where applicable. The status-strip presentation stays visually verified (pure presentation), as in task-9.

## Success Validation
Successful when: `ghostty-voice-ctl status` shows whether a wrapper sink or the focused-window sink is active (and how many wrappers are registered) without reading journald; `talk-to` shows different tokens for unreachable vs incompatible vs dropped and logs why; connecting to an older daemon yields an explicit "incompatible" indication rather than `offline`; the README lets a user verify wrapper delivery and explain an offline/focused-window fallback unaided; and all new unit + integration tests pass, written test-first.

## Out of Scope
- JSON/structured framing (keep the line protocol until a field needs it).
- The transcript-history surface and explicit `sink <target>` switching (still deferred from task-9).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `ghostty-voice-ctl status` reports the active Delivery sink and the registered wrapper-sink count
- [ ] #2 `talk-to` distinguishes unreachable / incompatible / dropped connection states and logs the reason
- [ ] #3 An incompatible (older) daemon is reported explicitly, never as a generic `offline`
- [ ] #4 README has a verification & troubleshooting section using the CONTEXT.md vocabulary
- [ ] #5 All work is test-first (Chicago-style) with unit + integration coverage as applicable; cargo test green
<!-- AC:END -->
