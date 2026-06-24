---
id: TASK-16.2
title: >-
  dev-install: config drift-guard — compare repo vs installed, prompt to
  overwrite, decline aborts before binaries
status: To Do
assignee: []
created_date: '2026-06-24 02:41'
updated_date: '2026-06-24 02:42'
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
- [ ] #1 Before any binary copy, the tool compares config.toml.example ↔ /usr/share/ghostty-voice/config.toml.example and dist/ghostty-voiced.service ↔ /usr/lib/systemd/user/ghostty-voiced.service for exact content equality
- [ ] #2 On any difference it lists the file(s), shows a diff, and prompts `overwrite installed configs? [y/N]`
- [ ] #3 Declining (or no TTY) aborts the run BEFORE the binaries are installed and before the restart — nothing is copied or restarted
- [ ] #4 Accepting overwrites the installed file(s) via sudo and runs `systemctl --user daemon-reload`, then proceeds to the binary install
- [ ] #5 No difference (or an absent installed counterpart) installs the missing file and proceeds; the personal ~/.config/ghostty-voice/config.toml is never read or overwritten
- [ ] #6 Committed atomically; bash -n and (if available) shellcheck clean; documented in the Local-development docs
<!-- AC:END -->
