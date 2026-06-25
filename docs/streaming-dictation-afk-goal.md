# AFK Goal — Streaming dictation (TASK-18)

Run this as a single autonomous (AFK) goal. It implements the streaming-dictation
feature end to end by completing the six Backlog subtasks of **TASK-18** in
dependency order, with every product decision already made (below) so you never
need to stop and ask.

## Objective

Ship **streaming dictation**: Shift+F9 starts a live, self-editing preview into the
wrapped agent's prompt; a ~10s silence or Shift+F10 finalizes; the full-utterance
**batch** transcription then replaces the preview with the jargon-corrected
Transcript. Parent PRD: **TASK-18**. Subtasks: **TASK-18.1 … TASK-18.6**.

## How to run each task

For every subtask, in order:

1. `mcp__backlog__task_view` it; read `mcp__backlog__get_backlog_instructions`
   with `instruction="task-execution"` the first time.
2. Set it **In Progress**. Write an implementation plan into the task.
3. Implement **test-first, Chicago-style (classicist) TDD** — red → green → refactor,
   asserting observable behaviour through *real* collaborators; test doubles only at
   the true external boundary (whisper-server over a localhost socket; a stand-in PTY
   line editor for wrapper edits). Mirror the existing unit style
   (`session.rs`/`sink.rs`/`queue.rs`/`machine.rs`/`pty.rs`/`trigger.rs`) and the
   real-socket style of `transcribe.rs`.
4. Keep **`cargo test --workspace` green, `cargo clippy` clean, `cargo fmt` clean at
   every commit.** Atomic commits. Never fewer tests than before.
5. Check off the task's acceptance criteria as they pass; write a PR-style
   `finalSummary`; set the task **Done**.
6. Do **not** merge to `main`. Work on a dedicated branch `task-18-streaming`
   (branch from `main`; bring along the `backlog/tasks/task-18*.md` files). Leave the
   branch for human review (same convention as TASK-17). Commit messages and PR body
   follow the repo's trailer convention; **never put task/slice IDs in source code or
   comments** (CLAUDE.md) — reference durable ADR/CONTEXT concepts instead.

## Dependency order (respect blockers)

```
18.1  ──▶ 18.2 ──▶ 18.3 ─┐
  └────▶ 18.4 ───────────┤──▶ 18.6
         18.2 ──▶ 18.5 ──┘
```

Do them: **18.1, 18.2, 18.3, 18.4, 18.5, 18.6.** (18.4 needs only 18.1; 18.5 needs
18.2; 18.6 needs 18.3+18.4+18.5.)

## Locked decisions — do NOT re-ask

- **Self-paced, no latency gate.** The single existing whisper-server is used; the
  decode loop starts the next decode when the last returns. Ship regardless of
  measured latency — the preview is allowed to lag on a busy GPU. No second model.
- **Live lane = rough preview; batch reconcile = truth.** Commit policy is
  **LocalAgreement-2**. The correction dictionary is applied **only** in the batch
  reconcile; the live preview shows raw Whisper text.
- **Backspace edits count Unicode codepoints** (ASCII-dominant). Edit fidelity is
  proven in CI against a **stand-in PTY line editor** (readline/bash). The real
  Claude Code composer is **not** in CI — it's the human's one-time manual smoke-test
  after 18.6 (note it in the final summary; do not block on it).
- **No wrapper registered at Shift+F9** → capture proceeds, the live preview no-ops,
  and the final Transcript is **Held-for-replay** (consistent with batch).
- **`[streaming]` config defaults** (all overridable, upgrade-tolerant):
  `window_seconds=15`, live-lane `beam_size=1`, `session_end_silence_seconds=10`,
  `silence_threshold_pct` = the existing VAD default.
- **Triggers.** Shift+F9 (idle) → start streaming; Shift+F10 → stop whatever runs
  (streaming → force-stop = finalize+deliver), else start a Toggle batch when idle.
  **VAD relinquishes the F9 start slot** but stays reachable via
  `ghostty-voice-ctl vad`. Toggle/VAD/Continuous batch modes otherwise remain.
- **Keystroke suppression** is active only during a live dictation; Shift+F9/F10 still
  resolve as triggers; everything else is dropped (not buffered). `Cancel` (via
  `ghostty-voice-ctl cancel`) erases the streaming buffer and delivers nothing.
- **ADRs.** Respect ADR-0001 (GPU pinning) and ADR-0003 (talk-to is the sole
  interface). Slice 18.6 adds a **new ADR extending ADR-0002** (streaming preview +
  batch reconcile is a conscious extension, not a reversal).

## Architecture guardrails

- Deep, pure, isolation-tested modules carry the load: the **streaming commit engine**
  (LocalAgreement-2), **window-PCM math**, and **pty edit-bytes** (beside
  `injection_bytes`). The daemon decode loop is thin glue, like today's Continuous
  driver loop. The streaming capture is a `Capture::Streaming` variant under the
  Recorder's one-mouth invariant. The final Transcript flows through `Delivery`
  (bound-at-trigger, cached-before-type, Held-for-replay); the live preview lane is
  ephemeral to the active wrapper and bypasses the record-order queue.
- Use the domain vocabulary from CONTEXT.md throughout (Utterance, Transcript,
  Recorder, Delivery, wrapper sink, Auto-type, Held-for-replay).

## Done when

All of TASK-18.1 … TASK-18.6 are **Done**, `cargo test --workspace` / clippy / fmt are
green, the `task-18-streaming` branch is pushed (or a PR opened) for human review, and
the final summary notes the one remaining human step: a manual smoke-test of live
editing in a real Claude Code composer.
