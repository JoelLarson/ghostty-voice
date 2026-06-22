---
id: TASK-12
title: 'PRD: talk-to packaging & release reliability'
status: To Do
assignee: []
created_date: '2026-06-22 23:25'
labels:
  - prd
  - packaging
dependencies: []
references:
  - task-9
  - packaging/PKGBUILD
  - packaging/ghostty-voice.install
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Two operational/release problems surfaced shipping task-9 via the AUR:
1. After a package upgrade the running `ghostty-voiced` keeps the OLD binary in memory until restarted; a stale daemon doesn't understand `register-sink`, so `talk-to` shows `offline` and dictation silently uses the old focused-window path. (A real user hit exactly this: on-disk binary was 0.1.8 but the live daemon was from the previous boot.)
2. The 0.1.8 release nearly shipped without the new `talk-to` binary (missing from `package()`), and `sha256sums` sat at `SKIP`. Both are easy to repeat and produce a broken or unverified package.

## Solution
1. Make the upgrade→restart requirement impossible to miss: a prominent `post_upgrade` instruction to restart the user daemon (exact command), plus a best-effort restart of running per-user instances if a safe mechanism exists. (Package scriptlets run as root and cannot trivially restart a per-user systemd service, so the message is the primary fix.)
2. A repeatable release procedure (bump pkgver → push tag → `updpkgsums` → regenerate `.SRCINFO` → push AUR) and a binary-completeness guard so a release cannot silently omit a binary.

## Issues (vertical slices)
- Make the upgrade→daemon-restart requirement reliable/legible.
- Repeatable AUR release procedure + binary-completeness guard.

## Testing Decisions
**Chicago-style (classicist) TDD applies where there is logic to drive**; most of this PRD is packaging/process, verified by execution rather than unit tests. Any helper script with parsing/logic gets a test. Validation is concrete and reproducible (see below) rather than mocked.

## Success Validation
Successful when: upgrading the package makes the running daemon pick up the new binary (or the user is unmissably told to restart, with the exact command), without breaking the package transaction when no daemon is running; and the documented/scripted release flow regenerates `.SRCINFO` and real checksums, with the completeness guard rejecting a package missing any of the four binaries (ghostty-voice, ghostty-voiced, ghostty-voice-ctl, talk-to) and passing when all are present. Validated by simulating an upgrade and a dry-run release.

## Out of Scope
- CI infrastructure beyond a local script/check (unless trivially added).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `post_upgrade` unmissably instructs (and best-effort performs) the daemon restart, with the exact command; no transaction failure when no daemon is running
- [ ] #2 A repeatable release procedure is captured: bump pkgver, push tag, updpkgsums, regenerate .SRCINFO, push AUR
- [ ] #3 A guard verifies all four binaries (ghostty-voice, ghostty-voiced, ghostty-voice-ctl, talk-to) are installed and fails if any is missing
- [ ] #4 Validated by simulating a package upgrade and a dry-run release
<!-- AC:END -->
