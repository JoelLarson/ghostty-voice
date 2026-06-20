---
id: TASK-1
title: 'S1 — Walking skeleton: speak → text typed into Ghostty'
status: To Do
assignee: []
created_date: '2026-06-20 07:26'
labels:
  - needs-triage
dependencies: []
references:
  - PLAN.md
  - CONTEXT.md
  - docs/adr/0001-pin-whisper-to-discrete-gpu-by-pci-address.md
  - docs/adr/0002-batch-transcription-first-segmented-pipeline-deferred.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

I dictate to a coding agent running in Ghostty over SSH, but the agent's own voice features can't cross SSH. I have no way to speak an instruction and have it land as typed text in the agent's prompt. Before investing in the full daemon, modes, and accuracy stack, I need proof that the core thread even works on *this* hardware: that Whisper `large-v3` actually runs on the RX 6900 XT via Vulkan (not silently on the iGPU or CPU), and that `ydotool` reliably injects the transcript into the focused terminal without dropping characters.

## Solution

A single CLI command — the **walking skeleton** — that records one spoken utterance, sends it to a **manually-started, warm** `whisper-server`, and types the returned transcript into the focused window via `ydotool`. This is the thinnest possible end-to-end thread: no daemon, no supervision, no caching, no accuracy stack. Its entire purpose is to prove the two scary unknowns *together* — Vulkan transcription on the 6900 XT and ydotool injection reliability — and to capture a real warm-latency number on `large-v3`.

## User Stories

1. As a developer, I want to run one command and have it record my microphone, so that I can dictate an utterance without wiring up a daemon first.
2. As a developer, I want the recording to stop when I signal end-of-utterance (Enter), so that I control when I'm done speaking.
3. As a developer, I want the recorded audio captured as 16 kHz mono s16 WAV, so that it's Whisper's native input with no resampling.
4. As a developer, I want the command to POST my audio to a warm `whisper-server` and receive the transcript, so that the model stays resident and I'm not reloading 3 GB per utterance.
5. As a developer, I want the returned transcript typed into whatever window is focused, so that I can keep Ghostty focused and see the text appear in the agent's prompt.
6. As a developer, I want Enter to never be pressed automatically, so that I review the text before submitting it to the agent.
7. As a developer, I want `whisper-server` pinned to the RX 6900 XT by PCI address, so that it never silently runs on the integrated Raphael GPU or the CPU.
8. As a developer, I want the tool to assert at load that the selected device's name matches the 6900 XT, so that a wrong-GPU launch fails loudly instead of running dog-slow.
9. As a developer, I want the PCI→Vulkan-index resolution to be deterministic and unit-tested, so that I trust the pinning logic without a GPU in the test loop.
10. As a developer, I want the host, port, model path, `vulkan_device` (PCI address), audio device, and `key_delay_ms` read from a config file, so that I can adjust them without recompiling.
11. As a developer, I want the transcript passed to `ydotool` as a single `--`-terminated argument, so that no part of my speech is interpreted as a shell or flag token.
12. As a developer, I want a configurable `key_delay_ms`, so that I can tune against `ydotool`'s character-drop behavior on this machine.
13. As a developer, I want to observe and record the warm transcription latency for a typical utterance, so that I have a real number for `large-v3` + `beam` on this GPU.
14. As a developer, I want to run the transcription path against a saved sample WAV, so that I can debug accuracy and timing without re-speaking.
15. As a developer, I want the core logic separated from the subprocess/IO edges, so that the deterministic parts are testable with real objects and no mocks (Chicago-style TDD).
16. As a developer, I want a clear error if `whisper-server` is unreachable, so that I know to start it rather than staring at a hang.
17. As a developer, I want the workspace seeded as a Cargo workspace with a `ghostty-voice-core` lib, so that S2+ build on the same foundation.

## Implementation Decisions

- **Cargo workspace** seeded now: `ghostty-voice-core` (lib, all pure logic) plus a thin skeleton binary. The full `ghostty-voiced` / `ghostty-voice-ctl` split is deferred to S2 — but all decision logic goes in `core` from day one so the daemon stays a thin IO shell.
- **Eight modules.** Pure (in `core`): **Vulkan device resolver** (PCI address + device enumeration → enumeration index + load-name assertion), **transcription response parser** (whisper-server JSON → clean transcript text), **config loader** (`config.toml` → typed config, defaults, `~` expansion), **ydotool command builder** (text + `key_delay_ms` → argv with `--` terminator). Boundary adapters: **audio recorder** (`pw-record`), **transcription transport** (HTTP multipart POST to `/inference`), **text injector** (`ydotool` exec). Plus a thin **orchestrator** wiring record → transcribe → inject.
- **whisper-server is started manually** (warm) for S1, pinned to the 6900 XT. The daemon translates the `vulkan_device` PCI address into `GGML_VK_VISIBLE_DEVICES` internally — for S1 this resolution logic exists and is tested, even though the human launches the server. Supervision (eager start, readiness, restart/backoff, teardown) is **S2**. See ADR-0001.
- **Trigger = simple CLI command, Enter-to-stop** (no toggle). Real hotkey-toggle needs shared cross-press state = the daemon + socket (lockfiles explicitly rejected), which is S2. GNOME custom keybindings fire on key *press only* (no key-up), so push-to-hold isn't available via that path regardless.
- **Audio**: 16 kHz mono s16 WAV via `pw-record`; default PipeWire source (config override allowed but not required for S1).
- **Transcription request**: multipart WAV POST to `http://<host>:<port>/inference`; default `beam`/`temperature` from config; response parsed to a single trimmed string. Batch only (ADR-0002).
- **Injection**: `ydotool type --key-delay <ms> -- "<transcript>"`; `auto_submit` is hard-off (no Enter).
- **Config** (`config.toml`, minimal subset): `[whisper] host, port, model_path, vulkan_device`; `[audio] device`; `[inject] key_delay_ms`.

## Testing Decisions

- A good test asserts **external behavior**, not internals — given inputs to a module, assert its observable output, with real collaborators and no mocking of pure logic.
- **Unit-tested (Chicago/classicist, real objects):** Vulkan device resolver (PCI→index + name-assertion, including wrong-name and not-found cases), transcription response parser (well-formed, multi-segment, empty), config loader (defaults, `~` expansion, missing fields), ydotool command builder (`--` terminator, special characters, key-delay).
- **Integration:** one end-to-end test driving real `pw-record` → real `whisper-server` → real `ydotool` against a saved sample WAV, asserting the expected text is injected. Boundary adapters get thin integration coverage; they are not unit-tested with mocks.
- **Prior art:** none — greenfield. S1 establishes the core/shell split and the pure-unit + thin-integration pattern that S2+ follow.

## Out of Scope

- Daemon, Unix socket, wire protocol, `ghostty-voice-ctl`, toggle/`cancel`/`status`/`reload` (S2).
- whisper-server supervision: eager start, readiness probe, restart/backoff, VRAM teardown (S2).
- Cache-before-type, ordered delivery queue, freshness window, `replay-last`, audio cues, `notify-send` (S3).
- Accuracy stack: `initial_prompt` vocab biasing, correction dictionary, beam tuning, empty/hallucination/min-duration filtering (S4).
- VAD mode (S5). Continuous mode — the north-star (S6). PKGBUILD, first-run model download, `install-hotkeys`, README (S7).
- No model auto-download (model placed manually). No GNOME focus guard (accepted risk).

## Further Notes

- **Two RADV devices confirmed** on this box (6900 XT `0000:03:00.0`, iGPU `0000:1a:00.0`) — pinning + load-name assertion are mandatory, not optional (ADR-0001).
- **`sox` is currently missing** on the machine; not needed for S1 (VAD is S5) but flagged for later install.
- Capturing the **warm-latency number** for `large-v3` is a primary S1 outcome — there is no latency target (accuracy-first), but the number informs Continuous-mode design later.
- Known later traps to design in, not discover: `initial_prompt` ~224-token cap (S4), and a startup `loading`/`starting` state distinct from `downloading` (S2). Out of scope here, noted so they aren't forgotten.
- Refs: `PLAN.md`, `CONTEXT.md`, `docs/adr/0001-pin-whisper-to-discrete-gpu-by-pci-address.md`, `docs/adr/0002-batch-transcription-first-segmented-pipeline-deferred.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Running the CLI command records the mic, transcribes via a warm whisper-server, and types the transcript into the focused window
- [ ] #2 Enter is never pressed automatically (auto_submit hard-off); the transcript is left in the prompt for review
- [ ] #3 whisper-server is pinned to the RX 6900 XT via the vulkan_device PCI address; a wrong/mismatched device fails loudly with a clear error rather than running on iGPU/CPU
- [ ] #4 PCI->Vulkan-index resolution and load-name assertion are unit-tested with real objects (no GPU required), including not-found and wrong-name cases
- [ ] #5 Transcript is passed to ydotool as a single -- terminated argument; command-builder is unit-tested for special characters and key-delay
- [ ] #6 Config (host, port, model_path, vulkan_device, audio device, key_delay_ms) is read from config.toml; config loader unit-tested for defaults and ~ expansion
- [ ] #7 Transcription response parser is unit-tested (well-formed, multi-segment, empty)
- [ ] #8 One end-to-end integration test drives real pw-record -> whisper-server -> ydotool against a saved sample WAV and asserts injected text
- [ ] #9 A warm-latency measurement for large-v3 on the 6900 XT is captured and recorded
- [ ] #10 Unreachable whisper-server produces a clear error, not a hang
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Seed Cargo workspace (ghostty-voice-core lib + skeleton bin). Build pure core modules first, TDD inside-out: (1) config loader, (2) Vulkan device resolver (PCI->index + name assertion), (3) transcription response parser, (4) ydotool command builder. Then boundary adapters: audio recorder (pw-record), transcription transport (HTTP multipart /inference), text injector (ydotool exec). Then orchestrator wiring record->transcribe->inject. whisper-server started manually/warm for this slice (supervision is S2). Finish with the e2e integration test on a sample WAV + capture warm latency.
<!-- SECTION:PLAN:END -->
