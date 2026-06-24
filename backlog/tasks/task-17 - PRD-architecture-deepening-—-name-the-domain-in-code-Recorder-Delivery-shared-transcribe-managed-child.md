---
id: TASK-17
title: >-
  PRD: architecture deepening — name the domain in code (Recorder, Delivery,
  shared transcribe & managed-child)
status: To Do
assignee: []
created_date: '2026-06-24 04:50'
labels:
  - prd
  - architecture
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The daemon (`ghostty-voiced/src/main.rs`, ~1335 lines) has become a god-file where several concepts the domain names precisely have **no module of their own** — their state and rules are smeared across `Daemon` fields and kept coherent only by convention. A maintainer (human or AFK agent) cannot read the code in the language of CONTEXT.md; the concepts are implicit, so changes touch many sites and risk state incoherence, and the subtlest correctness (Held-for-replay, newest-live handoff, the one-mouth invariant) is only reachable through whole-daemon integration tests.

Concretely:

- **The Recorder is a domain concept with no module.** CONTEXT.md: *"the single mic-capture facility. There is only ever one (you have one mouth); its state is `idle` or `recording`."* But its state is smeared across `Daemon.current_wav`, `recorder`, `continuous`, `continuous_gen`. Nothing makes the one-mouth invariant structural — `recorder` and `continuous.recorder` being mutually exclusive is only an implicit assumption. Stopping the mic is re-implemented three times (`DiscardRecording`, `stop_and_enqueue`, `teardown`) with subtly different cleanup.
- **The transcribe step is duplicated.** `transcribe_clip` and `transcribe_with_retry` are ~127 lines of two copies of the same record→transcribe→finalize loop (build params, duration-filter, retry-until-window, `post_inference`, `finalize_transcript`). They have already drifted; a retry-policy change must be made twice or they skew further.
- **Delivery is five structures synced by hand.** `queue`, `bindings`, `sinks`, `sink_conns`, and the ad-hoc `draining` flag are kept coherent by remembering to touch them in pairs at every enqueue/resolve/register/deregister. The richest, subtlest domain rules (bound-at-trigger, Held-for-replay, newest-live handoff, serialized Auto-type) live as scattered conventions, not behind one interface.
- **`protocol.rs` hand-maps the `State` enum three times** (`encode_token`, `label`, `parse`), so adding a state means editing three matches; ~20 needless `.to_owned()` on string literals.
- **Subprocess spawning is scattered with inconsistent cleanup.** `sox` (3 variants), `pw-record`, `whisper-server`, and `paplay` are each hand-spawned with duplicated device-pinning and bespoke error context; the recorder uses SIGINT-then-wait while whisper-server uses `start_kill()`.

## Solution

A behaviour-preserving refactor that **embeds the ubiquitous language into the code**: every concept CONTEXT.md names gets the module it is missing, so the code reads in the same terms as the domain and the hard-to-write integration tests become easy unit tests.

- **Recorder** — a module owning the single mic-capture facility: `idle | recording`, the one-mouth invariant, and the active capture's output as **one sum type** (batch WAV+seq *or* continuous session) so "both at once" is unrepresentable. The three stop paths collapse into one. (Scope: capture-state only — the continuous-mode driver loop stays in the daemon for now.)
- **Shared transcribe loop** — one simple retry loop, shared by batch **Utterances** and Continuous **Clips**, owning the retry-until-window + `finalize_transcript` step, parameterised by the small differences (prompt tail, sub-min handling). Keep it a simple loop.
- **Delivery** — a module owning the **Delivery queue** + bound-target **bindings** + sink routing + drain serialization as one unit: enqueue-with-bound-target, internally-serialized drain, and Wrapper-vs-Held route resolution behind one interface. The daemon stops bookkeeping parallel maps.
- **protocol State table** — collapse the three `State` matches into one token/label mapping; return `&'static str` where literals are returned.
- **Managed child** — one seam for spawning external processes (`sox`, `pw-record`, `whisper-server`, `paplay`) with device-pinning + error context + clean SIGINT-then-wait termination, making the whisper-vs-recorder cleanup uniform. (Runtime caveat: `audio.rs` is `std::process`, the daemon is `tokio::process`; the seam likely factors as a sync core for argv/pinning/termination policy with thin sync + async adapters.)

This is **not** a behaviour change. The observable daemon behaviour — record→transcribe→type, Held-for-replay, newest-live handoff, Continuous mode, first-run download, cues, GPU pinning — must be identical before and after. ADR-0001/0002/0003 are respected and unchanged.

## Issues (vertical slices)

Each slice is independently shippable, behaviour-preserving, and green on its own. Recommended order minimises rework (managed-child underpins Recorder capture):

1. **Managed-child seam** — consolidate external-process spawning (device-pinning, error context, SIGINT-then-wait) across `sox`/`pw-record`/`whisper-server`/`paplay`.
2. **Recorder module** — extract capture-state + the one-mouth invariant as a sum type; collapse the three stop paths; sits on the managed-child seam.
3. **Shared transcribe loop** — unify `transcribe_clip` / `transcribe_with_retry` into one simple loop used by Utterances and Clips.
4. **Delivery module** — fold queue + bindings + sink routing + drain serialization behind one interface; daemon stops syncing parallel maps.
5. **protocol State table** — collapse the triple `State` match; drop needless `.to_owned()`.

## Testing Decisions

**Chicago-style (classicist) TDD is required** — test-first red-green-refactor, assert observable state/behaviour through *real* collaborators, test doubles only at true external boundaries (the deep pure modules use none). This mirrors the established `sink.rs` / `queue.rs` / `machine.rs` unit style and the real-socket integration style of `wrapper_handoff.rs` / `held_for_replay.rs` / `ordered_drain.rs`.

A good test here asserts **external behaviour, not implementation detail**: that the Recorder refuses a second concurrent recording and yields exactly one Utterance per stop; that the transcribe loop retries within the window and returns the finalized text or `None`; that Delivery enqueues with a bound target, serializes the drain, and Holds-for-replay when the bound wrapper is gone. Because the refactor is behaviour-preserving, the existing `cargo test --workspace` suite (254 tests as of TASK-11) is the regression backstop and must stay green throughout; each extracted module additionally gets focused unit tests at its new interface, written test-first.

Modules to be unit-tested (all of them): **Recorder**, the **shared transcribe loop**, **Delivery**, the **protocol State mapping**, and the **managed-child** seam (its argv/pinning/termination policy as a pure core).

## Success Validation

Successful when:

1. The daemon's observable behaviour is **unchanged** — record→transcribe→type, Held-for-replay, newest-live handoff, Continuous mode, first-run download, cues, and GPU pinning all behave exactly as before.
2. `cargo test --workspace` is green (no fewer tests than today), `clippy` clean, `fmt` clean — throughout, slice by slice.
3. The one-mouth invariant is **structural** (a second concurrent recording is unrepresentable / refused), proven by a Recorder unit test.
4. The transcribe retry-until-window + finalize behaviour is proven by a single unit test surface and shared by both Utterance and Clip paths.
5. Delivery's enqueue-with-bound-target, serialized drain, and Held-for-replay are proven at the Delivery interface (not only through the full daemon).
6. Reading the daemon and core crates, the concepts **Recorder**, **Delivery queue / bound target / Held-for-replay**, and the **transcribe** step appear as named modules whose vocabulary matches CONTEXT.md.

## Out of Scope

- Any behaviour change, new mode, or protocol change (this is a refactor).
- Moving the Continuous-mode **driver loop** into the Recorder (Recorder owns capture-state only this round).
- Inlining the thin pure modules `pty.rs` / `strip.rs` — they are shallow but pure and tested; inlining would delete a test surface (a depth downgrade). Leave them.
- The `cue` and `cache` core/io splits — already exemplary; do not touch.
- Re-litigating ADR-0001/0002/0003.

## Further Notes

This PRD came out of an adversarial architecture review aimed at reducing code and making change easier. The review explicitly affirmed what is already good (the `cue`/`cache` core/io splits, the pure `machine.rs` state machine) so it is not disturbed. The driving principle, from the user: *any concept should be understandable by reading the code because it matches the language of the domain.*
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Daemon observable behaviour is unchanged: record→transcribe→type, Held-for-replay, newest-live handoff, Continuous mode, first-run download, cues, and GPU pinning all behave as before
- [ ] #2 A Recorder module owns idle|recording with the active capture as one sum type (batch OR continuous), making a second concurrent recording unrepresentable/refused; the three mic-stop paths collapse to one
- [ ] #3 transcribe_clip and transcribe_with_retry are unified into one simple retry-until-window + finalize loop shared by Utterances and Clips
- [ ] #4 A Delivery module owns the delivery queue + bound-target bindings + sink routing + drain serialization behind one interface; the daemon no longer hand-syncs parallel maps
- [ ] #5 protocol.rs maps the State enum once (single token/label/parse source) and returns &'static str where literals are returned
- [ ] #6 Each extracted module (Recorder, shared transcribe loop, Delivery, protocol State mapping, managed-child seam) has focused Chicago-style unit tests written test-first at its interface
- [ ] #7 cargo test --workspace is green with no fewer tests than today; clippy and fmt are clean
- [ ] #8 CONTEXT.md vocabulary (Recorder, Delivery queue, bound target, Held-for-replay) is legible in the code as named modules
<!-- AC:END -->
