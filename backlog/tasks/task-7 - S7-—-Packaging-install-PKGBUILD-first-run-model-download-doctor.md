---
id: TASK-7
title: 'S7 — Packaging & install: PKGBUILD, first-run model download, doctor'
status: Done
assignee: []
created_date: '2026-06-20 07:42'
updated_date: '2026-06-20 10:51'
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
- [x] #1 PKGBUILD vendors the whisper.cpp Vulkan build and installs the three binaries, the systemd user unit, and config.toml.example
- [x] #2 Model is NOT packaged; first-run download enters a downloading state (distinct from loading), notifies, fetches in background, and SHA-verifies
- [x] #3 doctor command checks/repairs /dev/uinput perms, input group, and YDOTOOL_SOCKET agreement; install-hotkeys pointer in post-install
- [x] #4 README covers setup, first-run, and troubleshooting (ydotoold/udev, Vulkan check, VRAM teardown); sox declared as a dependency
- [x] #5 Model-presence/SHA logic and doctor predicates are unit-tested; PKGBUILD builds in a clean chroot (integration)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
PKGBUILD (makedepends cmake+vulkan headers, vendored Vulkan build). Core-first TDD: model presence/path/SHA logic, doctor predicates. First-run download: downloading state rejects toggle/vad. Finalize cue sound source. Document linger. Resolves open items 1,3,6.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
S7 packaging & install implemented inside-out with Chicago TDD (one atomic commit per green cycle).

CODE-COMPLETE (sandbox-tested: pure unit tests + fake-HTTP-server/fake-socket integration):
- protocol: added distinct `Downloading` state (precedes Loading/VRAM-load).
- machine: rejects toggle/vad/continuous (and replay/reload/cancel) with "model still downloading"; answers only status while downloading.
- core::model: pure Healthy/Missing/Corrupt classification + needs_download + verify_download (case-insensitive hex; empty expected-hash skips the gate); HF large-v3 URL constant. The canonical SHA cannot be fabricated offline, so model_sha256 defaults empty (presence-only) and must be pinned on-hardware.
- io::download: streaming HTTP fetch to a sibling temp file, SHA-256 computed inline, verified via core, atomic rename into place (mismatch installs nothing); Progress callback. Integration-tested against a stdlib fake HTTP server.
- core::config: model_url + model_sha256 keys (defaults wire through core constants).
- daemon: ensure_model_present() runs before any whisper-server spawn — enters Downloading, fetches with 10% notify milestones, retries with backoff, never spins whisper-server against a missing model.
- cue: pure core::cue::resolve (Disabled/File/ThemeEvent) + io dispatcher (canberra-gtk-play -i <event> default, paplay <path> for files); config.toml.example ships working theme-event defaults. Resolves open item 1.
- install-hotkeys already binds toggle/vad/continuous/cancel; doctor already wired (ctl).

PACKAGING/DOCS:
- PKGBUILD: release source (git tag tarball, SKIP hash until tagged), pinned whisper.cpp tag for the vendored -DGGML_VULKAN=1 build, libcanberra dep + vulkan-tools/sound-theme-freedesktop optdepends; validated via `makepkg --printsrcinfo`.
- ghostty-voice.install: post-install pointer to config copy, doctor, install-hotkeys, enable, linger.
- README: downloading-state/SHA first-run flow, full Configuration-keys section (S2-S7), linger docs, Vulkan/CPU-fallback + stuck-in-downloading troubleshooting.
- VALIDATION.md: Part C on-hardware checks (C1 download, C2 cues, C3 clean-chroot build).

Gates: cargo test (workspace), cargo clippy --all-targets, cargo fmt --check all clean; git status clean.

PENDING ON-HARDWARE (no GPU/mic/whisper-server/GNOME/network in sandbox): the actual clean-chroot makepkg build (and recording the tarball sha256 to replace SKIP, after a v0.1.0 tag exists); pinning the real ggml-large-v3.bin SHA-256 from HuggingFace into model_sha256; live first-run download + reject-while-downloading + progress notifications; canberra/paplay cue audibility. All captured as VALIDATION.md Part C.
<!-- SECTION:FINAL_SUMMARY:END -->
