---
id: TASK-12.1
title: 'packaging: make the upgrade→daemon-restart requirement reliable/legible'
status: Done
assignee: []
created_date: '2026-06-22 23:27'
updated_date: '2026-06-23 04:27'
labels:
  - packaging
dependencies: []
references:
  - task-12
  - packaging/ghostty-voice.install
parent_task_id: TASK-12
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent: task-12 (packaging & release reliability PRD).

Update `packaging/ghostty-voice.install` `post_upgrade` to prominently instruct restarting the user daemon (exact command) so the new binary takes effect — the cause of the "offline" confusion was a stale in-memory daemon after upgrade. Attempt a safe best-effort restart of running per-user instances only if a supported mechanism exists (scriptlets run as root and can't trivially reach a per-user service). Never fail the transaction or start the daemon when it isn't enabled.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 post_upgrade prominently instructs restarting the user daemon with the exact command (systemctl --user restart ghostty-voiced) and why
- [x] #2 A best-effort restart of running per-user instances is attempted only via a safe supported mechanism, never blocking or failing the package transaction
- [x] #3 No error and no daemon start when none is enabled/running
- [ ] #4 Verified by simulating an upgrade: the message appears, and if a daemon is running its ExecMainStartTimestamp changes
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Mostly packaging/process; verified by execution (Chicago TDD only where there's logic).

Rewrite `packaging/ghostty-voice.install` `post_upgrade`:
1. Prominent message: a running ghostty-voiced keeps the OLD binary in memory until restarted; a stale daemon speaks an older protocol so talk-to shows `incompatible` and dictation falls back to focused-window. Exact remedy: `systemctl --user restart ghostty-voiced` (run as the user, not root).
2. Best-effort restart of already-running per-user instances via a SAFE supported mechanism that never starts a stopped daemon and never fails the transaction: for each user from `loginctl list-users`, `systemctl --machine=<user>@.host --user try-restart ghostty-voiced.service` (try-restart is a no-op on an inactive unit), all output/errors swallowed, `return 0`.
3. No error / no daemon start when none enabled/running (try-restart guarantees this).
Verify by sourcing the .install and calling post_upgrade in this env (no user daemon): message prints, exits 0, nothing started.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rewrote `packaging/ghostty-voice.install` `post_upgrade` (committed 3598728).

- Prints a prominent, restart-focused message with the exact command `systemctl --user restart ghostty-voiced` and *why* (a running daemon keeps the old binary in memory; a stale daemon speaks an older protocol so talk-to shows `incompatible` and dictation falls back to the focused-window path; run as the user, not root). AC #1.
- Best-effort restart of already-running per-user instances via a safe supported mechanism: for each user from `loginctl list-users`, `systemctl --machine=<user>@.host --user try-restart ghostty-voiced.service`, with all output/errors swallowed and `return 0`. `try-restart` acts only on an already-active unit, so a stopped/disabled daemon is never started, and nothing can fail the package transaction. AC #2, AC #3.

Verified by simulating the scriptlet (`source packaging/ghostty-voice.install; post_upgrade`) in this environment with no user daemon running: the message prints, exit code is 0, and nothing is started; `bash -n` clean and `post_install` still defined.

AC #1–#3 met. AC #4's "if a daemon is running its ExecMainStartTimestamp changes" is **demo-only here** — it needs a live per-user `ghostty-voiced` under a user systemd session (which would also spawn whisper-server / need a GPU), unavailable in this headless environment. The try-restart path is correct by construction (it restarts an active unit and no-ops otherwise) and the no-daemon path is verified. Recommended on-hardware check: with the daemon active, upgrade (or run `post_upgrade`) and confirm `systemctl --user show -p ExecMainStartTimestamp ghostty-voiced` changes.
<!-- SECTION:FINAL_SUMMARY:END -->
