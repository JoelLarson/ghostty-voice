---
id: TASK-16
title: >-
  PRD: dev-install tool — copy-based install with config drift-guard and a
  --clean option
status: To Do
assignee: []
created_date: '2026-06-24 02:41'
updated_date: '2026-06-24 02:42'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Sequence the three subtasks in order, ONE ATOMIC COMMIT EACH (16.1 → 16.2 → 16.3). 16.2 and 16.3 depend on 16.1 (the script must exist). Implementation is driven by the goal prompt below.

=== GOAL PROMPT (paste to a fresh agent session to finish TASK-16) ===

Implement TASK-16 (the dev-install tool) in the ghostty-voice repo. The design is fully decided — do not re-litigate it. Work through the three subtasks in order, ONE ATOMIC COMMIT PER SUBTASK, verifying before each commit.

Context:
- Repo: /home/chillwat/Development/JoelLarson/ghostty-voice (Rust workspace + Arch packaging). Branch: main; atomic commits straight to main are fine.
- Read the Backlog tasks for full detail via the backlog MCP (task_view): TASK-16 (PRD) and subtasks TASK-16.1, TASK-16.2, TASK-16.3.
- This SUPERSEDES the symlink/override dev tooling from TASK-15. The strict-config change and the ghostty-voice-git PKGBUILD from TASK-15 must remain untouched.

Confirmed design (decisions A & B — do not change):
- A: binaries are COPIED to /usr/bin via `sudo install -Dm755` (NOT ~/.local/bin), and there is NO systemd ExecStart override — the packaged unit (ExecStart=/usr/bin/ghostty-voiced) runs the dev build so `systemctl --user restart ghostty-voiced` works.
- B: the drift-guard compares the repo's static package files (config.toml.example, dist/ghostty-voiced.service) against their installed counterparts (/usr/share/ghostty-voice/config.toml.example, /usr/lib/systemd/user/ghostty-voiced.service). The user's personal ~/.config/ghostty-voice/config.toml is NEVER read or overwritten by the tool (only removed by --clean).

Deliverable: packaging/dev-install.sh (replacing packaging/dev-setup.sh), wired via the Makefile, plus doc updates.

Order & commits:
1) TASK-16.1 — copy install spine: `cargo build --release` → copy the 4 binaries over /usr/bin (sudo install -Dm755) → `systemctl --user restart ghostty-voiced`. Remove the symlink/override logic and migrate an old-style machine (delete the override.conf this repo wrote at ~/.config/systemd/user/ghostty-voiced.service.d/override.conf, then `systemctl --user daemon-reload`). Update the Makefile (`make dev`) and RELEASE.md/README "Local development". Commit.
2) TASK-16.2 — config drift-guard gating the binary copy: compare repo-vs-installed (cmp -s; show `diff` on mismatch), prompt `overwrite installed configs? [y/N]`; decline OR no-TTY → abort BEFORE copying binaries and before the restart; accept → sudo-overwrite the file(s) + `systemctl --user daemon-reload`, then continue to the copy. Absent installed counterpart → install it and continue. Commit.
3) TASK-16.3 — `--clean`: delete ~/.config/ghostty-voice, ~/.local/share/ghostty-voice (incl. the ~3 GB model), ~/.cache/ghostty-voice, ~/.local/state/ghostty-voice (honor XDG_CONFIG_HOME/XDG_DATA_HOME/XDG_CACHE_HOME/XDG_STATE_HOME) behind ONE confirmation listing the paths (+ `du -sh` sizes if cheap); `--clean -y`/`--force` skips the prompt; performs no build/install; add a `make clean-xdg` wrapper. Commit.

Constraints:
- bash, `set -euo pipefail`, idempotent. Each script passes `bash -n` and, if shellcheck is installed, `shellcheck`.
- A failed `cargo build` aborts before any copy (no partial install).
- Shell-only work: it should not touch the Rust gate, but run `cargo build` to confirm; if you touch any Rust, run `cargo test` + `cargo clippy --all-targets` + `cargo fmt --check`.
- For each subtask: set it In Progress, implement, check its acceptance criteria, set Done with a final summary (backlog MCP). Update the parent plan if the approach shifts.
- DO NOT run dev-install.sh against this machine — it sudo-overwrites /usr/bin and restarts the daemon. Verify by `bash -n`, shellcheck, `make -n`, and reading. The maintainer runs it.
- End every commit message with:
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>

Done when: dev-install.sh implements copy-install + drift-guard + --clean per the three subtasks; Makefile + docs updated; the symlink/override approach is removed; all three subtasks are Done in Backlog; three atomic commits on main; bash -n/shellcheck clean; the Rust gate still green.

=== END GOAL PROMPT ===
<!-- SECTION:PLAN:END -->
