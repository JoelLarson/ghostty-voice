# ghostty-voice — Voice Dictation for Ghostty on GNOME Wayland

## Purpose

Dictate to a coding agent (Claude Code, etc.) running over SSH in Ghostty. The agent's
own voice features are unavailable across SSH, so the Whisper model runs **locally on the
workstation** and the transcript is **typed into the focused terminal** as if from the
keyboard. You speak natural-language instructions; clean English prose appears in the
agent's prompt for you to review and submit.

## Target hardware (verified on the dev workstation)

| Component | Reality | Consequence |
|---|---|---|
| GPU | AMD RX 6900 XT, 16 GB VRAM, RADV **Vulkan** working | Run the largest model; **no CUDA** → faster-whisper is out |
| ROCm | `/opt/rocm` present but `rocminfo` missing (broken) | Use **Vulkan**, not ROCm |
| CPU | Ryzen 9 7950X (32 threads) | Capable fallback, but GPU is primary |
| RAM | 30 GB | Ample |
| Installed | `ydotool`, `ydotoold`, `pw-record`, `sox`(req), `wl-copy`, `notify-send`, `paplay` | Most of Phase 1 tooling already present |

OS: Arch Linux, GNOME on Wayland.

## Core constraints

- Wayland blocks X11 text injection → inject via **`ydotool`** (uinput, kernel-level).
- Model warmth is the whole point → the model stays **resident**; per-utterance latency is
  transcription only, paid once at boot.
- Text goes to an autonomous agent → **never auto-submit**; the human reviews first.

---

## Architecture

Three processes; one is a persistent service the others orbit.

```
  GNOME hotkeys ──spawn──> ghostty-voice-ctl ──unix socket──> ghostty-voiced (daemon)
   Super+D  toggle                                              │  owns ALL state
   Super+Shift+D  vad                                           │  supervises child:
   Super+Alt+D  cancel                                          ▼
                                              whisper-server (whisper.cpp + Vulkan, large-v3)
                                                  warm model, HTTP transcribe endpoint
   recording: pw-record / sox ──WAV──> daemon ──HTTP──> whisper-server ──text──> daemon
   daemon: correction-dict ──> ydotool type ──> focused Ghostty prompt
```

1. **`whisper-server`** — `whisper.cpp` built with `-DGGML_VULKAN=1`, model `large-v3`,
   held warm on the GPU, exposes the HTTP transcribe endpoint. **Not** a standalone systemd
   unit — it is a **child process supervised by the daemon**.
2. **`ghostty-voiced`** — the Rust controller daemon, the single systemd **user** service.
   Owns all state, listens on a Unix socket, supervises `whisper-server`, manages recording,
   POSTs audio, applies post-processing, calls `ydotool`, plays cues / notifications,
   manages the caches.
3. **`ghostty-voice-ctl`** — thin Rust client spawned by each hotkey; writes one command to
   the daemon socket and exits.

### Language & layout

Rust. Cargo workspace:

- `ghostty-voice-core` (lib) — config types, socket wire protocol, correction-dictionary logic.
- `ghostty-voiced` (bin) — supervising daemon (`tokio`, `reqwest`, `serde`/`toml`, `tracing`).
- `ghostty-voice-ctl` (bin) — thin client (`clap`).

Rust chosen because the daemon does **no in-process ML** (the model lives in
`whisper-server`); its work is subprocess lifecycle, socket and HTTP I/O, and text munging —
tokio's wheelhouse, and a long-running daemon is where Rust's reliability and small footprint
pay off. `whisper-rs` (Vulkan-capable) remains a future option if single-process embedding is
ever wanted, so the choice forecloses nothing.

### `whisper-server` supervision (daemon-owned)

- **Eager start on boot**: daemon spawns `whisper-server` at startup (the GPU is otherwise
  idle), polls its HTTP endpoint until the model is loaded/ready, *then* accepts commands.
- **Restart with backoff** on child death (GPU hiccup, OOM); `notify-send` on failure.
- **Teardown on daemon stop**: killing/stopping `ghostty-voiced` cascades to
  `whisper-server`, **freeing the 16 GB VRAM**. This is the "disable it" path — one
  `systemctl --user stop ghostty-voiced`.
- The `whisper-server` **binary path and launch args are config values**, so an upstream
  rename/flag change is a config edit, not a rebuild.

### `ydotoold` (NOT supervised)

`ydotoold` is a shared, privileged (uinput) input service. It is an **independent** systemd
user service and a **hard package dependency** — the daemon only **health-checks** its socket
at startup and `notify-send`s a clear error if unreachable. It is deliberately kept out of the
daemon's supervision/restart logic.

---

## Endpointing (two modes, switchable per-press)

- **Toggle** (`Super+D`): press to start, press again to stop → transcribe. The human decides
  when the utterance ends — zero false cutoffs, matches composing a prompt with thinking pauses.
- **VAD** (`Super+Shift+D`): press to start; **`sox` auto-stops on silence**
  (`silence 1 0.1 3% 1 2.0 3%` ≈ stop after ~2 s below threshold), then transcribe. Hands-free.
  Pressing `toggle` during a VAD recording is a manual early stop.
- **Cancel** (`Super+Alt+D`): abort the current recording, discard audio, no transcription.

The daemon owns recording state, so `toggle` is interpreted against the daemon's own state
(no cross-process race, no lockfile).

---

## Audio capture

- **Format**: 16 kHz, mono, s16 WAV — Whisper's native input, no resampling.
- **Toggle**: `pw-record` started on first press, stopped (killed) on second.
- **VAD**: `sox` records and self-terminates on silence.
- **Mic**: default PipeWire input; config override for a pinned device.

---

## Transcription & accuracy stack

Accuracy on technical jargon ("ydotool", "Ghostty", "useEffect", "kubectl", "rebase") is the
single hardest part of this use case. Layered, all stacking:

1. **Model**: `large-v3` (full, **not** `-turbo` — turbo's pruned decoder loses rare-word
   accuracy, and on a warm GPU we don't need its speed).
2. **`initial_prompt` vocabulary biasing**: seed the decoder with a domain vocab list
   (configurable, grows as misses are noticed), e.g.
   `"Transcript of technical instructions. Vocabulary: ydotool, Ghostty, faster-whisper,
   whisper.cpp, kubectl, ripgrep, useEffect, rebase, git stash, Wayland, systemd, Vulkan,
   Claude Code."`
3. **Correction dictionary** (post-processing): deterministic, case-insensitive find/replace
   for terms Whisper reliably mishears the same way (`"why do tool" → "ydotool"`,
   `"ghosty" → "Ghostty"`). This is the *real* Phase 3.1 — a **jargon spell-fixer**, NOT a
   "equals → =" code munger.
4. **`beam-size 8`**: larger beam buys accuracy on ambiguous audio at a latency we can afford.

Request params: `temperature 0` (determinism), no-speech/hallucination filtering on.

### Explicitly removed from the original plan

- ❌ **Code-symbol substitution** ("equals" → `=`, "dash dash" → `--`, "pipe" → `|`).
- ❌ **camelCase/snake_case auto-formatting**, keyword lowercasing.
- ❌ **Shell-vs-code auto-detection / prefix modes** ("shell:" / "code:").

These corrupt natural-language prompts to an agent. The tool dictates **English prose only**.
(If literal-code dictation is ever wanted, it would be a *separate explicit mode*, never
auto-detected — out of scope for now.)

---

## Injection (`ydotool`)

- `ydotool type --key-delay <ms> -- "<transcript>"`. **`key-delay` is configurable** and
  defaults conservatively — `ydotool` drops/reorders characters when typing too fast (and the
  very first keystroke after idle is prone to being eaten). Losing characters is unacceptable,
  so reliability is favored over typing speed.
- Transcript passed as a single `--`-terminated argument (no shell/flag interpretation).
- UTF-8: handled by `ydotool type`, but a non-priority — dictation is Latin/English prose.
- **Focus discipline**: `ydotool` injects into whatever window has focus (Wayland gives no
  sanctioned active-window query). An audio cue fires before typing; the user keeps Ghostty
  focused. No GNOME-extension focus guard (accepted risk for a personal tool).

---

## State machine & commands

States: **Idle → Recording → Transcribing → (type) → Idle**.

Commands (Unix socket via `ghostty-voice-ctl`):

| Command | Behavior |
|---|---|
| `toggle` | Idle→start recording; Recording→stop+transcribe; Transcribing→**ignored** (soft cue) |
| `vad` | start recording with `sox` silence auto-stop |
| `cancel` | abort recording, discard audio, return to Idle |
| `status` | print current state |
| `reload` | re-read config (vocab, correction dict, key-delay) **without** model reload |
| `replay-last` | re-run `ydotool type` on the most-recent cached transcript |

Hotkeys: `toggle`, `vad`, `cancel` bound now; `replay-last` CLI-only initially.

---

## Feedback

- **Audio cues (hot path)** via `paplay`: distinct **start** (now listening) and **stop**
  (working) sounds — the feedback needed every utterance, glanceable, focus-independent.
  An **empty/silence transcript uses the normal "done" cue** (no distinct error sound) and
  **types nothing**.
- **`notify-send` (exceptional only)**: "no speech detected" handled silently by the done cue;
  notifications reserved for "whisper-server unreachable", "ydotool failed", "downloading
  model", "model recovered". No per-utterance notification spam.
- **No tray/panel indicator** initially (would need a GNOME extension; audio covers "am I
  recording?").

---

## Caches & recovery (XDG cache dir)

Root: `$XDG_CACHE_HOME/ghostty-voice/` (→ `~/.cache/ghostty-voice/`).

- **WAV cache** — `recordings/<ISO-timestamp>.wav`. **Every** recording is kept (cheap, and
  doubles as the accuracy-debugging corpus: replay the exact audio Whisper misheard).
  **Count-capped** (~30), oldest pruned on each new recording.
- **Transcript cache** — `transcripts/`, **count-capped small** (~5; only active transcripts
  matter). Backs `replay-last`, which targets the most-recent transcript only.

### Failure handling

- **Empty / sub-0.3 s / hallucinated** (`[BLANK_AUDIO]`, silence-"Thank you.") → discard,
  done-cue, type nothing.
- **`whisper-server` unreachable** (mid-restart) → **keep the WAV**, queue it, retry on
  server health. **30 s auto-type window**: if transcription succeeds within ~30 s of
  recording, auto-type (you're still at the prompt); if slower, **do not auto-type** (stale,
  unsafe) → `notify-send` "failed — re-speak."
- **`ydotool type` fails** → transcript is in the transcript cache; `notify-send` "type
  failed"; recover with `replay-last` (refocus Ghostty first). Handles long dictations
  without loss. No automatic `wl-copy`.

---

## Configuration

`~/.config/ghostty-voice/config.toml` (with `reload` command for hot-apply of non-model fields):

```toml
[whisper]
binary       = "whisper-server"          # path; overridable on upstream rename
model_path   = "~/.local/share/ghostty-voice/models/ggml-large-v3.bin"
host         = "127.0.0.1"
port         = 8910                       # daemon launches & owns this
extra_args   = []                         # passthrough launch flags
beam_size    = 8
temperature  = 0.0
initial_prompt = "Transcript of technical instructions. Vocabulary: ydotool, Ghostty, ..."

[audio]
device       = "default"                 # PipeWire source name
vad_silence_seconds = 2.0
vad_threshold_pct   = 3
min_duration_seconds = 0.3                # shorter recordings are discarded

[inject]
key_delay_ms = 12                         # ydotool inter-key delay (reliability)
auto_submit  = false                      # never press Enter automatically

[feedback]
sound_start  = "..."                      # see Open Items — sound source undecided
sound_stop   = "..."

[cache]
wav_keep        = 30
transcript_keep = 5
retry_window_seconds = 30

[corrections]                             # deterministic jargon spell-fixer
"why do tool" = "ydotool"
"ghosty"      = "Ghostty"
```

### Fixed paths & interfaces

| Thing | Value |
|---|---|
| Control socket | `$XDG_RUNTIME_DIR/ghostty-voice.sock` |
| `whisper-server` endpoint | `POST http://127.0.0.1:8910/inference` (multipart WAV + params) |
| Model file | `~/.local/share/ghostty-voice/models/ggml-large-v3.bin` |
| Model source | HuggingFace `ggerganov/whisper.cpp` → `ggml-large-v3.bin` (verify SHA on download) |
| WAV cache | `$XDG_CACHE_HOME/ghostty-voice/recordings/<ISO-timestamp>.wav` |
| Transcript cache | `$XDG_CACHE_HOME/ghostty-voice/transcripts/` |
| Config | `~/.config/ghostty-voice/config.toml` |
| Logs | journald via `tracing` (`journalctl --user -u ghostty-voiced`) |

### Wire protocol (ctl ↔ daemon)

Newline-delimited UTF-8 over the Unix socket. Request = one command word
(`toggle` | `vad` | `cancel` | `status` | `reload` | `replay-last`). Response = one line:
`ok <state>` or `err <message>`. `status` returns the current state
(`idle` | `recording` | `transcribing` | `downloading`). Keep it dumb-simple; no JSON needed
unless a field grows.

### systemd user unit (`ghostty-voiced.service`)

- `ExecStart=ghostty-voiced`, `Restart=on-failure`, `RestartSec=2`.
- `WantedBy=default.target`; enable with `systemctl --user enable --now ghostty-voiced`.
- The unit owns only the daemon; the daemon owns `whisper-server`'s lifecycle.
- Document `loginctl enable-linger $USER` if dictation is wanted before/without a graphical login.

### GNOME hotkey binding (`install-hotkeys`)

Sets three custom keybindings under
`org.gnome.settings-daemon.plugins.media-keys custom-keybindings`, each pointing at
`ghostty-voice-ctl <toggle|vad|cancel>`. Default combos `Super+D` / `Super+Shift+D` /
`Super+Alt+D` (configurable). The helper writes the `customN` schema entries via `gsettings`.

---

## Packaging & install (Arch PKGBUILD)

- **PKGBUILD** is the installer (replaces `install.sh`/`Makefile`).
- **Vendor the `whisper.cpp` Vulkan build**: PKGBUILD compiles `whisper.cpp` with
  `-DGGML_VULKAN=1` (makedepends: cmake, vulkan headers, etc.).
- **Depends**: `ydotool` (→ `ydotoold`), `pipewire`/`pw-record`, `sox`, `wl-clipboard`,
  `libnotify`, Vulkan runtime.
- **Model NOT in the package** (~3 GB). Downloaded on **first run**: daemon checks
  `model_path`; if missing, `notify-send` "downloading model…" and fetches `ggml-large-v3.bin`.
  Installer stays out of model management.
- Installs: the three binaries, the `ghostty-voiced` systemd **user** unit, `config.toml.example`.
- **`ghostty-voice-ctl install-hotkeys`** helper sets the three GNOME custom keybindings via
  `gsettings` (per-user session — cannot run from package install; post-install message points
  the user to it).

---

## Deliverables

- `ghostty-voice-core`, `ghostty-voiced`, `ghostty-voice-ctl` (Cargo workspace).
- `ghostty-voiced.service` (systemd user unit).
- `config.toml.example`.
- `PKGBUILD`.
- `README`: setup, the `install-hotkeys`/`download-model` first-run steps, troubleshooting
  (ydotoold permissions/udev, Vulkan check, GPU teardown to free VRAM).

---

## Testing & edge cases

- **Modes**: toggle start/stop, VAD silence auto-stop, cancel mid-recording.
- **Focus**: Ghostty focused (happy path) vs. wrong window focused (expected misfire →
  `replay-last` recovery).
- **Failures**: `whisper-server` killed mid-utterance (retry + 30 s window); `ydotoold` down
  (startup health-check error); `ydotool` type failure (transcript cache + `replay-last`).
- **Transcription**: empty/silence → nothing typed; sub-0.3 s discard; hallucination filtering;
  long dictation (>30 s); jargon accuracy against the WAV cache corpus.
- **Lifecycle**: boot eager-start readiness; `whisper-server` crash-restart-backoff;
  daemon stop frees VRAM; `reload` applies config without model reload.

---

## Open items to decide (deferred — not blockers, but unresolved)

These were given sensible defaults but never explicitly decided; revisit when convenient.

1. **Audio cue sound files.** Where do the start/stop sounds come from? Options: (a) ship two
   small `.oga`/`.wav` assets in the package; (b) reuse freedesktop theme sounds via
   `canberra-gtk-play -i <event>` (e.g. `audio-volume-change`); (c) a terminal bell. Default
   assumption: ship our own two short sounds, played with `paplay`. **Undecided.**
2. **Default `key_delay_ms` value.** 12 ms is a guess; the real value is found empirically
   against `ydotool` char-drop behavior on this machine. Tune during the injection slice.
3. **First-run download UX while eager-on-boot.** During the ~3 GB model fetch the daemon is
   in `downloading` state and must reject `toggle`/`vad` with a notify ("model still
   downloading"), not hang. Confirm the download runs in the background and the daemon reports
   progress via `notify-send`.
4. **`whisper-server` Vulkan launch flags.** Exact args to force GPU offload (and confirm RADV
   is actually used, not silent CPU fallback) are TBD until the vendored build exists. Verify
   with a warm-latency measurement on `large-v3`.
5. **`whisper-server` readiness signal.** Confirm the cleanest "model loaded" probe (HTTP
   200 on a trivial request vs. a `/models`-style endpoint vs. log scrape).
6. **`ydotoold` setup ownership.** PKGBUILD depends on `ydotool`, but the udev rule for
   `/dev/uinput` + `input` group membership + `YDOTOOL_SOCKET` agreement is user environment
   setup. Decide whether `install-hotkeys` (or a sibling `doctor` command) also checks/repairs
   this, or whether it's README-only.
7. **VAD threshold defaults** (`vad_silence_seconds`, `vad_threshold_pct`) need real-mic tuning.

## Suggested milestone breakdown (for issue-splitting tomorrow)

Ordered so each slice is independently demoable and de-risks the next.

1. **M1 — whisper-server supervision + transcription round-trip.** Vendor the Vulkan build;
   daemon spawns `whisper-server`, polls readiness, restarts on death, tears down on stop.
   Prove it with a manual `curl` of a sample WAV → text. *De-risks the GPU/Vulkan unknown
   before any UI exists.* (Open items 4, 5.)
2. **M2 — recording + socket control core.** Unix socket + wire protocol, `ghostty-voice-ctl`,
   the Idle/Recording/Transcribing state machine, `toggle` via `pw-record`, POST to
   whisper-server, return text to stdout (no injection yet). `cancel`, `status`, `reload`.
3. **M3 — injection + feedback.** `ydotool type` with `key-delay`, audio cues, the
   focus-discipline pre-type cue, the clipboard-less `replay-last` recovery. (Open items 1, 2.)
4. **M4 — accuracy stack.** `initial_prompt` wiring, correction dictionary, `beam-size`,
   empty/hallucination/sub-min-duration filtering.
5. **M5 — VAD mode.** `sox` silence auto-stop, threshold config. (Open item 7.)
6. **M6 — caches + failure recovery.** WAV/transcript caches with count caps, the 30 s retry
   window, server-down queueing, ydotool-fail recovery.
7. **M7 — packaging.** PKGBUILD (vendored whisper.cpp Vulkan build), systemd user unit,
   first-run model download, `install-hotkeys`, README. (Open items 3, 6.)

## Resolved decisions (supersedes original open questions)

| Original suggestion | Resolved |
|---|---|
| Stateless script | ❌ → warm model in supervised `whisper-server` |
| faster-whisper | ❌ CUDA-only → **whisper.cpp + Vulkan**, `large-v3` |
| Python ~200–300 lines | ❌ → **Rust** Cargo workspace |
| `install.sh` | ❌ → **PKGBUILD** (vendored Vulkan build) |
| Push-to-toggle *or* hold | both **toggle + VAD**, switchable hotkeys |
| Code-symbol substitution | ❌ removed — **English prose only** + jargon correction |
| Auto-submit | ❌ off — human reviews before Enter |
| sounddevice/pyaudio | n/a — `pw-record`/`sox` shell-out |
