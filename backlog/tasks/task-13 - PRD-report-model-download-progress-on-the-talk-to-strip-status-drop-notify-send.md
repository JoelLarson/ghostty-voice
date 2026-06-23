---
id: TASK-13
title: >-
  PRD: report model download progress on the talk-to strip + status (drop
  notify-send)
status: Done
assignee:
  - claude
created_date: '2026-06-23 05:54'
updated_date: '2026-06-23 06:13'
labels:
  - prd
  - needs-triage
  - talk-to
dependencies: []
references:
  - 'IDEAS.md #4'
  - CONTEXT.md
  - crates/ghostty-voice-core/src/protocol.rs
  - crates/ghostty-voiced/src/main.rs
  - crates/ghostty-voice-io/src/download.rs
  - crates/talk-to/src/main.rs
  - README.md
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

On first run the daemon fetches the ~3 GB Whisper model, and today the only live signal of that download is a burst of `notify-send` toasts at 10% milestones. That is noisy, easy to miss or dismiss, decoupled from the rest of the daemon's observable **State**, and invisible once a desktop notification is gone. Now that there is a live status surface — the `talk-to` bottom **status strip** (and the richer `ghostty-voice-ctl status`) — the download should report there, like every other daemon state, instead of through transient toasts.

## Solution

Make the model-download progress part of the daemon's observable **State**, so it flows to the same surfaces every other state does:

- The daemon's `Downloading` state carries the percent complete. As the model streams in, the strip shows e.g. `downloading 42%` live, and `ghostty-voice-ctl status` reports `downloading 42` — both from one source of truth.
- `notify-send` is **no longer used for download progress** (or for the download start/complete/failed-retry messages). Those events stay in the journald log; the user watches the strip or `status`. All **non-download** notifications (whisper-server died/restarting, ydotoold unreachable, transcription/type failed, Held-for-replay, wrapper exited) are unchanged.

The user explicitly chose **strip + `status` only** — no `notify-send` fallback for the no-wrapper case (a first-run user with no `talk-to` open sees progress via `ghostty-voice-ctl status`).

## User Stories

1. As a first-run user with `talk-to` open, I want the bottom strip to show `downloading 42%` and climb live, so that I can see the model fetch progressing without watching notifications.
2. As a first-run user, I want `ghostty-voice-ctl status` to report `downloading <pct>`, so that I can check download progress from a shell with no `talk-to` running.
3. As a user, I do not want a stream of `notify-send` toasts during the download, so that my notification area is not spammed by a one-time setup step.
4. As a user, I want the strip to show plain `downloading` (no percent) before a total size is known, so that an indeterminate phase is still legible and never shows a bogus number.
5. As a user, I want the percent to advance smoothly but not thrash, so that the strip updates on whole-percent changes rather than flickering on every network chunk.
6. As a user, when the download finishes I want the strip to move on to `loading` then `idle` on its own, so that the transition to ready needs no toast.
7. As a user, if the download fails and retries, I want the strip to keep showing `downloading` (restarting its percent), so that a transient failure is not alarming and recovery is automatic.
8. As a user driving a wrapped agent, I want the download percent rendered with the same strip mechanism as `idle`/`recording`/`transcribing`, so that the surface behaves consistently.
9. As a developer, I want the percent to live inside the `State` value, so that there is a single source of truth that both the wrapper-sink `Frame::State` push and the `StatusReport` serialize automatically.
10. As a developer, I want the download (which runs on a blocking thread) to report progress across the sync→async boundary through one channel into `set_state`, so that the cached daemon state read by `status` and the `watch<State>` broadcast read by the strip never diverge.
11. As an operator, I want the download start/progress/complete/failure still recorded in the journald log, so that I can diagnose a stuck or failing first-run fetch after the fact.
12. As a user on an older daemon, I want the additive `downloading <pct>` token to remain backward-compatible with a bare `downloading`, so that a stale client/daemon still parses the state.
13. As a maintainer, I want the README first-run section to state that progress appears on the strip and in `status` (not via notifications), so that users know where to look.

## Implementation Decisions

**Approach (chosen): fold the percent into the state.** `State::Downloading` becomes `State::Downloading(Option<u8>)` — `None` = download underway, percent unknown (no `Content-Length` yet); `Some(p)` = `p`% complete. Because both the strip (`Frame::State` over the `watch<State>` broadcast) and `ghostty-voice-ctl status` (`StatusReport`) already serialize `State`, the percent reaches both surfaces from one source of truth. Alternatives considered and rejected: a parallel `Frame::DownloadProgress(u8)` + `StatusReport.download_pct` field (two representations to keep in sync), and a separate broadcast token channel (splits state across channels).

Modules built/modified:

- **`protocol.rs` (core, the deep module — pure, test-first):** `State::Downloading(Option<u8>)` (State stays `Copy`). Wire grammar on the deliberately-dumb newline line protocol: `downloading` or `downloading 42`; all other state words unchanged. `State::parse` accepts the full state substring and reconstructs the optional percent; the static `as_str()` is replaced by `encode_token() -> String` at the encode sites. A new `State::label() -> String` produces the human strip label (`"downloading 42%"`, `"downloading"`, `"idle"`, …). `Frame::State` encode/parse carry the new token unchanged in structure. `StatusReport::parse` is taught to isolate the state substring — take the tokens after `ok` up to the first `sink=`/`wrappers=` — so the two-token `downloading 42` round-trips alongside the existing `sink=`/`wrappers=` fields.
- **`machine.rs` (core):** the arms matching `State::Downloading` become `State::Downloading(_)`; behavior is unchanged (commands rejected while downloading; status answered).
- **`ghostty-voiced` (daemon):** enter the download with `set_state(State::Downloading(None))`. The download runs in `spawn_blocking`; its `on_progress` closure stops calling `notify-send` and instead sends whole-percent-throttled updates over a channel; a concurrent async loop receives them and calls `set_state(Downloading(Some(pct)))`, keeping the cached `state` (read by `status`) and the `watch<State>` broadcast (read by the strip) in lockstep. Remove the download-related `notify-send` calls (initial "downloading the ~3 GB model", the per-10% milestones, "download complete — loading", "download failed — retrying"); keep the corresponding `info!/warn!/error!` journald logs and every non-download `notify-send`.
- **`ghostty-voice-io/download.rs`:** unchanged — it already exposes `Progress { received, total }` with `percent() -> Option<u8>`, which the daemon's throttle consumes.
- **`talk-to` (binary):** `apply_frame` renders `State::label()` into the strip (a `state downloading 42` frame shows `downloading 42%`). Link-state tokens (`unreachable`/`incompatible`/`rejected`/`dropped`) are unchanged. Strip painting stays visually verified per project convention; the `label()` logic is unit-tested in core.
- **`ghostty-voice-ctl`:** no code change (it prints the reply, now `ok downloading 42 sink=… wrappers=…`). README first-run section updated to document `downloading <pct>` on the strip / in `status`, and that `notify-send` no longer reports download progress.

## Testing Decisions

**Chicago-style (classicist) TDD**, consistent with the existing `protocol.rs`/`sink.rs` unit tests and the `ghostty-voiced/tests/` integration tests. Good tests assert observable wire/behaviour (token round-trips, the rendered label, the pushed frame a client receives) through real collaborators — no mocks of the pure modules; test doubles only at true boundaries (the socket peer). Modules under test:

- **`protocol.rs` (unit, test-first):** `State::Downloading(Some/None)` ↔ token round-trips (`"downloading 42"`, `"downloading"`); `Frame::State(Downloading(Some(42)))` encode/parse round-trip; `StatusReport` carrying a `downloading 42` state plus `sink=`/`wrappers=` encode/parse round-trip; `State::label()` mapping including the `"42%"` form; backward-compatible parse of a bare `downloading`.
- **`machine.rs` (unit):** the `Downloading(_)` arms still reject commands and answer status.
- **Integration (`ghostty-voiced/tests/`, real socket + real protocol, mirroring `sink_registration.rs`):** a pushed `state downloading 42` frame is parsed by the client and rendered as `downloading 42%`.

Prior art: `crates/ghostty-voice-core/src/protocol.rs` inline tests; `crates/ghostty-voiced/tests/sink_registration.rs`, `status_report.rs`.

## Out of Scope

- Any `notify-send` **fallback** for download progress when no wrapper is attached (explicitly declined — strip + `status` only).
- Changes to any non-download notification.
- A progress surface for the focused-window user beyond `ghostty-voice-ctl status`.
- JSON/structured framing (keep the line protocol; a version token already exists for compatibility).
- Per-byte/byte-count display (whole-percent only).

## Further Notes

The real first-run download (network + ~3 GB model, GPU) is not exercisable in a headless/CI environment; the sync→async progress plumbing is validated by the protocol/label unit tests plus the pushed-frame integration test, and the live download path is verified by faithful wiring + reported honestly as demo-only at finalization. Domain vocabulary (Delivery sink, focused-window/wrapper sink, State, Downloading, Frame, StatusReport, status strip, Freshness window) is used throughout. The `downloading <pct>` token is additive and backward-compatible, consistent with the existing protocol-version handshake.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The talk-to strip shows `downloading <pct>%` live during the model download, and plain `downloading` when the total size is unknown
- [x] #2 `ghostty-voice-ctl status` reports the download percent (`ok downloading <pct> sink=… wrappers=…`), additive and backward-compatible with a bare `downloading`
- [x] #3 No `notify-send` is emitted for download progress/start/complete/failure; all non-download notifications are unchanged; download events remain in the journald log
- [x] #4 The percent lives in `State::Downloading(Option<u8>)` as the single source of truth feeding both `Frame::State` and `StatusReport`; the daemon updates it via `set_state` (strip and status never diverge), throttled to whole-percent changes
- [x] #5 All work is test-first (Chicago-style): protocol token/round-trip/label unit tests + a pushed-frame integration test; cargo test, clippy, and fmt green
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Approach A (decided): the download percent lives in `State::Downloading(Option<u8>)`, one source of truth feeding both `Frame::State` (strip) and `StatusReport` (status). Work the three subtasks in dependency order, Chicago-style TDD throughout.

13.1 protocol (core, test-first): `State::Downloading(Option<u8>)` (stays Copy); replace `State::as_str` with `encode_token()->String`; add `State::label()->String`; `State::parse` accepts the full state substring (`downloading 42`); `StatusReport::parse` isolates the state substring (tokens after `ok` up to first `sink=`/`wrappers=`); update `machine.rs` arms to `Downloading(_)` preserving the carried percent; update daemon/talk-to call sites (talk-to strip renders via `label()`). New integration test mirroring sink_registration.rs: a pushed `state downloading 42` parses + renders `downloading 42%`.

13.2 daemon: enter `Downloading(None)`; in `download_model_once` run the blocking download, throttle whole-percent changes through a pure `PercentThrottle` helper, send over a sync→async channel into `set_state(Downloading(Some(pct)))`. Remove the download-related notify-send calls (start, per-10% milestones, complete, failed-retry); keep journald logs + all non-download notifies. Retry resets percent to None. Unit-test the throttle.

13.3 docs: README first-run/model section documents `downloading <pct>` on the strip and in `status`, and that notify-send no longer reports download progress (journald still does), in CONTEXT.md vocabulary.

After each slice: cargo test --workspace, cargo clippy --workspace --all-targets, cargo fmt --check all green. Live ~3 GB download is not exercisable headless — wiring validated by protocol/label units + pushed-frame integration test + throttle unit test; reported demo-only at finalization. Leave on a branch, no push.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Delivered end to end across the three subtasks (all Done), Approach A throughout — the percent lives in `State::Downloading(Option<u8>)`, one source of truth feeding both `Frame::State` (strip) and `StatusReport` (status).

13.1 protocol (core, test-first): `State::Downloading(Option<u8>)` (stays Copy); `as_str`→`encode_token()->String`; `State::parse` reconstructs the optional percent from the whole state substring (bare `downloading` backward-compatible, >100/non-numeric/trailing rejected); `State::label()` renders `downloading 42%`; `StatusReport::parse` isolates the state substring before the `sink=`/`wrappers=` fields; machine `Downloading(_)` arms preserve the carried percent. New integration test `ghostty-voiced/tests/download_progress.rs` proves a pushed `state downloading 42` decodes and renders `downloading 42%` over a real socket.

13.2 daemon: pure unit-tested `PercentThrottle` (whole-percent dedup); `download_model_once` enters `Downloading(None)` per attempt (retry restarts the percent), runs the blocking transfer in `spawn_blocking`, and forwards throttled percents over a sync→async mpsc into a task calling `set_state(Downloading(Some(pct)))`. All four download notify-sends removed (start, per-10% milestone, complete, failed-retry); journald logs + every non-download notify retained.

13.3 docs: README first-run/model section, strip-states paragraph, and troubleshooting note document `downloading <pct>` in `status` / `downloading <pct>%` on the strip and that notify-send no longer reports download progress (journald still does), in CONTEXT.md vocabulary.

Verification: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --check` all green; every pre-existing test still passes.

Demo-only (reported honestly): the live first-run fetch (network + ~3 GB model + GPU) cannot be exercised in this headless environment, so the live climbing percent and the absence-of-toasts on a real download are verified by faithful wiring + the testable surfaces (protocol/label units, the `PercentThrottle` units, and the pushed-frame integration test), not by an end-to-end live run.

Branch `task-13-download-progress-on-strip` (4 code/doc commits + backlog updates). Not pushed; no tags; no AUR publish — left for review per instructions.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
PRD complete: model-download progress is now part of the daemon's observable **State** and flows to the same surfaces as every other state. The percent lives in `State::Downloading(Option<u8>)` — one source of truth that both the wrapper-sink `Frame::State` push and the `StatusReport` serialize — so the `talk-to` **status strip** shows `downloading 42%` and `ghostty-voice-ctl status` shows `downloading 42` without ever diverging, with a bare `downloading` until a `Content-Length` is known. The daemon streams real percentages from the `spawn_blocking` download through a pure, whole-percent `PercentThrottle` across a sync→async channel into `set_state`. All four download-related `notify-send` toasts are removed (start, per-10% milestones, complete, failed-retry); their journald logs and every non-download notification are unchanged. The `downloading <pct>` token is additive and backward-compatible with a bare `downloading` on the deliberately-dumb line protocol. README documents the new behaviour.

Success Validation (the five acceptance criteria): #1 strip shows `downloading <pct>%` / plain `downloading` — implemented, covered by the pushed-frame integration test (live climb is demo-only). #2 `status` reports `ok downloading <pct> sink=… wrappers=…`, additive/backward-compatible — covered by StatusReport encode/parse units. #3 no download notify-send, non-download notifications unchanged, events stay in journald — code-audited. #4 percent in `State::Downloading(Option<u8>)` as the single source feeding `Frame::State` and `StatusReport`, updated via `set_state`, whole-percent throttled — implemented and unit-tested. #5 all test-first (Chicago-style) with protocol/label/round-trip units, a throttle unit, and a pushed-frame integration test; cargo test/clippy/fmt green.

Honest scope note: the live ~3 GB network download (no GPU/mic/network here) is demo-only; every cargo-testable aspect is green and the live path is wired faithfully. Work is on branch `task-13-download-progress-on-strip`, not pushed — left for review.
<!-- SECTION:FINAL_SUMMARY:END -->
