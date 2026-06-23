---
id: TASK-11
title: 'PRD: talk-to multi-wrapper correctness — deterministic active-sink lifecycle'
status: Done
assignee: []
created_date: '2026-06-22 23:25'
updated_date: '2026-06-23 04:09'
labels:
  - prd
  - talk-to
dependencies: []
references:
  - task-9
  - crates/ghostty-voice-core/src/sink.rs
  - crates/ghostty-voiced/src/main.rs
  - CONTEXT.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The v1 sink registry (task-9) tracks a set of live wrapper sinks plus a single active sink, and on deregister it falls back to the **focused-window sink** whenever the active wrapper disconnects — even if other wrapper sinks are still registered. With two `talk-to` sessions, closing the active one silently drops dictation to the `ydotool` focused-window path instead of handing off to the still-live wrapper. Observed in practice; a sharp edge of the "one wrapper, sequential switching" assumption. (Trigger-time binding itself is correct — "the sink active when you trigger wins" — this PRD is only about the lifecycle when a wrapper leaves.)

## Solution
Make the active-sink lifecycle deterministic across multiple wrappers: deregistering the active wrapper hands off to the **most-recently-registered still-live wrapper sink**, and falls back to the focused-window sink only when no wrapper remains. Trigger-time binding and Held-for-replay are unchanged (an utterance bound to a now-dead wrapper is still held, never redirected). The registry must track registration order + liveness to pick the newest survivor (today it only tracks a live set + a single active value).

## Issues (vertical slices)
- SinkRegistry newest-live handoff on deregister (pure, test-first).
- Daemon-level integration coverage of the handoff + CONTEXT.md update.

## Testing Decisions
**Chicago-style (classicist) TDD is required.** The registry change is a pure module driven test-first with no doubles (the established `sink.rs` style): multi-wrapper register/deregister ordering, the newest-live handoff, fall-through to focused-window only when empty, and the unchanged dead-bound→Held semantics. A daemon-level integration test (mirroring `held_for_replay.rs` / `ordered_drain.rs` over a real socket) covers a real wrapper disconnect handing off to another live wrapper.

## Success Validation
Successful when: with two wrappers registered, closing the active one routes subsequent dictation to the other still-live wrapper (never to the focused-window sink); the focused-window sink returns only when the last wrapper exits; an utterance already bound to a dead wrapper is still held; unit + integration tests prove this, written test-first; and CONTEXT.md describes the newest-live handoff (replacing the v1 drops-to-focused-window note).

## Out of Scope
- Explicit user-driven sink switching (`sink <target>`) — still deferred from task-9.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Deregistering the active wrapper hands off to the most-recently-registered still-live wrapper sink; the focused-window sink reactivates only when none remain
- [x] #2 Deregistering a non-active wrapper never changes which sink is active
- [x] #3 A transcript already bound to a now-dead wrapper is still Held-for-replay, never redirected (unchanged)
- [x] #4 Test-first unit tests (no doubles) + a daemon-level integration test cover the multi-wrapper handoff; cargo test green
- [x] #5 CONTEXT.md (Delivery sink) updated to describe the newest-live handoff
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
PRD complete — both issues done, Success Validation holds.

- TASK-11.1 (aa697ef): pure `SinkRegistry` newest-live handoff. `live` is now a registration-ordered `Vec`; deregistering the active wrapper hands off to the most-recently-registered survivor (`live.last()`), falling back to the focused-window sink only when none remain. Added `wrapper_count()`. Chicago-style TDD, no doubles.
- TASK-11.2 (9f6475b): daemon-level integration test `wrapper_handoff.rs` (real socket + real registry/queue/protocol) proving the handoff and the last-wrapper-exit fallback end to end; CONTEXT.md (Delivery sink) updated to the newest-live handoff.

Success Validation: with two wrappers registered, closing the active one routes subsequent dictation to the other still-live wrapper (never focused-window); the focused-window sink returns only when the last wrapper exits; an utterance bound to a now-dead wrapper is still Held (proven by the unit test even when a handoff kept another wrapper active); unit + integration tests prove this, written test-first; CONTEXT.md describes the handoff. AC #1–#5 met. The running daemon already calls `deregister` in `serve_sink`, so the fix is live with no extra wiring. Out-of-scope (explicit `sink <target>` switching) untouched. `cargo test --workspace` (254), clippy, fmt green.
<!-- SECTION:FINAL_SUMMARY:END -->
