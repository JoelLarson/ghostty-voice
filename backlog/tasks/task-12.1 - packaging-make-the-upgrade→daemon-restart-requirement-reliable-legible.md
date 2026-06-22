---
id: TASK-12.1
title: 'packaging: make the upgrade→daemon-restart requirement reliable/legible'
status: To Do
assignee: []
created_date: '2026-06-22 23:27'
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
- [ ] #1 post_upgrade prominently instructs restarting the user daemon with the exact command (systemctl --user restart ghostty-voiced) and why
- [ ] #2 A best-effort restart of running per-user instances is attempted only via a safe supported mechanism, never blocking or failing the package transaction
- [ ] #3 No error and no daemon start when none is enabled/running
- [ ] #4 Verified by simulating an upgrade: the message appears, and if a daemon is running its ExecMainStartTimestamp changes
<!-- AC:END -->
