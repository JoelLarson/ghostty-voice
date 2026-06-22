---
id: TASK-9.11
title: >-
  talk-to follow-up: repeatable AUR release procedure + binary-completeness
  check
status: To Do
assignee: []
created_date: '2026-06-22 23:20'
labels:
  - packaging
  - release
dependencies: []
references:
  - task-9
  - packaging/PKGBUILD
  - packaging/ghostty-voice.install
parent_task_id: TASK-9
priority: low
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (talk-to PTY wrapper, Delivery sink v1).

## Problem
The 0.1.8 release surfaced two recurring release pitfalls: the PKGBUILD shipped without the new `talk-to` binary in package() until it was caught manually, and `sha256sums` sat at `SKIP`. Both are easy to repeat on the next release and produce a package that builds but is missing a binary, or fails verification.

## Desired outcome
A repeatable, documented release procedure plus a guard so a future release cannot silently omit a binary. Captures the full flow: bump pkgver → push the git tag → updpkgsums → regenerate .SRCINFO → push to the AUR repo.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A release doc or script captures the full flow: bump pkgver, push the git tag, updpkgsums (real sha256), regenerate .SRCINFO, push to the AUR repo
- [ ] #2 A check (script or documented makepkg step) verifies the built package installs all four binaries (ghostty-voice, ghostty-voiced, ghostty-voice-ctl, talk-to) and fails if any is missing
- [ ] #3 The procedure includes regenerating .SRCINFO for the AUR repository
<!-- AC:END -->
