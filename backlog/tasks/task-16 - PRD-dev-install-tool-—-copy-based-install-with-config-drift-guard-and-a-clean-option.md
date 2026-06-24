---
id: TASK-16
title: >-
  PRD: dev-install tool — copy-based install with config drift-guard and a
  --clean option
status: To Do
assignee: []
created_date: '2026-06-24 02:41'
labels:
  - needs-triage
dependencies: []
references:
  - packaging/dev-setup.sh
  - packaging/RELEASE.md
  - packaging/PKGBUILD
  - dist/ghostty-voiced.service
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The fast local-iteration tooling from TASK-15 used `~/.local/bin` symlinks + a systemd `ExecStart` override. The maintainer has revised the approach: it should behave like a lightweight reinstall of the parts that change (the Rust binaries), driven by the existing packaged systemd unit, with a safety guard so it never silently clobbers installed config files, plus a one-shot way to wipe XDG state (config / share / model) — replacing the manual "delete xdg files" step.

This **supersedes the symlink + override design** in TASK-15 (the strict-config change and the `ghostty-voice-git` PKGBUILD from TASK-15 stand).

## Solution (confirmed design)

A `packaging/dev-install.sh` (replacing the symlink `dev-setup.sh`), wrapped by `make dev`:

- **Copy, not symlink.** Build the workspace, then copy the four freshly-built binaries over the installed ones.
- **Install to `/usr/bin` (decision A).** No `ExecStart` override: the packaged systemd unit's `ExecStart` is `/usr/bin/ghostty-voiced`, so that is the only path where `systemctl --user restart ghostty-voiced` runs the dev build. Copying over the pacman-owned binaries requires `sudo` (same as `makepkg -si`). The prior `~/.local/bin` symlinks + override are removed/migrated.
- **Config drift-guard (decision B).** Before copying any binary, compare the repo's *static package files* against what is installed on the machine — `config.toml.example` ↔ `/usr/share/ghostty-voice/config.toml.example`, and `dist/ghostty-voiced.service` ↔ `/usr/lib/systemd/user/ghostty-voiced.service`. If any differ, prompt to overwrite. **Decline → abort before the binaries are installed** (consistent version, or nothing). The maintainer's personal `~/.config/ghostty-voice/config.toml` is *not* compared or overwritten here — a stale personal config now fails loudly at the restart (the strict-config change from TASK-15).
- **Restart.** After a successful (possibly config-updated) install, `systemctl --user restart ghostty-voiced`.
- **`--clean` option.** Delete the XDG user state — `~/.config/ghostty-voice`, `~/.local/share/ghostty-voice` (incl. the ~3 GB model), `~/.cache/ghostty-voice`, `~/.local/state/ghostty-voice` — behind a single explicit confirmation. This is the old manual "delete xdg files" step, made a first-class, opt-in action.

## Scope (issues)

- **Issue 1 — copy-based install path**: build → drift-guard gate → copy binaries to `/usr/bin` (sudo) → restart; retire the symlink/override approach; update the Makefile + docs.
- **Issue 2 — config drift-guard**: repo-vs-installed comparison of the static package files, overwrite prompt, decline aborts before the binary install.
- **Issue 3 — `--clean`**: wipe XDG user state behind one confirmation; documents the model re-download.

## Testing / success validation

- Shell tooling, so validation is `bash -n` + `shellcheck` clean, idempotency (re-run is a no-op when nothing changed), and a documented dry run. No Rust behavior changes, so the existing `cargo test`/`clippy`/`fmt` gate must stay green.
- Success: `make dev` rebuilds, refreshes installed configs only with confirmation (declining installs nothing), copies the binaries, and restarts the daemon; `--clean` removes XDG state behind a confirmation; the strict-config change and both PKGBUILDs are untouched.

## Out of scope

- The `~/.local/bin` symlink loop and the systemd `ExecStart` override (removed).
- Touching/auto-rewriting the user's personal `~/.config/ghostty-voice/config.toml`.
- Splitting the vendored whisper.cpp into its own package (separate future work).

Refs: TASK-15 (superseded dev-setup; strict config + -git PKGBUILD retained), packaging/RELEASE.md, packaging/dev-setup.sh, packaging/PKGBUILD.
<!-- SECTION:DESCRIPTION:END -->
