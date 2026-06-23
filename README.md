# ghostty-voice

Voice dictation that types clean English prose into the focused **Ghostty** terminal on
**Wayland** (any compositor — desktop-environment-agnostic), so you can speak instructions to
a coding agent running over SSH. Whisper runs **locally** on the workstation GPU (whisper.cpp +
Vulkan, `large-v3`); the transcript is injected as if typed from the keyboard. The text is
**never auto-submitted** — you review before pressing Enter.

See `PLAN.md` for the full design, `CONTEXT.md` for the domain language, and `docs/adr/` for
the load-bearing decisions.

## Architecture

Three processes; the daemon owns all state:

- **`whisper-server`** — whisper.cpp built with `-DGGML_VULKAN=1`, model held warm on the GPU,
  supervised as a child of the daemon.
- **`ghostty-voiced`** — the daemon (one systemd **user** service): supervises whisper-server,
  listens on a Unix socket, records, transcribes, injects, manages caches.
- **`ghostty-voice-ctl`** — thin client for manual commands (`status`, `replay-last`) and
  `doctor`. (The everyday triggers are tactile — the daemon reads them directly via evdev, no
  per-keypress process spawn.)

## Build

```sh
cargo build --release          # the three binaries land in target/release/
cargo test                     # the test suite (pure logic + boundary integration)
```

For a packaged install on Arch:

```sh
makepkg -si                    # builds the Rust workspace + a vendored whisper.cpp Vulkan build
```

## First-run setup

1. **GPU pin** — `ghostty-voice` pins whisper to your discrete GPU by **PCI address**
   (`[whisper].vulkan_device`, default `0000:03:00.0`). Find yours with `lspci`. Two RADV
   devices (discrete + iGPU) make this mandatory — see `docs/adr/0001`.
2. **Model** (~3 GB, not packaged) — fetched **on first run**. When the daemon starts and
   `[whisper].model_path` is absent, it enters the `downloading` state (visible via
   `ghostty-voice-ctl status`), streams `ggml-large-v3.bin` from `[whisper].model_url`
   (HuggingFace by default) into `~/.local/share/ghostty-voice/models/`, and `notify-send`s
   progress at 10% milestones. While downloading, `toggle`/`vad`/`continuous` are rejected with
   "model still downloading" — the daemon never hangs. The fetch is SHA-256 verified if you pin
   `[whisper].model_sha256` (copy the hash from the HuggingFace LFS page); leave it empty to
   accept by presence. A failed/corrupt fetch is discarded and retried with backoff.
3. **Config** — copy `config.toml.example` to `~/.config/ghostty-voice/config.toml` and edit.
4. **Environment** — `ydotoold` must be running, you must have `/dev/uinput` access, and the
   trigger device must be readable (you're in the `input` group). Diagnose with:
   ```sh
   ghostty-voice-ctl doctor
   ```
5. **Triggers** — the daemon reads two keys directly from `/dev/input` via evdev (any
   compositor, no GNOME). Defaults are **Shift+F10** (Start) and **Shift+F9** (Stop); change
   them in the `[input]` section of your config:
   ```toml
   [input]
   start_combo = "Shift+F10"
   stop_combo  = "Shift+F9"
   hold_threshold_ms = 250
   # Pin your keyboard if you have more than one (find names with `cat /proc/bus/input/devices`):
   device = "auto"          # or "name:daskeyboard", or a "/dev/input/eventN" path
   ```
   Restart the daemon after editing. Because evdev does not grab the device, the trigger key
   **also** does its normal thing (e.g. an F-key still sends its escape to the terminal) — pick
   a spare key you don't otherwise use. With more than one keyboard, set `device = "name:..."`
   rather than `auto`, or the daemon may read the wrong one.
6. **Enable the daemon**:
   ```sh
   systemctl --user enable --now ghostty-voiced
   ```
   **Linger** — by default a user service runs only while you have an active login session. To
   keep `ghostty-voiced` (and the warm model) running across logout, or to have it available
   before/without a graphical login, enable lingering:
   ```sh
   loginctl enable-linger "$USER"
   ```
   Disable it with `loginctl disable-linger "$USER"` if you'd rather the daemon (and its 16 GB
   of VRAM) only live during your session.

## Configuration keys

All keys live in `~/.config/ghostty-voice/config.toml` (see `config.toml.example`). `reload`
hot-applies non-model fields (`ghostty-voice-ctl reload`). The slice each key belongs to:

- `[whisper]` — `model_path`, `model_url`, `model_sha256` (first-run download, S7),
  `vulkan_device` (GPU pin, S2/ADR-0001), `beam_size`, `temperature`, `prompt_prefix`, `vocab`
  (accuracy stack, S4).
- `[audio]` — `max_recording_seconds` (runaway cap, S3), `min_duration_seconds` (S4),
  `vad_silence_seconds` / `vad_threshold_pct` (VAD, S5), `clip_cut_pause_seconds` /
  `session_end_silence_seconds` / `min_clip_seconds` (Continuous mode, S6).
- `[inject]` — `key_delay_ms` (S2).
- `[input]` — tactile triggers (S8): `start_combo` / `stop_combo` (e.g. `Shift+F10`),
  `hold_threshold_ms` (tap-vs-hold cutoff), and `device` (`auto`, a `/dev/input/...` path, or
  `name:<substr>`). Edit directly; restart the daemon to apply.
- `[feedback]` — `sound_start` / `sound_stop` (cues, S7): a freedesktop theme event id (default)
  or a sound-file path — both played via `paplay`; empty disables.
- `[cache]` — `wav_keep`, `transcript_keep`, `retry_window_seconds` (delivery + freshness, S3).
- `[corrections]` — the jargon spell-fixer table (S4).

## Usage

Two tactile keys drive everything (defaults **Shift+F10** = Start, **Shift+F9** = Stop), with
tap-vs-hold semantics. Recording begins the instant Start goes down (record-on-press), so
push-to-talk never clips the first word.

- **Start tap** (quick press/release): **latch** recording on — talk freely, stop deliberately.
- **Start hold** (press and hold): **push-to-talk** — records only while held, stops and
  transcribes on release.
- **Stop tap**: **stop** the latched recording → transcribe → type.
- **Stop hold**: start a **hands-free VAD** recording — `sox` auto-stops on the first trailing
  silence, then transcribes → types.

The tap-vs-hold cutoff is `[input].hold_threshold_ms` (~250 ms). Change the keys or device in
the `[input]` section of your config and restart the daemon.

- **Continuous mode** (the north-star long-form mode) and `cancel` remain available as
  `ghostty-voice-ctl continuous` / `ghostty-voice-ctl cancel` (no tactile gesture this round).
  In Continuous mode, short pauses cut the audio into **clips** that batch-transcribe in the
  background (context-chained), and a long silence (~10 s) ends the **session** — the assembled
  transcript is typed once. Tune the segmentation with `clip_cut_pause_seconds` /
  `session_end_silence_seconds` / `min_clip_seconds` in `[audio]`.
- **Status**: `ghostty-voice-ctl status` reports the daemon state plus the active **Delivery
  sink** and how many **wrapper sinks** are registered:
  ```
  ok idle sink=focused-window wrappers=0     # no talk-to running — default ydotool path
  ok idle sink=wrapper wrappers=1            # a talk-to wrapper sink is the active target
  ```
  `sink=wrapper` confirms dictation routes into a `talk-to`-wrapped agent's PTY; `sink=focused-window`
  is the default `ydotool` Auto-type into the focused window. The `sink=`/`wrappers=` fields are
  additive — an older daemon answers a bare `ok <state>`.
- **Replay**: `ghostty-voice-ctl replay-last` re-injects the most-recent cached transcript.
- **Disable / free the 16 GB VRAM**: `systemctl --user stop ghostty-voiced` (cascades to
  whisper-server).

## Troubleshooting

- **Nothing typed / wrong window** — `ydotool` injects into whatever has focus; keep Ghostty
  focused. A misfire is recoverable: refocus Ghostty, then `ghostty-voice-ctl replay-last`
  re-injects the most-recent cached transcript. A transcript is cached to
  `$XDG_CACHE_HOME/ghostty-voice/transcripts/` *before* typing, so it is never lost even if
  typing fails. WAV recordings are kept under `recordings/` (count-capped) as a debugging corpus.
- **`ydotoold` errors** — run `ghostty-voice-ctl doctor`; ensure `ydotoold` is running, you're
  in the `input` group, and `/dev/uinput` exists.
- **Wrong GPU / slow** — check the daemon log (`journalctl --user -u ghostty-voiced`) for the
  `pinning whisper-server to Vulkan device N` line; confirm it's your discrete card.
- **Vulkan not used (silent CPU fallback)** — confirm RADV sees your card with
  `vulkaninfo --summary` (from `vulkan-tools`); the discrete GPU must appear. whisper-server is
  pinned via `GGML_VK_VISIBLE_DEVICES`; a missing Vulkan ICD loader (`vulkan-icd-loader`, a
  package dependency) makes it fall back to CPU.
- **Stuck in `downloading`** — `ghostty-voice-ctl status` shows `downloading` until the ~3 GB
  model lands; watch progress notifications, or `journalctl --user -u ghostty-voiced`. A
  corrupt fetch (SHA mismatch when `model_sha256` is pinned) is discarded and retried.
- **Dropped characters** — raise `[inject].key_delay_ms`.

## Status

Built and tested in CI-equivalent (pure logic + mock-server / fake-socket integration).
**On-hardware end-to-end** (real GPU transcription, mic capture, ydotool injection) requires
the workstation and is validated manually — see the build/first-run steps above.
