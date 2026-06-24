---
id: TASK-14.2
title: >-
  relocate Shift+F9/F10 triggers into talk-to; remove the daemon's global evdev
  listener
status: To Do
assignee: []
created_date: '2026-06-24 00:46'
labels:
  - needs-triage
dependencies: []
references:
  - crates/talk-to/src/main.rs
  - crates/ghostty-voiced/src/main.rs
  - crates/ghostty-voice-core/src/gesture.rs
  - crates/ghostty-voice-io/src/input.rs
parent_task_id: TASK-14
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Part of TASK-14 (talk-to as the sole interface). Moves triggering out of the daemon's system-wide evdev listener and into `talk-to`, so Shift+F9/F10 act **only while you are in the talk-to window**.

## What changes

- `talk-to` intercepts the **Shift+F10** (`ESC [ 21 ; 2 ~`) and **Shift+F9** (`ESC [ 20 ; 2 ~`) terminal escape sequences in its proxy loop — the same interception pattern it already uses for the F12 debug key — and, instead of forwarding them to the child PTY, sends a daemon command over the control socket:
  - **Shift+F10 → `toggle`** (start/stop a batch recording)
  - **Shift+F9 → `vad`** (start a hands-free VAD recording; auto-stops on silence)
  - A terminal reports key *presses* only, so there is no hold/tap timing — these are discrete commands. `cancel` stays on `ghostty-voice-ctl`.
- Commands are sent over the control socket. talk-to already keeps a persistent registered-sink connection (read-only push); sending a command uses a short-lived one-shot connection (the same one-command-then-reply path `ghostty-voice-ctl` uses), so the daemon's existing `apply_command` path is reused unchanged.
- **Delete the daemon's global evdev listener:** `spawn_input_listener`, `input_reader_loop`, `handle_button`, and the reader thread in `ghostty-voiced`.
- **Delete the now-dead tactile modules:** `ghostty-voice-io/src/input.rs` (evdev device open/read), and the pure `ghostty-voice-core` modules `input.rs` (KeyTracker), `gesture.rs`, and `key_combo.rs` — the press/release/tap/hold machinery has no terminal equivalent. Remove the `[input]` config section and the `ghostty-voice-ctl bind` flow + doctor trigger-device check that drove it.
- The escape-sequence → command decision is a **pure, unit-tested** function in `ghostty-voice-core` (feed bytes, assert the resolved command) so the matching is verified without a terminal.
- Update CONTEXT.md (triggers are in-terminal, not global; no tap/hold) and README (how to trigger from inside talk-to; `ghostty-voice-ctl` for cancel).

## Testing (classicist TDD, no mocks)

- New pure parser: feed the exact Shift+F9/F10 byte sequences and assert `vad`/`toggle`; assert unrelated bytes (including bare F9/F10 and the F12 debug key) pass through untouched.
- talk-to proxy behavior: a matched trigger is consumed (not written to the child) and dispatched; non-trigger bytes still reach the child verbatim.
- Daemon no longer opens any `/dev/input` device (the evdev path is gone); existing socket-command and wrapper-sink integration tests stay green.
- Full workspace `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check` green.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 talk-to intercepts Shift+F10 (toggle) and Shift+F9 (vad) escape sequences in its proxy loop and sends the command to the daemon over the control socket, instead of forwarding the bytes to the child
- [ ] #2 The escape-sequence → command mapping is a pure function in ghostty-voice-core, unit-tested by feeding byte sequences; unrelated bytes (including bare F9/F10 and the F12 debug key) pass through untouched
- [ ] #3 The daemon's global evdev listener is deleted (spawn_input_listener/input_reader_loop/handle_button + reader thread); the daemon opens no /dev/input device
- [ ] #4 The dead tactile modules are deleted: io/input.rs and core input.rs/gesture.rs/key_combo.rs; the [input] config section, ghostty-voice-ctl bind flow, and doctor trigger-device check are removed
- [ ] #5 Cancel remains available via ghostty-voice-ctl cancel; README documents triggering from inside talk-to and that triggers do not fire when focused elsewhere
- [ ] #6 CONTEXT.md updated: triggers are in-terminal (talk-to), press-only, no tap/hold/PTT; no system-wide input capture
- [ ] #7 New pure trigger-parser tests and updated talk-to/daemon tests pass; full cargo test, clippy --all-targets, fmt --check are green
<!-- AC:END -->
