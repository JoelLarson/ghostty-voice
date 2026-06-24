---
id: TASK-15
title: >-
  dev workflow: fast local iteration loop, ghostty-voice-git PKGBUILD, and
  upgrade-tolerant config parsing
status: In Progress
assignee: []
created_date: '2026-06-24 02:09'
updated_date: '2026-06-24 02:09'
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
- [ ] #1 Config parsing warns-and-ignores unknown TOML fields instead of failing: a config carrying removed sections (e.g. [inject]/[input]) still loads, real fields still apply, and the ignored keys are surfaced (logged by the daemon) so typos remain visible
- [ ] #2 Config unit tests cover: an unknown section/field is ignored (not an error) and is reported in the collected-unknowns list; known fields still override defaults
- [ ] #3 packaging/dev-setup.sh is idempotent: creates ~/.local/bin symlinks to target/release binaries and installs a systemd user ExecStart override for ghostty-voiced; documents the two-command inner loop
- [ ] #4 A ghostty-voice-git PKGBUILD builds from the local working tree with pkgver() from git describe (no manual version bump/tag/checksums); the existing release PKGBUILD is unchanged
- [ ] #5 README/RELEASE docs note the three layers (cargo inner loop, -git package, release PKGBUILD); full cargo test, clippy --all-targets, fmt --check stay green
<!-- AC:END -->
