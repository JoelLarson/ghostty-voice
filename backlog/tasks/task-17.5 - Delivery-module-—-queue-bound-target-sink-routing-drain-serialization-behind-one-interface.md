---
id: TASK-17.5
title: >-
  Delivery module — queue + bound target + sink routing + drain serialization
  behind one interface
status: To Do
assignee: []
created_date: '2026-06-24 04:54'
labels:
  - architecture
  - refactor
dependencies: []
parent_task_id: TASK-17
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Parent
TASK-17 — PRD: architecture deepening — name the domain in code.

## What to build
Fold the five hand-synced structures — `queue`, `bindings`, `sinks`, `sink_conns`, and the ad-hoc `draining` flag — into one **Delivery** module that owns the **Delivery queue**, the bound-target **bindings**, sink routing (Wrapper vs Held), and drain serialization. The richest domain rules live here: a transcript is bound to its target **at trigger time** (never at delivery), the drain is serialized so **Auto-type** never interleaves, and a transcript whose bound **wrapper sink** is gone is **Held-for-replay** (never silently redirected, even when a newest-live handoff kept another wrapper active). Expose enqueue-with-bound-target → seq, an internally-serialized drain, and route resolution behind one interface so the daemon stops touching parallel maps in pairs at every enqueue/resolve/register/deregister. Behaviour unchanged — this is the exact behaviour `wrapper_delivery.rs`, `held_for_replay.rs`, `ordered_drain.rs`, and `wrapper_handoff.rs` already assert, now reachable as Delivery-level unit tests too.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A Delivery module owns the delivery queue + bound-target bindings + sink routing + drain serialization behind one interface
- [ ] #2 enqueue-with-bound-target, serialized drain, and Wrapper-vs-Held route resolution are exposed as the interface; the daemon no longer hand-syncs queue/bindings/sinks/sink_conns/draining in pairs
- [ ] #3 Bound-at-trigger and Held-for-replay (including held-even-when-a-handoff-kept-another-wrapper-live) hold through the new interface
- [ ] #4 Drain stays serialized so Auto-type never interleaves (no behaviour regression vs the ad-hoc draining flag)
- [ ] #5 Chicago-style unit tests written test-first at the Delivery interface cover enqueue+bind, serialized drain order, and Held-for-replay on dead bound sink
- [ ] #6 Existing real-socket integration tests (wrapper_delivery, held_for_replay, ordered_drain, wrapper_handoff) stay green; clippy + fmt clean

## Blocked by
None — can start immediately.

## Working agreement
- **Chicago-style (classicist) TDD**, every change: red → green → refactor, test-first; assert observable behaviour through real collaborators; test doubles only at true external boundaries (this deep module uses none — drive it through its real queue/registry/protocol).
- **Tidy after every green**: once a test passes, do the small structural cleanups (rename, dedupe, extract) as a distinct step before moving on.
- **Atomic commits**: one logical change per commit, suite green at each; keep the test-first commit and the tidy commit separate where it reads cleanly.
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
