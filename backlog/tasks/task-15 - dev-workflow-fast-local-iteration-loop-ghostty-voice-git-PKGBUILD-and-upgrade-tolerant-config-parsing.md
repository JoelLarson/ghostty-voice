---
id: TASK-15
title: >-
  dev workflow: fast local iteration loop, ghostty-voice-git PKGBUILD, and
  upgrade-tolerant config parsing
status: Done
assignee: []
created_date: '2026-06-24 02:09'
updated_date: '2026-06-24 02:42'
labels:
  - needs-triage
dependencies: []
references:
  - packaging/PKGBUILD
  - packaging/RELEASE.md
  - crates/ghostty-voice-core/src/config.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reduce the friction of testing local changes (today: version bump → commit → delete xdg files → stop service → `makepkg -si`, which runs the AUR *release* pipeline incl. a full vendored whisper.cpp rebuild). Maintainer uses Arch + `paru` and intends to publish to the AUR later.

## What to add

1. **Fast inner loop (no packaging).** `~/.local/bin` symlinks → `target/release/*` plus a systemd user `ExecStart` override pointing at the symlinked daemon, so the loop is `cargo build --release && systemctl --user restart ghostty-voiced` with no copy, no sudo, no `/usr/bin` collision with the future package. Provide a one-time idempotent `packaging/dev-setup.sh` and a convenience runner (Makefile/justfile target).

2. **`ghostty-voice-git` PKGBUILD** that builds from the working tree with a `pkgver()` derived from `git describe` (no manual bump / tag / checksums). Doubles as the future AUR `-git` companion package. Keep the existing release PKGBUILD untouched.

3. **Upgrade-tolerant config parsing.** Today every section is `#[serde(deny_unknown_fields)]`, so a stale `~/.config/ghostty-voice/config.toml` carrying removed sections (e.g. the now-deleted `[inject]`/`[input]`) fails to parse and the daemon silently falls back to defaults — which is why a config file has to be deleted on upgrade. Change parsing to **warn-and-ignore** unknown fields (keep surfacing them so typos are still caught) instead of hard-failing.

## Notes / decisions
- Inner-loop binaries go to `~/.local/bin` (not `/usr/bin`) so they never collide with the eventual pacman-owned package; switching back to the released package = remove the symlinks + the systemd override.
- Longer-term (out of scope here, noted for later): split the vendored whisper.cpp Vulkan build into its own package so neither dev nor release rebuilds it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Config parsing stays strict: malformed TOML OR any unknown key/section (a typo, or a section left over from a previous version) is an error (deny_unknown_fields retained on every section)
- [x] #2 A present-but-invalid config is a loud, addressed failure, not silent defaults: the daemon refuses to start (aborts with the error logged) on an invalid config; a *missing* config still uses defaults
- [x] #3 `reload` rejects an invalid config — it keeps the running (last-good) config and returns the error to the client, never swapping to defaults or crashing the live daemon
- [x] #4 packaging/dev-setup.sh is idempotent: creates ~/.local/bin symlinks to target/release binaries and installs a systemd user ExecStart override for ghostty-voiced; a Makefile target runs the two-command inner loop
- [x] #5 A ghostty-voice-git PKGBUILD builds from the local repo with pkgver() from git describe (no manual version bump/tag/checksums); the existing release PKGBUILD is unchanged
- [x] #6 README/RELEASE docs note the three layers (cargo inner loop, -git package, release PKGBUILD) and the strict-config behavior; full cargo test, clippy --all-targets, fmt --check stay green
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Design pivot (maintainer decision): config must be CORRECT — fail for any reason, an addressed problem, not papered over. Reverted the warn-and-ignore/serde_ignored approach. Kept deny_unknown_fields (strict). The real bug surfaced: an invalid config was silently replaced with Config::default(). Fix: daemon aborts startup on an invalid (present) config and `reload` rejects it while keeping the running config. The 'delete xdg files on upgrade' step is therefore handled by treating a config-breaking release as an explicit fix-the-config event (loud failure tells you exactly what to remove), not by tolerating stale keys.

Dev-tooling SUPERSEDED by TASK-16: the symlink ~/.local/bin + systemd ExecStart override approach is replaced by a copy-based dev-install.sh (copy binaries to /usr/bin, no override; config drift-guard; --clean). The strict-config change and the ghostty-voice-git PKGBUILD from this task remain in force.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Two parts: strict-fail config enforcement, and a layered dev workflow.

STRICT CONFIG (the maintainer's decision — a bad config is a problem to fix, not paper over):
- Kept `deny_unknown_fields` on every section; `Config::from_toml_str` stays strict (malformed TOML or any unknown key/section is an error). Restored the `rejects_unknown_field`/`rejects_unknown_section` tests (the latter now also asserts a stale `[inject]` section is rejected).
- Fixed the real bug: the daemon was catching a parse error and silently using `Config::default()`. Now `load_config` returns `Result` — a *missing* config still yields defaults, but a *present-but-invalid* config is an error. Startup aborts with the error logged (`inspect_err` + `?`), so systemd marks the unit failed and journald shows why. `reload` rejects an invalid config (returns the error to the client) and keeps the running last-good config — never swaps to defaults, never crashes the live daemon.
- Reverted the earlier warn-and-ignore spike (dropped the `serde_ignored` dep).

DEV WORKFLOW (three layers by change cadence):
- `packaging/dev-setup.sh` (idempotent): symlinks `~/.local/bin/*` → `target/<profile>/*` and writes a systemd user `ExecStart` override (`%h/.local/bin/ghostty-voiced`), so dev builds never touch pacman-owned `/usr/bin`. Prints an undo line; warns if `~/.local/bin` isn't on PATH.
- `Makefile`: `setup`/`setup-debug`, `dev`/`dev-debug` (cargo build + `systemctl --user restart`), `check` (test/clippy/fmt). Inner loop is one command, no version bump/commit/sudo.
- `packaging/ghostty-voice-git/PKGBUILD`: VCS package building the local repo's committed HEAD via `git+file://`, `pkgver()` from `git describe` (verified → `0.1.9.r32.gaeb7772`), `provides/conflicts ghostty-voice`, whisper.cpp clone cached, completeness guard mirrored. Install hook symlinked to avoid drift. Doubles as the future AUR `-git` companion (swap `source` to the GitHub remote). The release PKGBUILD is untouched.

DOCS: RELEASE.md gains a "Local development" section (the three layers + the strict-config note); README's config section documents the strict failure mode.

VERIFY: full `cargo test` green (251), `clippy --all-targets` clean, `fmt --check` clean; `dev-setup.sh` and the PKGBUILD pass `bash -n`; Makefile uses real tabs. NOT verified: a full `makepkg` of the `-git` package end-to-end (clones + builds vendored whisper.cpp — minutes + Vulkan build deps); it is structurally complete and syntax-checked.

FOLLOW-UP (noted, out of scope): split the vendored whisper.cpp Vulkan build into its own package so neither dev nor release rebuilds it.
<!-- SECTION:FINAL_SUMMARY:END -->
