---
id: TASK-16.2
title: >-
  dev-install: config drift-guard — compare repo vs installed, prompt to
  overwrite, decline aborts before binaries
status: Done
assignee:
  - Joel Larson
created_date: '2026-06-24 02:41'
updated_date: '2026-06-24 02:48'
labels:
  - needs-triage
dependencies:
  - TASK-16.1
references:
  - config.toml.example
  - dist/ghostty-voiced.service
parent_task_id: TASK-16
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Part of TASK-16. The safety gate that runs in `dev-install.sh` *before* any binary is copied (depends on Issue 1's script).

## What it does
Compare the repo's static package files against what is installed on the machine:
- `config.toml.example` ↔ `/usr/share/ghostty-voice/config.toml.example`
- `dist/ghostty-voiced.service` ↔ `/usr/lib/systemd/user/ghostty-voiced.service`

- If any differ: print which file(s) differ and a diff, then prompt `overwrite installed configs? [y/N]`.
  - **Decline (or no TTY available)** → abort the whole run **before** the binaries are installed and **before** the restart (consistent version, or nothing changes).
  - **Accept** → overwrite the installed file(s) with `sudo install`, `systemctl --user daemon-reload` (the unit may have changed), then continue to the binary install (Issue 1).
- If none differ, or an installed counterpart is absent → install the missing file and continue.

## Notes / boundaries
- The maintainer's personal `~/.config/ghostty-voice/config.toml` is **never** read or overwritten here (a stale one fails loudly at the restart — the strict-config change from TASK-15). Only the static package files are compared.
- Comparison is exact content (e.g. `cmp -s`); the prompt shows a `diff` for context.
- The prompt is the only interaction; in a non-interactive run a difference is a hard abort, never a silent overwrite.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Before any binary copy, the tool compares config.toml.example ↔ /usr/share/ghostty-voice/config.toml.example and dist/ghostty-voiced.service ↔ /usr/lib/systemd/user/ghostty-voiced.service for exact content equality
- [x] #2 On any difference it lists the file(s), shows a diff, and prompts `overwrite installed configs? [y/N]`
- [x] #3 Declining (or no TTY) aborts the run BEFORE the binaries are installed and before the restart — nothing is copied or restarted
- [x] #4 Accepting overwrites the installed file(s) via sudo and runs `systemctl --user daemon-reload`, then proceeds to the binary install
- [x] #5 No difference (or an absent installed counterpart) installs the missing file and proceeds; the personal ~/.config/ghostty-voice/config.toml is never read or overwritten
- [x] #6 Committed atomically; bash -n and (if available) shellcheck clean; documented in the Local-development docs
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add a drift_guard() to dev-install.sh, called in main() after build and before migrate/install/restart (so a decline aborts before any binary copy or restart). For each static package pair:
- config.toml.example ↔ /usr/share/ghostty-voice/config.toml.example
- dist/ghostty-voiced.service ↔ /usr/lib/systemd/user/ghostty-voiced.service
Compare exact content with `cmp -s`. Absent installed counterpart → `sudo install -Dm644` it and continue (nothing to clobber). Present + differ → collect. After the loop, if any differ: print the file(s) + `diff`, then if stdin is not a TTY abort (exit 1, nothing installed/restarted); else prompt `overwrite installed configs? [y/N]` — decline aborts, accept overwrites the differing file(s) via `sudo install -Dm644` + `systemctl --user daemon-reload`, then control returns to main which proceeds to the binary copy. The personal ~/.config/ghostty-voice/config.toml is never read or written. Document the guard in RELEASE.md's inner-loop section. Verify bash -n / make -n / cargo build; do not run the script. Commit atomically.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added drift_guard() to dev-install.sh, called in main() after build() and before migrate/install/restart. It compares the repo's static package files against their installed counterparts with exact-content `cmp -s`: config.toml.example ↔ /usr/share/ghostty-voice/config.toml.example and dist/ghostty-voiced.service ↔ /usr/lib/systemd/user/ghostty-voiced.service.

- Absent installed counterpart → `sudo install -Dm644` it, continue.
- Difference → collected; after the loop, prints a labeled `diff` per file, then prompts `overwrite installed configs? [y/N]`.
  - No TTY (`! [ -t 0 ]`) → abort (exit 1) before any binary copy/restart.
  - Decline → abort (exit 1) before any binary copy/restart.
  - Accept → `sudo install -Dm644` the differing file(s) + `systemctl --user daemon-reload`, then main() proceeds to install_binaries + restart.
- No differences → returns, run proceeds.

The personal ~/.config/ghostty-voice/config.toml is never referenced. Because drift_guard runs before install_binaries and restart_daemon, a decline/no-TTY leaves the machine as a consistent version or untouched. RELEASE.md's inner-loop section documents the guard.

Verified: `bash -n` clean; `make -n dev` correct; empty-array expansions guarded by an early `return 0`. shellcheck not installed on this machine. Did NOT run the script. Committed atomically as 7ad42db.
<!-- SECTION:FINAL_SUMMARY:END -->
