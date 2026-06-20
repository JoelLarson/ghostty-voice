---
id: TASK-8
title: 'S8 — evdev tactile input (PTT/tap/hold), remove GNOME dependency'
status: To Do
assignee: []
created_date: '2026-06-20 20:40'
labels:
  - needs-triage
dependencies: []
references:
  - PLAN.md
  - CONTEXT.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The GNOME custom-keybinding triggers are frustrating and limiting. There are four nearly-identical `Super+modifier+D` combos (toggle/vad/continuous/cancel) that are hard to tell apart and easy to fumble — a mis-hit lands on a Ghostty or GNOME binding (e.g. splitting/closing a window). There's no clear signal of *when recording starts* or *how to stop it*. And the whole trigger path is tied to GNOME's `gsettings`, even though nothing else in the tool needs GNOME. Worst of all, GNOME keybindings fire on **press only**, so they physically cannot express the control I actually want — **push-to-talk** (hold to record) and **tap-to-latch**.

## Solution

The daemon reads input **directly via evdev** (`/dev/input`), beneath the compositor — so triggers work on **any Wayland compositor (or X)**, not just GNOME, and can distinguish **tap from hold** and see key **release**. Two configurable keys drive everything (defaults: **Start = Shift+F10**, **Stop = Shift+F9**):

- **Start tap** → begin a latched recording.
- **Start hold** → push-to-talk: record while held, stop + transcribe on release.
- **Stop tap** → stop the latched recording → transcribe.
- **Stop hold** → start a hands-free VAD recording (auto-stops on silence).

Recording begins the instant **Start goes down** (record-on-press), so push-to-talk never clips the first words; the release duration reclassifies tap (latch) vs hold (PTT). The GNOME/`gsettings` dependency — and `libcanberra` — are removed entirely; the tool's requirements become **Wayland + PipeWire**, desktop-environment-agnostic.

## User Stories

1. As a dictator, I want to hold one key and have it record only while held, so that push-to-talk feels instant and I never clip my first words.
2. As a dictator, I want a quick tap of the start key to latch recording on, so that I can talk freely and stop it deliberately.
3. As a dictator, I want a quick tap of the stop key to end a latched recording, so that stopping is unambiguous.
4. As a dictator, I want to hold the stop key to start a hands-free VAD recording, so that I can dictate without holding anything and have it auto-stop on silence.
5. As a dictator, I want recording to begin the moment I press start, so that push-to-talk captures the very first word.
6. As a dictator, I want the tap-vs-hold threshold configurable, so that I can tune it to my own timing.
7. As a user, I want my triggers to work without GNOME, so that I'm not tied to a specific desktop environment.
8. As a user, I want fewer dependencies, so that installation and maintenance are simpler (no gsettings, no libcanberra).
9. As a user, I want to choose which keys trigger start and stop, so that I can avoid keys I already use.
10. As a user, I want a setup flow that shows exactly what a key emits when I press it, so that I can tell whether it's a free key or already remapped by my mouse/keyboard software.
11. As a user, I want the setup to warn me if a chosen key is a primary key or already bound in gsettings, so that I don't pick something that conflicts.
12. As a user, I want a live "press it once" test after binding, so that I can confirm nothing else fires (the ground-truth conflict check).
13. As a user, I want to re-run the bind flow anytime, so that I can change my keys later.
14. As a security-conscious user, I want the daemon to read only my one configured device and react only to my two configured keys, never logging other input, so that the keylogger-grade capability is tightly scoped.
15. As a user, I want clear feedback when recording starts and stops, so that the tactile control is paired with an obvious signal (handled with the existing cues, now via paplay).
16. As a maintainer, I want the gesture-to-command logic to be pure and unit-tested, so that the tricky tap/hold/PTT/VAD behavior is verified without hardware.
17. As a maintainer, I want the modifier-tracking and press-duration logic to be testable from timestamped event sequences, so that timing behavior is covered without a real device.
18. As a user, I want the daemon to keep working if my input device disappears and reappears (unplug/replug), so that triggers recover.

## Implementation Decisions

- **evdev boundary**: read input via the `evdev` **Rust crate** (reads `/dev/input` directly) — this is NOT a new *system package* dependency, and it makes the tool compositor-agnostic. The user is already in the `input` group, so no new permissions.
- **Passive read, never grab**: do not `EVIOCGRAB` the device (that seizes the whole device and breaks normal typing/clicking). Consequence: a non-spare key double-fires, so the bind flow's job is to help pick a free key, not to "claim" one.
- **Pure modules** (in `ghostty-voice-core`):
  - **gesture** (already implemented): `command_for(state, event, hold_threshold) → Option<Command>`, mapping `ButtonEvent` to existing `Toggle`/`Vad` commands. The command flows through the existing `machine` transition.
  - **key-combo**: parse a combo (e.g. `Shift+F10`) and match it against an evdev keycode plus current modifier state.
  - **input key-tracker**: consume *timestamped* raw key events (the configured keys + Shift), track modifier state, compute press→release durations, and emit `ButtonEvent{Down / Up{held}}`. Pure given timestamps — the unit-testable heart of the timing logic.
  - **bind-conflict evaluator**: given a captured combo and known bindings (gsettings query results, primary-key set), return actionable warnings — same shape as `doctor::evaluate`.
- **Config**: a new `[input]` section — `start_combo`, `stop_combo`, `hold_threshold_ms` (default ~250), and a `device` selector (path, name match, or auto). Replaces the gsettings bindings.
- **Bind/setup flow** (`ghostty-voice-ctl bind`, replacing `install-hotkeys`): capture the next key event, show exactly what it emitted, warn on primary/already-remapped/gsettings-bound keys, run a live "press once" test, and write the config. Re-runnable.
- **Remove GNOME**: delete `core::hotkeys`, the `ghostty-voice-ctl install-hotkeys` subcommand, and all `gsettings` usage. Switch audio cues from `canberra-gtk-play` to `paplay` (PipeWire, already a dependency) and drop `libcanberra` from the package depends.
- Default combos **Shift+F9 / Shift+F10** chosen for low collision. No cancel/discard gesture for now.
- This **supersedes** the GNOME-hotkey trigger path.

## Testing Decisions

- A good test asserts **external behavior** with **real objects** (no mocks): feed inputs, assert the observable command/event output.
- **Unit-tested in core**: gesture (done — 9 tests); key-combo (parse + match against codes/modifiers); the input key-tracker (drive it with timestamped event sequences — Shift down, F10 down, F10 up after N ms — and assert the emitted `ButtonEvent` and the tap/hold/PTT/VAD outcomes); input config (defaults + parse); the bind-conflict evaluator (healthy vs each warning case).
- **Boundary / integration**: the evdev read loop is the only true boundary — it supplies timestamped raw events to the pure tracker; full end-to-end verification needs real input hardware (optionally a virtual `uinput` device, which is heavy). Prior art: `doctor::evaluate` (pure predicates over probe results) and the daemon's fake-server / fake-socket integration tests.

## Out of Scope

- Mouse-button binding and the interactive autodetect flow (simplified to fixed key combos this round; can revisit).
- A cancel/discard gesture (no third combo for now).
- A visual/tray "recording" indicator (feedback improvements are a separate concern; existing audio cues remain, now via paplay).
- Comprehensive global conflict detection (impossible — Linux has no unified binding registry; the live test is the backstop).
- X11-specific behavior (evdev works regardless, but X/GNOME aren't a test target).

## Further Notes

- **Security**: reading `/dev/input` is keylogger-grade capability. Mitigate by opening only the one configured device and reacting only to the two configured codes — never reading or logging anything else.
- The gesture state machine is **already implemented and committed** (red-first, 9 tests); this PRD wires it to real input, adds config + the bind flow, and removes the GNOME path.
- Refs: `PLAN.md`, `CONTEXT.md`, `docs/adr/`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Two configurable keys (default Start=Shift+F10, Stop=Shift+F9) drive triggers via evdev; works without GNOME on any Wayland compositor
- [ ] #2 Start tap latches recording; Start hold = push-to-talk (record-on-press, stop on release); Stop tap stops; Stop hold starts VAD
- [ ] #3 Recording begins on Start key-down so push-to-talk never clips the first words; hold threshold (~250ms) is configurable
- [ ] #4 Pure key-tracker turns timestamped raw key events into ButtonEvents (Shift tracking + press duration) and is unit-tested without hardware
- [ ] #5 key-combo parsing/match, input config, and a bind-conflict evaluator are unit-tested with real objects (no mocks)
- [ ] #6 ghostty-voice-ctl bind captures a key, shows what it emits, warns on primary/remapped/gsettings-bound keys, runs a live test, and writes config
- [ ] #7 Daemon opens only the one configured device and reacts only to the two configured codes; never logs other input
- [ ] #8 GNOME removed: core::hotkeys + install-hotkeys + all gsettings deleted; cues switched canberra->paplay; libcanberra dropped from PKGBUILD
- [ ] #9 README/PLAN updated: target is Wayland + PipeWire, DE-agnostic
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
gesture state machine already done (committed). Core-first TDD: key-combo parse/match; input key-tracker (timestamped events -> ButtonEvent, modifier + duration); [input] config; bind-conflict evaluator (doctor-style). Then boundary: evdev read loop feeding the tracker; ctl bind flow. Then removal: delete core::hotkeys + ctl install-hotkeys + gsettings; cue canberra->paplay; drop libcanberra. Then docs. Passive read (no EVIOCGRAB). Atomic commit per green; trunk-based on main.
<!-- SECTION:PLAN:END -->
