---
id: TASK-12.2
title: 'release: repeatable AUR release procedure + binary-completeness guard'
status: To Do
assignee: []
created_date: '2026-06-22 23:27'
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
- [ ] #1 A release doc/script captures the full flow: bump pkgver, push tag, updpkgsums (real sha256), regenerate .SRCINFO, push AUR
- [ ] #2 A guard verifies all four binaries (ghostty-voice, ghostty-voiced, ghostty-voice-ctl, talk-to) are installed in the built package and fails if any is missing
- [ ] #3 .SRCINFO regeneration is part of the procedure
- [ ] #4 Any script logic is covered by a test
<!-- AC:END -->
