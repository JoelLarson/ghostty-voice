---
id: TASK-16.1
title: >-
  dev-install: copy-based install path (build → copy binaries to /usr/bin →
  restart); retire symlink/override
status: Done
assignee:
  - Joel Larson
created_date: '2026-06-24 02:41'
updated_date: '2026-06-24 02:47'
labels:
  - needs-triage
dependencies: []
references:
  - packaging/dev-setup.sh
  - Makefile
  - packaging/RELEASE.md
parent_task_id: TASK-16
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Part of TASK-16. The core install path of the new `packaging/dev-install.sh` (replaces the symlink `packaging/dev-setup.sh`).

## What it does
- `cargo build --release` (the workspace).
- Copy the four built binaries over the installed ones at `/usr/bin/{ghostty-voice,ghostty-voiced,ghostty-voice-ctl,talk-to}` via `sudo install -Dm755`. (Decision A: `/usr/bin` so the packaged systemd unit's `ExecStart=/usr/bin/ghostty-voiced` runs the dev build with no override.)
- `systemctl --user restart ghostty-voiced`.
- Retire the prior approach: delete the `~/.local/bin` symlinks logic and the systemd `ExecStart` override from `dev-setup.sh`, and clean up a machine previously set up that way (remove the override.conf this repo wrote, daemon-reload).
- Update the `Makefile` (`make dev` = this tool) and the RELEASE.md/README "Local development" section to the copy model (drop the symlink/override wording).

The config drift-guard (Issue 2) and `--clean` (Issue 3) extend this script; this issue is the build → copy → restart spine plus the migration off the old approach.

## Notes
- `set -euo pipefail`; idempotent; a failed `cargo build` aborts before any copy (no partial install).
- Keep the `ghostty-voice-git` PKGBUILD and the release PKGBUILD untouched.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 packaging/dev-install.sh builds the workspace then copies the 4 binaries over /usr/bin/{ghostty-voice,ghostty-voiced,ghostty-voice-ctl,talk-to} with sudo install -Dm755, and runs `systemctl --user restart ghostty-voiced`
- [x] #2 A failed cargo build aborts before anything is copied (no partial install); the script is set -euo pipefail and idempotent
- [x] #3 The symlink + ExecStart-override approach is removed: the old dev-setup.sh symlink/override logic is gone and the tool cleans up a previously-written override.conf (+ daemon-reload) so an old-style machine is migrated
- [x] #4 make dev runs the copy-based tool; stale Makefile targets (setup/setup-debug/dev-debug for the symlink model) are updated or removed accordingly
- [x] #5 RELEASE.md and README Local-development docs describe the copy-based install (to /usr/bin, sudo, no override/symlinks); the -git and release PKGBUILDs are untouched
- [x] #6 Committed atomically; bash -n and (if available) shellcheck are clean; the cargo test/clippy/fmt gate stays green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Create packaging/dev-install.sh (replacing dev-setup.sh) with the copy-install spine:
1. `cargo build --release` (a failed build aborts before any system mutation — set -euo pipefail).
2. Migrate an old-style machine: if the override.conf this repo wrote exists at $XDG_CONFIG_HOME/systemd/user/ghostty-voiced.service.d/override.conf, remove it (+ rmdir empty dir) and `systemctl --user daemon-reload`.
3. Copy the four built binaries over /usr/bin/{ghostty-voice,ghostty-voiced,ghostty-voice-ctl,talk-to} via `sudo install -Dm755`.
4. `systemctl --user restart ghostty-voiced` (packaged unit ExecStart=/usr/bin/ghostty-voiced runs the dev build — no override/symlink).
Delete packaging/dev-setup.sh. Rewrite Makefile: `make dev` = the tool; drop setup/setup-debug/dev-debug; keep `check`. Update RELEASE.md inner-loop section + add a README Local-development note to the copy model. Leave both PKGBUILDs and the strict-config change untouched. Verify with bash -n, shellcheck, make -n, cargo build; do NOT run the script. Commit atomically.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created packaging/dev-install.sh and removed packaging/dev-setup.sh. The new tool: `cargo build --release` → migrate off the old approach (remove the override.conf at $XDG_CONFIG_HOME/systemd/user/ghostty-voiced.service.d/override.conf + `systemctl --user daemon-reload`) → copy the four binaries over /usr/bin/{ghostty-voice,ghostty-voiced,ghostty-voice-ctl,talk-to} via `sudo install -Dm755` → `systemctl --user restart ghostty-voiced`. Build runs first so a failed build aborts before any system mutation (set -euo pipefail; idempotent). The packaged unit's ExecStart=/usr/bin/ghostty-voiced runs the dev build with no override/symlink (decision A).

Makefile rewritten: `make dev` runs the tool; the symlink-model setup/setup-debug/dev-debug targets removed; `check` kept. RELEASE.md inner-loop section and a new README "Local development" note describe the copy model. Both PKGBUILDs and the strict-config change untouched.

Verified: `bash -n` clean; `make -n dev`/`make -n check` correct; `cargo build` green (Rust gate untouched). shellcheck not installed on this machine. Did NOT run the script (it sudo-overwrites /usr/bin and restarts the daemon). Committed atomically as c293160.
<!-- SECTION:FINAL_SUMMARY:END -->
