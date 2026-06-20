---
id: TASK-2
title: 'S2 — Real toggle tool: daemon + whisper-server supervision'
status: In Progress
assignee: []
created_date: '2026-06-20 07:42'
updated_date: '2026-06-20 08:29'
labels:
  - needs-triage
dependencies:
  - TASK-1
references:
  - PLAN.md
  - CONTEXT.md
  - docs/adr/0001-pin-whisper-to-discrete-gpu-by-pci-address.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

S1 works, but I have to start `whisper-server` by hand and run a CLI command for every utterance — there's no hotkey, the warm model isn't managed for me, and the 16 GB of VRAM stays allocated until I manually kill the server. I want a background service I start once, toggle with a hotkey, that owns the model's whole lifecycle: start it warm, keep it alive, restart it if it dies, and free the VRAM when I stop.

## Solution

A systemd **user** daemon (`ghostty-voiced`) that supervises `whisper-server` as a child process and listens on a Unix socket, plus a thin client (`ghostty-voice-ctl`) spawned by GNOME hotkeys. `Super+D` toggles recording; the daemon owns recording state (no lockfile, no cross-process race). The daemon eager-starts the model on boot, restarts it with backoff on death, and tears it down — freeing VRAM — when the daemon stops (`systemctl --user stop ghostty-voiced`).

## User Stories

1. As a developer, I want to `systemctl --user enable --now ghostty-voiced` once, so that dictation is always available without manual server juggling.
2. As a developer, I want the daemon to eager-start `whisper-server` warm on boot, so that the first utterance pays no model-load latency.
3. As a developer, I want the daemon to reject commands until the model is loaded **on the right GPU**, so that I never dictate into a not-yet-ready or wrong-device server.
4. As a developer, I want a distinct `loading`/`starting` state separate from `idle`, so that "model still coming up" is observable and not confused with ready.
5. As a developer, I want `Super+D` to toggle recording start/stop, so that I control utterance boundaries hands-on.
6. As a developer, I want `cancel` and `status` commands, so that I can abort a recording and inspect daemon state.
7. As a developer, I want `reload` to re-read non-model config, so that I can adjust settings without restarting the model.
8. As a developer, I want the daemon to restart `whisper-server` with backoff on crash and `notify-send` on failure, so that a GPU hiccup self-heals.
9. As a developer, I want stopping the daemon to cascade-kill `whisper-server`, so that the 16 GB VRAM is freed — the "disable it" path.
10. As a developer, I want the daemon to health-check the `ydotoold` socket at startup and notify clearly if it's unreachable, so that injection failures are diagnosed up front.
11. As a developer, I want `whisper-server`'s binary path, launch args, and `vulkan_device` to be config values, so that an upstream rename/flag change is a config edit, not a rebuild.
12. As a developer, I want an `install-hotkeys` helper that binds `toggle`/`cancel` via gsettings, so that I don't hand-edit GNOME keybinding schemas.

## Implementation Decisions

- Split S1's skeleton into two binaries — `ghostty-voiced` (supervising daemon) and `ghostty-voice-ctl` (thin client) — atop `ghostty-voice-core`.
- **Supervisor module** (core, pure): child-lifecycle state machine (`starting`/`loading`/`ready`/`failed`), backoff schedule, restart policy — testable without a real process. Boundary adapter: process spawn/kill + signal handling.
- Readiness gate reuses S1's **load-name assertion** (resolved via the `vulkan_device` PCI address). This is where the `loading` vs `ready` distinction lands.
- Unix socket at `$XDG_RUNTIME_DIR/ghostty-voice.sock`; newline-delimited wire protocol: request = one command word (`toggle|cancel|status|reload`), response = `ok <state>` or `err <message>`.
- **State machine in core**: `idle`/`recording`/`transcribing`, single-utterance for S2 (`toggle` during `transcribing` → soft cue). The Recorder + ordered delivery queue is **S3**; S2 still types directly like S1.
- `ydotoold` is health-checked at startup only, **not supervised** (independent privileged service, hard package dependency).
- systemd user unit: `ExecStart=ghostty-voiced`, `Restart=on-failure`, `RestartSec=2`, `WantedBy=default.target`; document `loginctl enable-linger`.

## Testing Decisions

- A good test asserts external behavior with real objects; mock only the true boundaries (child process, socket).
- **Unit (core, real objects):** supervisor state machine (`starting→loading→ready→crash→backoff→restart`, teardown), backoff schedule, wire-protocol parse/format (all commands + `ok`/`err` responses), command→state-transition logic.
- **Integration:** daemon drives a **fake `whisper-server`** (stub HTTP) to exercise readiness gating and restart without a GPU; real socket round-trip via `ghostty-voice-ctl`; `ydotoold`-down health-check path; one real end-to-end toggle→type smoke test against a real warm server.

## Out of Scope

Cache-before-type, Recorder + ordered delivery queue, `replay-last`, freshness window, audio cues (S3); accuracy stack (S4); VAD (S5); Continuous mode (S6); PKGBUILD, first-run model download, `doctor` (S7). S2 types directly, single utterance at a time.

## Further Notes

- The `loading` state (distinct from S7's `downloading`) is introduced here — closes the startup-state-gap flagged during S1.
- `ydotoold` deliberately kept out of supervision/restart logic.
- Refs: `PLAN.md`, `CONTEXT.md`, `docs/adr/0001`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ghostty-voiced runs as a systemd user service and supervises whisper-server as a child
- [ ] #2 Model eager-starts warm on boot; daemon rejects commands until loaded on the correct GPU (load-name assertion); a distinct loading state exists
- [ ] #3 Super+D toggles recording via ghostty-voice-ctl over the Unix socket; cancel/status/reload work
- [ ] #4 whisper-server restarts with backoff on crash (notify on failure); stopping the daemon cascades-kills it and frees VRAM
- [ ] #5 ydotoold socket is health-checked at startup with a clear notify if unreachable; ydotoold is NOT supervised
- [ ] #6 Supervisor state machine and wire protocol are unit-tested with real objects; readiness/restart tested against a fake whisper-server
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Split into ghostty-voiced + ghostty-voice-ctl atop core. Core-first TDD: supervisor state machine + backoff, wire-protocol parse/format, command->state transitions. Then boundary adapters: process spawn/kill+signals, Unix socket, ydotoold health-check. Integration via a stub whisper-server. Single-utterance (queue is S3).
<!-- SECTION:PLAN:END -->
