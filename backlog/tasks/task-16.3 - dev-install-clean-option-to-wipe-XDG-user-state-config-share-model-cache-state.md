---
id: TASK-16.3
title: >-
  dev-install: --clean option to wipe XDG user state (config / share / model /
  cache / state)
status: Done
assignee:
  - Joel Larson
created_date: '2026-06-24 02:41'
updated_date: '2026-06-24 02:50'
labels:
  - needs-triage
dependencies:
  - TASK-16.1
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
- [x] #1 dev-install.sh --clean deletes ~/.config/ghostty-voice, ~/.local/share/ghostty-voice (incl. the model), ~/.cache/ghostty-voice, and ~/.local/state/ghostty-voice (honoring XDG_*_HOME overrides)
- [x] #2 It lists the paths (with sizes where cheap) and requires one explicit confirmation; declining aborts and removes nothing; --clean -y/--force skips the prompt
- [x] #3 --clean performs no build and no binary install (standalone maintenance action); a make clean-xdg target wraps it
- [x] #4 Docs note that removing the share dir forces a multi-GB model re-download on next daemon start
- [x] #5 Committed atomically; bash -n and (if available) shellcheck clean
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add `--clean` to dev-install.sh as a standalone maintenance action (no build/install). Refactor main() into arg-parse + dispatch: rename the current install path to install_flow(); main() parses `--clean` and `-y`/`--force`/`--yes`, plus `-h/--help` (usage). No args → install_flow; `--clean` → clean_xdg then exit.

clean_xdg() removes the four XDG dirs honoring overrides:
- ${XDG_CONFIG_HOME:-$HOME/.config}/ghostty-voice
- ${XDG_DATA_HOME:-$HOME/.local/share}/ghostty-voice  (incl. the ~3 GB model)
- ${XDG_CACHE_HOME:-$HOME/.cache}/ghostty-voice
- ${XDG_STATE_HOME:-$HOME/.local/state}/ghostty-voice
List only the present dirs with `du -sh` sizes + a model re-download note, require ONE confirmation (`remove all of the above? [y/N]`); decline/no-TTY aborts removing nothing; `--clean -y`/`--force` skips the prompt. Then `rm -rf` each present dir. Add a `make clean-xdg` wrapper. Document `--clean`/`make clean-xdg` + the model re-download in RELEASE.md. Verify bash -n / make -n; do not run. Commit atomically.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added `--clean` to dev-install.sh as a standalone maintenance action and a `make clean-xdg` wrapper. main() is now an arg-parsing dispatcher (no args → install_flow; `--clean` → clean_xdg; `-h/--help` → usage; unknown → exit 2).

clean_xdg() removes the four XDG dirs honoring the overrides:
- ${XDG_CONFIG_HOME:-$HOME/.config}/ghostty-voice
- ${XDG_DATA_HOME:-$HOME/.local/share}/ghostty-voice (incl. the ~3 GB model)
- ${XDG_CACHE_HOME:-$HOME/.cache}/ghostty-voice
- ${XDG_STATE_HOME:-$HOME/.local/state}/ghostty-voice

It collects only the dirs that exist, lists each with its `du -sh` size plus a model re-download note, then requires one confirmation (`remove all of the above? [y/N]`). Decline or no-TTY removes nothing (exit 1); `--clean -y`/`--force` skips the prompt. It builds and installs nothing. RELEASE.md documents the action and the re-download.

Verified: `bash -n` clean; `make -n dev`/`clean-xdg`/`check` correct; `dev-install.sh --help` (side-effect-free) renders the usage. Did NOT run the wipe. Committed atomically as 2b3f191.
<!-- SECTION:FINAL_SUMMARY:END -->
