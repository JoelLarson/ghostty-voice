# ghostty-voice

Voice dictation that types clean English prose into a coding agent you run inside **`talk-to`**,
so you can speak instructions to an agent running over SSH. Whisper runs **locally** on the
workstation GPU (whisper.cpp + Vulkan, `large-v3`); the transcript is delivered straight into the
wrapped agent's PTY. The text is **never auto-submitted** — you review before pressing Enter.

`talk-to` is the **sole interface**: both triggering and delivery go through the wrapper you are
actively using. There is **no system-wide hotkey** and **no typing into the focused window**.

See `PLAN.md` for the full design, `CONTEXT.md` for the domain language, and `docs/adr/` for
the load-bearing decisions (`docs/adr/0003` records this interface model).

## Architecture

Three processes plus the wrapper; the daemon owns all state:

- **`whisper-server`** — whisper.cpp built with `-DGGML_VULKAN=1`, model held warm on the GPU,
  supervised as a child of the daemon.
- **`ghostty-voiced`** — the daemon (one systemd **user** service): supervises whisper-server,
  listens on a Unix socket, records, transcribes, and pushes transcripts to the active wrapper
  sink. It owns **no input device** — triggering happens in `talk-to`.
- **`talk-to`** — the PTY wrapper and **the interface**: `talk-to <command>` wraps an agent on a
  pseudo-terminal, registers as a **wrapper sink**, recognizes the trigger keys in its own proxy,
  and injects finished transcripts into the agent's PTY.
- **`ghostty-voice-ctl`** — thin client for one-shot commands (`cancel`, `status`, `reload`,
  `replay-last`) and `doctor`.

## Build

```sh
cargo build --release          # the four binaries land in target/release/
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
   `[whisper].model_path` is absent, it enters the `downloading` **State** and streams
   `ggml-large-v3.bin` from `[whisper].model_url` (HuggingFace by default) into
   `~/.local/share/ghostty-voice/models/`. Progress is reported live from one source of truth:
   `ghostty-voice-ctl status` shows `downloading <pct>` (e.g. `downloading 42`) and a running
   `talk-to`'s bottom **status strip** shows `downloading <pct>%` (e.g. `downloading 42%`),
   advancing on whole-percent changes — a bare `downloading` until the server reports a total
   size. Download progress is **not** sent via `notify-send`; the start/progress/completion/
   failure milestones stay in the journald log (`journalctl --user -u ghostty-voiced`). While
   downloading, `toggle`/`vad`/`continuous` are rejected with
   "model still downloading" — the daemon never hangs. The fetch is SHA-256 verified if you pin
   `[whisper].model_sha256` (copy the hash from the HuggingFace LFS page); leave it empty to
   accept by presence. A failed/corrupt fetch is discarded and retried with backoff.
3. **Config** — copy `config.toml.example` to `~/.config/ghostty-voice/config.toml` and edit.
4. **Enable the daemon** (it fetches the model on first run):
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
5. **Diagnose** that the daemon is reachable:
   ```sh
   ghostty-voice-ctl doctor
   ```

## Configuration keys

All keys live in `~/.config/ghostty-voice/config.toml` (see `config.toml.example`). `reload`
hot-applies non-model fields (`ghostty-voice-ctl reload`). Config parsing is **strict**: an
existing config that is malformed or carries an unknown key (a typo, or a section removed in a
newer version) makes the daemon **refuse to start** — and `reload` reject the change, keeping the
running config — rather than silently using defaults. A *missing* config is fine (defaults apply).
The slice each key belongs to:

- `[whisper]` — `model_path`, `model_url`, `model_sha256` (first-run download, S7),
  `vulkan_device` (GPU pin, S2/ADR-0001), `beam_size`, `temperature`, `prompt_prefix`, `vocab`
  (accuracy stack, S4).
- `[audio]` — `max_recording_seconds` (runaway cap, S3), `min_duration_seconds` (S4),
  `vad_silence_seconds` / `vad_threshold_pct` (VAD, S5), `clip_cut_pause_seconds` /
  `session_end_silence_seconds` / `min_clip_seconds` (Continuous mode, S6).
- `[feedback]` — `sound_start` / `sound_stop` (cues, S7): a freedesktop theme event id (default)
  or a sound-file path — both played via `paplay`; empty disables.
- `[cache]` — `wav_keep`, `transcript_keep`, and `retry_window_seconds` (how long the daemon
  keeps retrying a transcription while whisper-server is unreachable before holding the utterance).
- `[corrections]` — the jargon spell-fixer table (S4).

There is no trigger configuration: the keys are fixed (see **Usage**) and recognized inside
`talk-to`.

## Usage

Run your agent inside `talk-to`. SSH is just a command:

```sh
talk-to claude
talk-to ssh host claude
```

**While you are in the `talk-to` window**, two keys drive recording. A terminal reports key
*presses* only (no hold/release timing), so these are discrete commands — there is no tap/hold or
push-to-talk:

- **Shift+F10** — **toggle**: start a batch recording; press again to stop and transcribe.
- **Shift+F9** — start a hands-free **VAD** recording: `sox` auto-stops on the first trailing
  silence, then transcribes.

The transcript is delivered into the wrapped agent's PTY with **no trailing Enter** — you review
and press Enter yourself. Triggers fire **only** in the `talk-to` window; nothing happens when
you are focused elsewhere.

Other commands go through `ghostty-voice-ctl`:

- **Cancel** the current recording: `ghostty-voice-ctl cancel`.
- **Continuous mode** (the north-star long-form mode): `ghostty-voice-ctl continuous`. Short
  pauses cut the audio into **clips** that batch-transcribe in the background (context-chained),
  and a long silence (~10 s) ends the **session** — the assembled transcript is delivered once.
  Tune the segmentation with `clip_cut_pause_seconds` / `session_end_silence_seconds` /
  `min_clip_seconds` in `[audio]`.
- **Status**: `ghostty-voice-ctl status` reports the daemon state and how many **wrapper sinks**
  are registered:
  ```
  ok idle wrappers=0     # no talk-to running — nowhere to deliver
  ok idle wrappers=1     # a talk-to wrapper sink is the active target
  ```
  The `wrappers=` field is additive — an older daemon answers a bare `ok <state>`.
- **Replay**: `ghostty-voice-ctl replay-last` re-delivers the most-recent cached transcript into
  the active wrapper sink (errors if no `talk-to` is registered).
- **Disable / free the 16 GB VRAM**: `systemctl --user stop ghostty-voiced` (cascades to
  whisper-server).

## Delivery sinks — where dictation goes

A `talk-to` is a **wrapper sink**: the daemon pushes finished transcripts down its connection and
`talk-to` writes them into the wrapped agent's PTY — a known pipe, no "wrong-window" risk. The
wrapper sink is the **only** kind of sink; with no `talk-to` running there is **no active sink**
and nowhere to deliver.

**Verify where dictation is going** without reading logs:

```
ok idle wrappers=1     # routing into a talk-to-wrapped agent
ok idle wrappers=0     # no talk-to — a triggered utterance would be held for replay
```

For a per-delivery trace, the daemon log records `delivered to wrapper sink SinkId(N)`:

```sh
journalctl --user -u ghostty-voiced -f
```

**Trigger-time binding (why your text never lands in the wrong place).** An utterance's target
sink is **bound when you trigger the recording**, not when its transcript is ready. It is **never
silently redirected**: if the bound **wrapper sink** has exited by the time the transcript is
ready, the transcript is **Held-for-replay** (cached, recoverable with
`ghostty-voice-ctl replay-last`), never handed to whatever is active now.

**Running several `talk-to` sessions.** Launching another `talk-to` makes the newest wrapper sink
the active one. If you close the **active** wrapper while others are still running, the active
sink **hands off to the most-recently-registered still-live wrapper** (the *newest-live handoff*).
There is no active sink only when the **last** wrapper exits. (An utterance already bound to a
now-closed wrapper is still Held-for-replay, never handed to the survivor.)

**After upgrading the package, restart the daemon.** A running `ghostty-voiced` keeps the *old*
binary in memory until restarted; a stale daemon speaks an older control protocol and refuses the
wrapper-sink handshake, so `talk-to`'s strip shows **`incompatible`** and dictation will not reach
the wrapped agent until you restart:

```sh
systemctl --user restart ghostty-voiced
```

**Strip / link states.** While registered, `talk-to`'s bottom strip shows the daemon voice
**State** (`idle`/`recording`/`transcribing`, and on first run `downloading <pct>%` as the model
fetches). Otherwise it shows a distinct link token — `unreachable` (no daemon), `incompatible`
(stale/old daemon — restart it, see above), `rejected` (registration refused), or `dropped` (a
previously-good connection ended). In every non-registered case `talk-to` keeps working as a plain
passthrough; the reason is logged to `~/.local/state/ghostty-voice/talk-to.log` (`$XDG_STATE_HOME`
if set).

## Troubleshooting

- **Nothing delivered** — `ghostty-voice-ctl status` must show `wrappers>=1`; if it shows
  `wrappers=0` there is no `talk-to` registered and a triggered utterance is held. A transcript is
  cached to `$XDG_CACHE_HOME/ghostty-voice/transcripts/` *before* delivery, so it is never lost
  even if a PTY write fails — recover it with `ghostty-voice-ctl replay-last`. WAV recordings are
  kept under `recordings/` (count-capped) as a debugging corpus.
- **Triggers do nothing** — they fire only while you are in the `talk-to` window (Shift+F10 =
  toggle, Shift+F9 = VAD). Confirm the daemon is reachable with `ghostty-voice-ctl doctor`, and
  that the strip shows a daemon state rather than a link token (see above).
- **Wrong GPU / slow** — check the daemon log (`journalctl --user -u ghostty-voiced`) for the
  `pinning whisper-server to Vulkan device N` line; confirm it's your discrete card.
- **Vulkan not used (silent CPU fallback)** — confirm RADV sees your card with
  `vulkaninfo --summary` (from `vulkan-tools`); the discrete GPU must appear. whisper-server is
  pinned via `GGML_VK_VISIBLE_DEVICES`; a missing Vulkan ICD loader (`vulkan-icd-loader`, a
  package dependency) makes it fall back to CPU.
- **Stuck in `downloading`** — `ghostty-voice-ctl status` shows `downloading <pct>` (and a
  `talk-to` strip shows `downloading <pct>%`) until the ~3 GB model lands; watch the strip or
  `status`, or tail `journalctl --user -u ghostty-voiced` (progress is logged there, not sent as
  notifications). A corrupt fetch (SHA mismatch when `model_sha256` is pinned) is discarded and
  retried.
- **`talk-to` strip shows a connection problem** (`unreachable` / `incompatible` / `rejected` /
  `dropped`) — see the **Delivery sinks** section above. Most often `incompatible` after a package
  upgrade: `systemctl --user restart ghostty-voiced`.

## Status

Built and tested in CI-equivalent (pure logic + mock-server / fake-socket integration).
**On-hardware end-to-end** (real GPU transcription, mic capture, PTY delivery) requires the
workstation and is validated manually — see the build/first-run steps above.
