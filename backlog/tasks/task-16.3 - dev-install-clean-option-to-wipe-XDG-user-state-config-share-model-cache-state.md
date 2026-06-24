---
id: TASK-16.3
title: >-
  dev-install: --clean option to wipe XDG user state (config / share / model /
  cache / state)
status: To Do
assignee: []
created_date: '2026-06-24 02:41'
labels:
  - needs-triage
dependencies: []
references:
  - packaging/dev-setup.sh
parent_task_id: TASK-16
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Part of TASK-16. The opt-in maintenance action that replaces the old manual "delete xdg files" step (depends on Issue 1's script).

## What it does
- `dev-install.sh --clean` (and a `make clean-xdg` wrapper) deletes the XDG user state:
  - `~/.config/ghostty-voice` (personal config)
  - `~/.local/share/ghostty-voice` (incl. the ~3 GB **model**)
  - `~/.cache/ghostty-voice` (recordings/transcripts corpus)
  - `~/.local/state/ghostty-voice` (logs)
- Lists exactly what will be removed (paths, and sizes via `du -sh` if cheap) and requires **one** explicit confirmation; aborts if declined. `--clean -y` / `--force` skips the prompt for scripted use.
- `--clean` is standalone: it does **not** build or install binaries.

## Notes
- Document clearly that removing `~/.local/share/ghostty-voice` forces a multi-GB model re-download on the next daemon start.
- Use the XDG base-dir env vars when set (`XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_CACHE_HOME`/`XDG_STATE_HOME`), falling back to the `~/.config` etc. defaults.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 dev-install.sh --clean deletes ~/.config/ghostty-voice, ~/.local/share/ghostty-voice (incl. the model), ~/.cache/ghostty-voice, and ~/.local/state/ghostty-voice (honoring XDG_*_HOME overrides)
- [ ] #2 It lists the paths (with sizes where cheap) and requires one explicit confirmation; declining aborts and removes nothing; --clean -y/--force skips the prompt
- [ ] #3 --clean performs no build and no binary install (standalone maintenance action); a make clean-xdg target wraps it
- [ ] #4 Docs note that removing the share dir forces a multi-GB model re-download on next daemon start
- [ ] #5 Committed atomically; bash -n and (if available) shellcheck clean
<!-- AC:END -->
