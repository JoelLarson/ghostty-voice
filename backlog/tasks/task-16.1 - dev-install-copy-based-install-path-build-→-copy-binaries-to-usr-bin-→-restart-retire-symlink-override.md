---
id: TASK-16.1
title: >-
  dev-install: copy-based install path (build → copy binaries to /usr/bin →
  restart); retire symlink/override
status: To Do
assignee: []
created_date: '2026-06-24 02:41'
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
- [ ] #1 packaging/dev-install.sh builds the workspace then copies the 4 binaries over /usr/bin/{ghostty-voice,ghostty-voiced,ghostty-voice-ctl,talk-to} with sudo install -Dm755, and runs `systemctl --user restart ghostty-voiced`
- [ ] #2 A failed cargo build aborts before anything is copied (no partial install); the script is set -euo pipefail and idempotent
- [ ] #3 The symlink + ExecStart-override approach is removed: the old dev-setup.sh symlink/override logic is gone and the tool cleans up a previously-written override.conf (+ daemon-reload) so an old-style machine is migrated
- [ ] #4 make dev runs the copy-based tool; stale Makefile targets (setup/setup-debug/dev-debug for the symlink model) are updated or removed accordingly
- [ ] #5 RELEASE.md and README Local-development docs describe the copy-based install (to /usr/bin, sudo, no override/symlinks); the -git and release PKGBUILDs are untouched
- [ ] #6 Committed atomically; bash -n and (if available) shellcheck are clean; the cargo test/clippy/fmt gate stays green
<!-- AC:END -->
