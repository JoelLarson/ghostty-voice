---
id: TASK-12.2
title: 'release: repeatable AUR release procedure + binary-completeness guard'
status: Done
assignee: []
created_date: '2026-06-22 23:27'
updated_date: '2026-06-23 04:31'
labels:
  - packaging
  - release
dependencies: []
references:
  - task-12
  - packaging/PKGBUILD
parent_task_id: TASK-12
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-12 (packaging & release reliability PRD).

Capture a repeatable release procedure (bump pkgver → push tag → `updpkgsums` → regenerate `.SRCINFO` → push AUR) as a doc and/or script, plus a guard (script or documented makepkg check) that verifies the built package installs all four binaries and fails if any is missing — the 0.1.8 release nearly shipped without `talk-to` in `package()`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A release doc/script captures the full flow: bump pkgver, push tag, updpkgsums (real sha256), regenerate .SRCINFO, push AUR
- [x] #2 A guard verifies all four binaries (ghostty-voice, ghostty-voiced, ghostty-voice-ctl, talk-to) are installed in the built package and fails if any is missing
- [x] #3 .SRCINFO regeneration is part of the procedure
- [x] #4 Any script logic is covered by a test
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. `packaging/check-package-binaries.sh` (standalone, testable): given a pkgdir/install root, fail (exit 1) listing any of the four binaries (ghostty-voice, ghostty-voiced, ghostty-voice-ctl, talk-to) missing under usr/bin; exit 2 on usage error; exit 0 + summary when all present.
2. PKGBUILD package(): inline binary-completeness guard (self-contained build gate, no external-file dependency) checking $pkgdir for the four binaries; `error` + `return 1` if any missing. Cross-reference the script.
3. `packaging/RELEASE.md`: repeatable flow — bump pkgver/reset pkgrel → commit+tag+push → updpkgsums (real sha256) → makepkg -f (runs the guard) → makepkg --printsrcinfo > .SRCINFO → push AUR; plus a dry-run note and the standalone guard usage.
4. Test (cargo, shells out to bash): `release_guard.rs` — all four present → exit 0; remove one → non-zero + names it; no arg → exit 2.
5. The pre-existing PKGBUILD change (sha256sums SKIP → real hash) is part of this slice's "real checksums" story — commit it here.
6. cargo test/clippy/fmt + bash -n green.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Captured a repeatable release procedure and a binary-completeness guard. Committed as b5a0f1a.

- `packaging/RELEASE.md` (AC #1, #3): the full flow — bump pkgver / reset pkgrel → commit + tag + `git push --tags` → `updpkgsums` (real sha256, never SKIP) → `makepkg -f` (runs the guard) → `makepkg --printsrcinfo > .SRCINFO` → copy files into the AUR clone and push — plus a dry-run note and the standalone guard usage.
- Guard (AC #2): `PKGBUILD`'s `package()` now has an inline binary-completeness check (self-contained build-time gate) that `error`s + `return 1` if any of the four binaries (ghostty-voice, ghostty-voiced, ghostty-voice-ctl, talk-to) is missing from `$pkgdir`; `packaging/check-package-binaries.sh` runs the identical check standalone for dry-runs / post-install. Also flipped `sha256sums` from `SKIP` to a real hash.
- Test (AC #4): `crates/ghostty-voiced/tests/release_guard.rs` shells out to the guard script — all four present → exit 0/"OK"; talk-to missing → exit 1 naming it; all missing → lists all four; a non-executable file doesn't count; no arg → exit 2. The inline package() guard was separately simulated (complete → exit 0, missing talk-to → exit 1 naming it).

Validated by a dry-run of the guard (both the script via cargo and the inline loop via a bash simulation) and `bash -n` on all packaging files. Full `makepkg`/AUR publish is intentionally not run (no network/tag and per the no-publish constraint). AC #1–#4 met. `cargo test --workspace` (280), clippy, fmt green.
<!-- SECTION:FINAL_SUMMARY:END -->
