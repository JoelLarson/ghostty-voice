---
id: TASK-9.7
title: 'talk-to follow-up: make the upgrade→daemon-restart requirement reliable'
status: To Do
assignee: []
created_date: '2026-06-22 23:20'
labels:
  - talk-to
  - packaging
dependencies: []
references:
  - task-9
  - packaging/ghostty-voice.install
parent_task_id: TASK-9
priority: high
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-9 (talk-to PTY wrapper, Delivery sink v1).

## Problem
After upgrading the package, the running `ghostty-voiced` keeps the OLD binary in memory until restarted. A stale daemon does not understand `register-sink`, so `talk-to` shows `offline` and dictation silently falls back to the old focused-window path. This caused real, hard-to-diagnose confusion: the on-disk binary was new (0.1.8) but the live daemon was from the previous boot.

## Constraint
pacman/AUR install scriptlets run as **root** and cannot reliably restart a **per-user** systemd service (`systemctl --user` from root does not target user sessions without extra machinery). So the fix is primarily to make the requirement impossible to miss, plus a best-effort restart only if a safe supported mechanism exists.

## Desired outcome
A user upgrading the package is clearly told to restart the daemon (or it is restarted for them where feasible), so the new features actually take effect. Pairs with the talk-to "legible connection state" follow-up (an old daemon should also be detectable from the client side).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 post_upgrade prominently instructs restarting the user daemon with the exact command (systemctl --user restart ghostty-voiced) to pick up the new binary
- [ ] #2 If a safe, supported mechanism exists to restart running per-user instances from the root scriptlet, it is attempted best-effort and never fails or blocks the package transaction
- [ ] #3 Upgrading with no active/enabled daemon does not error and does not start the daemon
- [ ] #4 Verified by simulating an upgrade: the restart message appears, and if a running daemon is restarted its ExecMainStartTimestamp changes
<!-- AC:END -->
