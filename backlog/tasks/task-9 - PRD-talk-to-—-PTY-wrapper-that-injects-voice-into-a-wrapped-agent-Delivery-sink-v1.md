---
id: TASK-9
title: >-
  PRD: talk-to — PTY wrapper that injects voice into a wrapped agent (Delivery
  sink, v1)
status: To Do
assignee: []
created_date: '2026-06-22 06:36'
labels:
  - needs-triage
  - prd
dependencies: []
references:
  - 'IDEAS.md #4'
  - >-
    CONTEXT.md (Delivery sink, Auto-type, Freshness window, Held-for-replay,
    Replay-last)
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Today I dictate by pressing a tactile trigger and the daemon types the **Transcript** into whatever window is *focused* via `ydotool` (the **Auto-type** path). That is agent-agnostic and works over SSH, but it has no idea *which* program receives the text — delivery targets "the focused window," not a specific agent. When I am driving a single coding agent (claude-code, often over SSH), I want my speech to go into *that agent*, deterministically, hands-free, without babysitting window focus.

## Solution

A foreground launcher, `talk-to <command>` (e.g. `talk-to ssh host claude`), that spawns the agent on a pseudo-terminal, passes its TUI through to my Ghostty window unchanged, and registers itself with the daemon as a **wrapper sink**. When I trigger a recording (existing evdev hotkey or `ghostty-voice-ctl`), the daemon records + transcribes on the desktop GPU as today, then **pushes** the finished **Transcript** to the wrapper sink, which writes it into the agent's PTY exactly as if typed — no trailing newline, so I still review and press Enter. A reserved bottom status row shows live voice state. It works over SSH with no extra machinery: injected bytes ride the existing ssh stdin pipe to the remote agent.

This generalizes delivery into a single **active Delivery sink**: `ydotool`-into-focused-window becomes the default *focused-window sink*; `talk-to` is a *wrapper sink*. Exactly one sink is active at a time; an utterance binds its target sink when triggered and is never silently redirected.

## User Stories

1. As a voice-dictation user, I want to run `talk-to claude` and have claude render normally in my terminal, so that wrapping the agent costs me nothing visually.
2. As a user driving a remote agent, I want `talk-to ssh host claude` to work the same as local, so that SSH is not a special case.
3. As a hands-free user, I want my trigger to record + transcribe and have the text land in the wrapped agent with no intervention, so that the happy path is truly hands-free.
4. As a careful user, I want injected text to arrive without a trailing Enter, so that I review before submitting.
5. As a user, I want a bottom status row showing idle/recording/transcribing, so that I can tell at a glance what the daemon is doing.
6. As a user, I want the wrapped agent to use the full screen except the reserved status row, so that nothing is clipped or corrupted.
7. As a user resizing my terminal, I want the wrapped agent to reflow correctly, so that resize never breaks rendering.
8. As a user, I want Ctrl-C and signals to reach the wrapped agent, not the wrapper, so that the agent behaves as if launched directly.
9. As a user, I want launching `talk-to` to make its wrapper sink active automatically, so that I do not run a separate activation step.
10. As a user, I want exiting `talk-to` to restore the focused-window sink, so that normal `ydotool` dictation resumes unchanged.
11. As a user with no wrapper running, I want today's focused-window Auto-type behavior unchanged, so that this feature is purely additive.
12. As a user, I want the daemon to deliver to exactly one active sink, so that text never goes two places.
13. As a user, I want a transcript bound to the sink active when I started speaking, so that switching contexts mid-utterance does not misroute it.
14. As a user, if my `talk-to` dies before delivery, I want the transcript held (cached) and recoverable via `replay-last`, never dumped into my current focus, so that a crash never types into the wrong place.
15. As a user, I want the transcript cached before delivery is attempted, so that no speech is ever lost to a failed write.
16. As a developer, I want the wrapper to inject into the agent's stdin pipe (not synthetic keystrokes), so that delivery is deterministic and needs no /dev/uinput.

## Implementation Decisions

Modules (boundaries confirmed with the developer):

New deep modules (pure, unit-tested in isolation):
- **Strip geometry** — `(rows, cols, strip_height) -> (child winsize, strip region)`. Encodes the bottom-strip invariant: child winsize is `(H-1, W)`, origin unchanged, so the child's bytes forward verbatim with no VT emulator. A top strip or side pane is explicitly rejected because it would require emulating + compositing the child.
- **Status-strip renderer** — `(State + detail) -> ANSI bytes` that paint the reserved row and restore the cursor without touching the child region. (Pure; visual-checked, not unit-tested in v1.)
- **Sink registry / active-sink** — encapsulates "exactly one active sink," lifecycle (wrapper registers -> active; deregisters or dies -> focused-window), target-binding-at-trigger, and dead-bound-sink -> Held-for-replay. Extends today's `delivery::decide`.
- **Push-sink protocol** — extends the control-socket vocabulary: parse `register-sink`, encode the pushed-Transcript frame. This is the first structured (non one-line request/response) message on the socket; decide framing (e.g. JSON) here.

Modified / thin boundary adapters (integration- or manually-tested):
- **Daemon connection handler** — add a persistent registered-sink connection path beside today's one-shot read-line/write-response exchange. After `register-sink` the connection stays open and the daemon writes Transcript frames to it; closing it (or process death) deregisters and reactivates the focused-window sink.
- **Daemon delivery (queue drain)** — deliver to the *active sink* instead of hard-calling `ydotool` type. Focused-window sink = today's `type_text` (unchanged, still gated by the Freshness window). Wrapper sink = write a Transcript frame to the registered connection; the Freshness window does not apply (the PTY target is exact).
- **PTY proxy plumbing** — forkpty, raw-mode passthrough, SIGWINCH -> recompute child winsize to `(H-1, W)`, signal forwarding. OS glue; not unit-tested.
- **`talk-to` binary** — new workspace crate wiring proxy + socket client + strip.

Contracts:
- Delivery binds the target sink at *trigger time*, not delivery time.
- Dead bound sink -> Held-for-replay; never silent redirect to the current focus.
- One active sink at all times; focused-window sink is the default/floor.

## Testing Decisions

Good tests assert external behavior, not implementation details. Prior art: inline `#[cfg(test)]` unit tests in `ghostty-voice-core` (`protocol.rs`, `delivery.rs`, `inject.rs`) and daemon integration tests in `ghostty-voiced/tests/` (`ordered_drain.rs`, `accuracy_pipeline.rs`).

Test these modules (confirmed with developer):
- **Strip geometry** (unit) — winsize/region math across terminal sizes and edge cases (tiny terminals, 1-row reserve); asserts the `(H-1, W)`/origin invariant.
- **Sink registry / active-sink** (unit) — one-active-at-a-time, register/deregister lifecycle, target-binding-at-trigger, dead-bound-sink -> held.
- **Push-sink protocol** (unit) — parse `register-sink`, round-trip encode/decode the pushed-Transcript frame; same style as existing `protocol.rs` tests.
- **Delivery routing** (integration, daemon-level) — a registered wrapper sink receives the pushed Transcript end-to-end; mirrors `ordered_drain.rs`.

Status-strip renderer is verified visually in v1 (pure presentation), not unit-tested.

## Out of Scope

Deferred — seam kept open, NOT rejected:
- **Compositor introspection** to give the focused-window sink the same strong bound-target/hold-and-ask guarantee the wrapper sink has natively. The focused-window sink stays best-effort via the Freshness window in v1. This is a deliberate deferral, not a rejection of the DE-agnostic question.
- **Explicit `ghostty-voice-ctl sink <target>`** mid-session switching (lifecycle-implicit switching is enough for v1).
- **Transcript-history surface** — list cached transcripts newest->oldest and re-route any to any sink; the generalization of `replay-last` and the "ask me where to send it" UI.
- **Read-back / dialogue capture** — the wrapper writes *in* only; it does not parse the child's output stream (that would require a VT emulator).

## Further Notes

SSH works because `talk-to` wraps whatever command it is given; `ssh host claude` is just a command, and injected bytes flow PTY master -> ssh stdin -> remote PTY -> agent stdin over the existing pipe. The strong "deliver to the original target; if it died, hold and ask" guarantee is exact for the wrapper sink (durable PTY identity) and structurally unavailable to the focused-window sink (no compositor introspection). Thinnest tracer-bullet slice for implementation: a bare PTY proxy that forwards verbatim and injects a hardcoded string on a keypress — proves PTY + transparent passthrough + SSH with zero daemon coupling — before any sink wiring. Full design lives in IDEAS.md #4; domain language in CONTEXT.md.
<!-- SECTION:DESCRIPTION:END -->
