---
id: TASK-9.6
title: 'talk-to follow-up: hand off to the most-recent live wrapper sink on deregister'
status: To Do
assignee: []
created_date: '2026-06-22 23:20'
labels:
  - talk-to
dependencies: []
references:
  - task-9
  - crates/ghostty-voice-core/src/sink.rs
  - CONTEXT.md (Delivery sink)
parent_task_id: TASK-9
priority: high
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (talk-to PTY wrapper, Delivery sink v1).

## Problem
When the active **wrapper sink** (a running `talk-to`) disconnects, `SinkRegistry::deregister` reactivates the **focused-window sink** even when other wrapper sinks are still registered. With two `talk-to` sessions, closing the active one silently drops dictation back to the `ydotool` focused-window path instead of handing off to the still-live wrapper. This was observed in practice and is a sharp edge of the v1 "one wrapper, sequential switching" model.

## Desired outcome
Closing/killing the active wrapper hands off to the **most-recently-registered still-live wrapper sink**, and only falls back to the focused-window sink when no wrapper remains — so "newest live wrapper" is always the active sink. Trigger-time binding and Held-for-replay semantics are unchanged (an utterance bound to a now-dead wrapper is still held, never redirected).

The registry must track enough order/liveness to pick the newest survivor (today it only tracks a live set + a single active value).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Deregistering the active wrapper sink reactivates the most-recently-registered still-live wrapper sink when one exists
- [ ] #2 The focused-window sink reactivates only when no registered wrapper sink remains
- [ ] #3 Deregistering a non-active wrapper sink never changes which sink is active
- [ ] #4 A transcript already bound to a now-dead wrapper sink is still Held-for-replay, never redirected (unchanged)
- [ ] #5 Chicago-style unit tests (test-first, no doubles) cover multi-wrapper register/deregister ordering and the handoff; cargo test green
- [ ] #6 CONTEXT.md (Delivery sink) is updated to describe newest-live handoff, replacing the prior drops-to-focused-window v1 note
<!-- AC:END -->
