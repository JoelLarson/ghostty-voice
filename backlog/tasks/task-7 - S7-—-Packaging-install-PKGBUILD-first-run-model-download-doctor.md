---
id: TASK-7
title: 'S7 — Packaging & install: PKGBUILD, first-run model download, doctor'
status: To Do
assignee: []
created_date: '2026-06-20 07:42'
labels:
  - needs-triage
dependencies:
  - TASK-6
references:
  - PLAN.md
  - CONTEXT.md
  - docs/adr/0001-pin-whisper-to-discrete-gpu-by-pci-address.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The tool only runs from a dev checkout — there's no clean way to install it, its dependencies, the systemd unit, or the hotkeys, and the ~3 GB model has to be placed by hand. I want it installable on Arch, with the model fetched on first run and a way to diagnose the fiddly `ydotoold`/uinput setup.

## Solution

A **PKGBUILD** that vendors the `whisper.cpp` Vulkan build and installs the three binaries, the systemd user unit, and `config.toml.example`; **first-run model download** (the daemon fetches `ggml-large-v3.bin` if missing, with a `downloading` state and notify); `install-hotkeys` plus a `doctor` command for `ydotoold`/udev/socket setup; and a README covering setup, first-run, and troubleshooting.

## User Stories

1. As a developer, I want `makepkg` to build `whisper.cpp` with `-DGGML_VULKAN=1`, so that the GPU build is vendored and reproducible.
2. As a developer, I want the package to depend on `ydotool`, `pipewire`/`pw-record`, `sox`, `wl-clipboard`, `libnotify`, and the Vulkan runtime, so that a fresh install has everything it needs.
3. As a developer, I want the install to place the three binaries, the systemd **user** unit, and `config.toml.example`, so that setup is one `makepkg -si`.
4. As a developer, I want the model downloaded on first run (not in the package), with a `downloading` state that rejects `toggle`/`vad` with a notify, so that the ~3 GB isn't in the package and the daemon doesn't hang while fetching.
5. As a developer, I want the downloaded model SHA-verified, so that a corrupt fetch is caught.
6. As a developer, I want a post-install pointer to `install-hotkeys`, so that I know to run the per-user hotkey setup (which can't run from package install).
7. As a developer, I want a `doctor` command that checks/repairs `/dev/uinput` permissions, `input` group membership, and `YDOTOOL_SOCKET` agreement, so that injection setup is diagnosable.
8. As a developer, I want a README covering setup, first-run steps, and troubleshooting (ydotoold perms/udev, Vulkan check, GPU teardown to free VRAM), so that I (or another user) can get running.

## Implementation Decisions

- **PKGBUILD** is the installer (replaces install.sh/Makefile); makedepends include cmake + Vulkan headers; vendors the Vulkan build.
- **Model NOT packaged** (~3 GB); first-run download: daemon checks `model_path`, and if missing enters a `downloading` state (distinct from S2's `loading`), notifies, and fetches in the background with progress notifications.
- **`doctor` command** (core: pure predicates over env probes; boundary: the probes): `ydotoold` socket reachability, `/dev/uinput` perms + `input` group, `YDOTOOL_SOCKET` agreement.
- Cue **sound source finalized** here (ship two short sounds via `paplay`, or `canberra-gtk-play` theme sounds — `canberra-gtk-play` is present on the box).
- systemd user unit installed; `loginctl enable-linger` documented for pre-/without-graphical-login dictation.

## Testing Decisions

- **Unit (core):** model-presence + path/SHA-verification logic; `doctor` checks as pure predicates over probe results (healthy vs each broken condition).
- **Integration:** PKGBUILD builds in a clean chroot; first-run download path (small fixture / mocked fetch) flips `downloading→loading→ready` and rejects commands while downloading; `doctor` detects a broken `ydotoold`/uinput setup.

## Out of Scope

Non-Arch packaging; distributing the model binary in the package; any new dictation features (all modes are S1–S6).

## Further Notes

- Resolves the deferred open items: cue sound source (1), first-run download UX (3), `ydotoold` setup ownership (6).
- Declares the `sox` dependency that is currently **missing** on the machine.
- Refs: `PLAN.md`, `CONTEXT.md`, `docs/adr/0001`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PKGBUILD vendors the whisper.cpp Vulkan build and installs the three binaries, the systemd user unit, and config.toml.example
- [ ] #2 Model is NOT packaged; first-run download enters a downloading state (distinct from loading), notifies, fetches in background, and SHA-verifies
- [ ] #3 doctor command checks/repairs /dev/uinput perms, input group, and YDOTOOL_SOCKET agreement; install-hotkeys pointer in post-install
- [ ] #4 README covers setup, first-run, and troubleshooting (ydotoold/udev, Vulkan check, VRAM teardown); sox declared as a dependency
- [ ] #5 Model-presence/SHA logic and doctor predicates are unit-tested; PKGBUILD builds in a clean chroot (integration)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
PKGBUILD (makedepends cmake+vulkan headers, vendored Vulkan build). Core-first TDD: model presence/path/SHA logic, doctor predicates. First-run download: downloading state rejects toggle/vad. Finalize cue sound source. Document linger. Resolves open items 1,3,6.
<!-- SECTION:PLAN:END -->
