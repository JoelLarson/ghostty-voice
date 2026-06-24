---
id: TASK-14
title: >-
  PRD: talk-to as the sole interface — remove global hotkeys & the
  focused-window sink
status: To Do
assignee: []
created_date: '2026-06-24 00:45'
labels:
  - needs-triage
dependencies: []
references:
  - CONTEXT.md
  - IDEAS.md
  - docs/adr/
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

ghostty-voice still carries two "global"/desktop-integration paths that predate `talk-to`:

1. **A system-wide trigger listener.** The daemon opens a raw `/dev/input` evdev keyboard and reacts to Shift+F9/F10 *everywhere*, regardless of which window is focused. This is keylogger-grade capability scoped to one device, but it fires no matter where you are.
2. **A focused-window delivery sink.** Finished transcripts are typed into *whatever window is focused* via `ydotool`, guarded by a time-based Freshness window because the daemon has no window identity — the documented "wrong-window" risk.

Both exist to integrate with the desktop (GNOME/Wayland) compositor. But `talk-to` is now the primary interface: it wraps the agent on a PTY, registers as a wrapper sink, and receives transcripts pushed straight into the agent's pipe — a known target with no wrong-window risk. The global paths are now redundant surface area and risk: triggers fire when you are not even in a talk-to session, and transcripts can mistype into the wrong window.

## Solution

Make `talk-to` the **sole** interface for both triggering and delivery.

- **Triggers move into talk-to.** `talk-to` intercepts the Shift+F9 / Shift+F10 terminal escape sequences in its proxy loop (as it already intercepts the F12 debug key) and sends commands to the daemon over the control socket. Triggers therefore fire **only while you are in the talk-to window**. A terminal delivers key *presses* only (no release/hold timing), so the tap/hold/PTT/VAD gesture model collapses to discrete commands: **Shift+F10 = toggle (start/stop batch recording)**, **Shift+F9 = start hands-free VAD**. Cancel stays available via `ghostty-voice-ctl cancel`.
- **The daemon's global evdev listener is removed entirely** — no more system-wide keyboard reading. The pure tactile timing/gesture/key-combo modules and the `[input]` config section go with it.
- **The focused-window sink is removed entirely.** No `ydotool`, no Freshness window, no "wrong-window" risk. The **wrapper sink (talk-to) is the only Delivery sink.** With no wrapper registered, there is nothing to type into: a triggered utterance whose bound wrapper is gone is Held-for-replay, and `replay-last` re-routes to the active wrapper sink (never to a focused window).

The tool's tactile capability and its desktop-typing capability are both retired; the only way text reaches an agent is through a talk-to PTY you are actively using.

## Scope structure

Two reviewable slices (subtasks), each independently testable, plus the domain-doc/ADR updates that record the shift:

- **Slice A — remove the focused-window/ydotool sink** (delivery layer): the wrapper sink becomes the only sink.
- **Slice B — relocate triggers into talk-to** (input layer): in-terminal Shift+F9/F10 → socket commands; delete the daemon's global evdev listener.

An ADR records the architecture decision (talk-to is the sole interface; no global input capture; no focused-window typing); CONTEXT.md is updated to match the new domain model (sinks, freshness window, triggers).

## Testing / Success validation

- Classicist (Chicago-school) TDD: assert observable behavior with real objects, no mocks. New pure logic (talk-to escape-sequence → command mapping) is unit-tested by feeding byte sequences and asserting the resolved command.
- Daemon integration tests (fake socket/server) updated to the wrapper-only sink model — the `status` report no longer advertises a focused-window sink; a command with no wrapper registered holds rather than types.
- Success is validated when: `talk-to` triggers recording from inside its window and nothing fires when focused elsewhere; transcripts are delivered only to the wrapper PTY; the daemon no longer opens any `/dev/input` device and no longer calls `ydotool`; full workspace `cargo test`, `cargo clippy --all-targets`, and `cargo fmt --check` are green.

## Out of Scope

- Re-introducing any system-wide trigger or desktop typing.
- Recovering tap/hold/PTT semantics inside the terminal (e.g. via the Kitty keyboard protocol's key-release reports) — discrete press-only commands this round.
- New trigger keys beyond the two existing combos.

Refs: CONTEXT.md, IDEAS.md (#4 talk-to), TASK-8 (the evdev path being reversed), docs/adr/.
<!-- SECTION:DESCRIPTION:END -->
