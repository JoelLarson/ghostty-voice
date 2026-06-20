# ghostty-voice

Voice dictation that types clean English prose into the focused **Ghostty** terminal on
**GNOME/Wayland**, so you can speak instructions to a coding agent running over SSH. Whisper
runs **locally** on the workstation GPU (whisper.cpp + Vulkan, `large-v3`); the transcript is
injected as if typed from the keyboard. The text is **never auto-submitted** — you review
before pressing Enter.

See `PLAN.md` for the full design, `CONTEXT.md` for the domain language, and `docs/adr/` for
the load-bearing decisions.

## Architecture

Three processes; the daemon owns all state:

- **`whisper-server`** — whisper.cpp built with `-DGGML_VULKAN=1`, model held warm on the GPU,
  supervised as a child of the daemon.
- **`ghostty-voiced`** — the daemon (one systemd **user** service): supervises whisper-server,
  listens on a Unix socket, records, transcribes, injects, manages caches.
- **`ghostty-voice-ctl`** — thin client spawned by each GNOME hotkey.

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
2. **Model** (~3 GB, not packaged) — fetched on first run into
   `~/.local/share/ghostty-voice/models/ggml-large-v3.bin`.
3. **Config** — copy `config.toml.example` to `~/.config/ghostty-voice/config.toml` and edit.
4. **Injection environment** — `ydotoold` must be running and you must have `/dev/uinput`
   access. Diagnose with:
   ```sh
   ghostty-voice-ctl doctor
   ```
5. **Hotkeys** — install the GNOME custom keybindings (Super+D toggle, Super+Shift+D vad,
   Super+Ctrl+D continuous, Super+Alt+D cancel):
   ```sh
   ghostty-voice-ctl install-hotkeys
   ```
6. **Enable the daemon**:
   ```sh
   systemctl --user enable --now ghostty-voiced
   # For dictation before/without a graphical login:
   loginctl enable-linger "$USER"
   ```

## Usage

- **Toggle** (`Super+D`): press to start recording, press again to stop → transcribe → type.
- **VAD** (`Super+Shift+D`): press to start; `sox` auto-stops on the first trailing silence,
  then transcribes → types. Hands-free, single utterance.
- **Continuous** (`Super+Ctrl+D`): the north-star, hands-free long-form mode. Talk naturally
  with pauses; short pauses cut the audio into **clips** that batch-transcribe in the
  background (context-chained), and a long silence (~10 s) ends the **session** — the
  assembled transcript is then typed once. `cancel` aborts the whole session. Tune the
  segmentation with `clip_cut_pause_seconds` / `session_end_silence_seconds` /
  `min_clip_seconds` in `[audio]`.
- **Cancel** (`Super+Alt+D`): abort the current recording (or the whole continuous session).
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
- **Dropped characters** — raise `[inject].key_delay_ms`.

## Status

Built and tested in CI-equivalent (pure logic + mock-server / fake-socket integration).
**On-hardware end-to-end** (real GPU transcription, mic capture, ydotool injection) requires
the workstation and is validated manually — see the build/first-run steps above.
