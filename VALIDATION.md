# Validating ghostty-voice on the workstation

Tailored to this box: Arch Linux, GNOME/Wayland, AMD **RX 6900 XT** (RADV Vulkan) at PCI
`0000:03:00.0`, plus the Raphael iGPU at `0000:1a:00.0`. You're already in the `input` group
and `/dev/uinput` exists.

Work top-to-bottom. **Part A** validates the whole foundation **without** building whisper
(fast, ~10 min). **Part B** is the full speech→text end-to-end (needs whisper.cpp + the model).

---

## 0. Install / build prerequisites

### 0.1 System packages

Already present: `pipewire`/`pw-record`, `ydotool`/`ydotoold`, `wl-clipboard`, `libnotify`,
`gsettings`, `vulkan-icd-loader`+RADV, Rust toolchain.

Install what's missing:

```sh
sudo pacman -S --needed cmake git base-devel sox \
  vulkan-headers spirv-headers spirv-tools glslang shaderc
```

(`sox` is only needed for VAD / Continuous mode — Part B's S5/S6 — but install it now.
`spirv-headers`/`spirv-tools`/`glslang`/`shaderc` are the Vulkan **shader toolchain** that
whisper.cpp's `ggml-vulkan` needs at build time — without `spirv-headers` the cmake configure
fails with `Could not find ... SPIRV-Headers`.)

### 0.2 Build the Rust workspace

```sh
cd ~/Development/JoelLarson/ghostty-voice
cargo build --release
# binaries: target/release/{ghostty-voice, ghostty-voiced, ghostty-voice-ctl}
```

### 0.3 Build whisper.cpp with Vulkan (needed for Part B only)

```sh
cd ~/Development/JoelLarson/ghostty-voice
git clone --depth=1 https://github.com/ggerganov/whisper.cpp
cmake -S whisper.cpp -B whisper.cpp/build -DGGML_VULKAN=1 -DCMAKE_BUILD_TYPE=Release
cmake --build whisper.cpp/build -j --target whisper-server
# → whisper.cpp/build/bin/whisper-server
./whisper.cpp/build/bin/whisper-server --help    # CONFIRM the flags: --model, --host, --port
```

> If `--help` shows different flags than `--model/--host/--port`, tell me — the daemon's
> launch args and the config are easy to adjust.

### 0.4 Download the model (~3 GB; auto-download is NOT yet wired — S7 gap)

```sh
mkdir -p ~/.local/share/ghostty-voice/models
( cd whisper.cpp && ./models/download-ggml-model.sh large-v3 )
cp whisper.cpp/models/ggml-large-v3.bin ~/.local/share/ghostty-voice/models/
```

### 0.5 Config

```sh
mkdir -p ~/.config/ghostty-voice
cp config.toml.example ~/.config/ghostty-voice/config.toml
# defaults already target the 6900 XT (vulkan_device = "0000:03:00.0") and port 8910.
```

### 0.6 Start ydotoold (for any injection)

```sh
export YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket"
ydotoold -p "$YDOTOOL_SOCKET" &      # leave running; needs /dev/uinput (you have access)
# verify:
ls -l "$YDOTOOL_SOCKET"
YDOTOOL_SOCKET="$YDOTOOL_SOCKET" ydotool type "ydotool works"   # should type into focused field
```

Put `export YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket"` in your shell rc so the daemon
and `ydotool` agree on the path.

---

## Part A — validate the foundation WITHOUT whisper (do this first)

### A1. Test suite passes on your hardware

```sh
cargo test
```
**Expect:** every suite `ok`, ~90 unit + 3 integration tests, 0 failed.

### A2. `doctor` — injection environment

```sh
./target/release/ghostty-voice-ctl doctor
```
**Expect:** `ok input group`, `ok uinput device`, and `ok ydotoold socket` **if** you did 0.6
(otherwise it correctly FAILs that line).

### A3. GPU pinning — the scariest unverified logic (no whisper needed)

```sh
./target/release/ghostty-voiced
```
**Expect in the log, before it fails to spawn whisper:**
```
pinning whisper-server to Vulkan device 0 (0000:03:00.0)
```
✅ `device 0` / `0000:03:00.0` means the `vulkaninfo` parse + deviceUUID→PCI + resolver
correctly picked the **6900 XT**, not the iGPU. (If it says device 1 or the iGPU address,
that's a real bug — tell me.) It will then loop `failed to spawn whisper-server` — expected,
leave it running for A4.

### A4. Socket + state machine over the real Unix socket

With `ghostty-voiced` still running from A3, in another terminal:
```sh
./target/release/ghostty-voice-ctl status    # expect: ok loading
./target/release/ghostty-voice-ctl toggle     # expect: err model still loading
```
✅ Proves the wire protocol + state machine work over the real socket. Stop the daemon with
Ctrl-C (watch for `shutting down — freeing VRAM`).

### A5. install-hotkeys — real GNOME gsettings

```sh
./target/release/ghostty-voice-ctl install-hotkeys
gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings
```
**Expect:** the output includes `.../ghostty-voice-toggle/` and `.../ghostty-voice-cancel/`,
merged with any pre-existing customN entries (none clobbered). (The bindings fire
`ghostty-voice-ctl`, so install it on PATH — `sudo install -Dm755 target/release/* /usr/bin/`
— or expect "command not found" when you press the keys until then.)

**If Part A is all green, the S1/S2 foundation is sound on your hardware.**

---

## Part B — full speech→text end-to-end (needs Part 0.3/0.4)

### B1. S1 walking skeleton (you start whisper manually)

Terminal 1 — start the warm server, pinned to the 6900 XT:
```sh
GGML_VK_VISIBLE_DEVICES=0 \
  ./whisper.cpp/build/bin/whisper-server \
  --model ~/.local/share/ghostty-voice/models/ggml-large-v3.bin \
  --host 127.0.0.1 --port 8910
```
Watch its startup log: it should report loading on the **RX 6900 XT** (not CPU/iGPU). Wait
until it says it's listening.

Terminal 2 — dictate one utterance:
```sh
export YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket"
./target/release/ghostty-voice
# → "● Recording — press Enter to stop." Speak a sentence with jargon
#   ("rebase onto main, then run kubectl"), press Enter.
```
**Expect:** the transcript prints, a `transcribed in N.NNs` line appears (your **warm-latency
number** — note it), and the text is typed into whatever window is focused. Keep a text field
focused to see it land. ✅ This proves Vulkan transcription **and** ydotool injection together.

> Watch for: dropped/reordered characters (raise `[inject].key_delay_ms`), a giant latency
> (wrong device — check the server log), or a `[BLANK_AUDIO]`-type result on silence.

### B2. S2 daemon + hotkey

Put `whisper-server` on PATH (or set `[whisper].binary` to the full path):
```sh
sudo install -Dm755 whisper.cpp/build/bin/whisper-server /usr/bin/whisper-server
sudo install -Dm755 target/release/ghostty-voiced  /usr/bin/ghostty-voiced
sudo install -Dm755 target/release/ghostty-voice-ctl /usr/bin/ghostty-voice-ctl
```
Run the daemon (foreground for now, so you see logs):
```sh
ghostty-voiced
# wait for: "whisper-server ready"
```
Another terminal (or the Super+D hotkey from A5):
```sh
ghostty-voice-ctl status     # expect: ok idle
ghostty-voice-ctl toggle      # start recording (speak)
ghostty-voice-ctl toggle      # stop -> transcribe -> type into focused window
```
**Expect:** `ok idle` → `ok recording` → `ok transcribing`, then the text types in and state
returns to idle. ✅ Full S2 path.

Free the VRAM when done:
```sh
# Ctrl-C the foreground daemon, or once enabled as a service:
systemctl --user stop ghostty-voiced
```

---

## What is NOT yet wired (so you don't test for it)

These pure modules are built + unit-tested but **not yet integrated into the daemon runtime**
(deferred until this foundation is validated):

- **S3** ordered delivery queue, cache-before-type, `replay-last`, audio cues.
- **S4** accuracy stack *applied* to the request (`initial_prompt`, corrections, `beam-8`,
  filtering) — B1/B2 currently transcribe with whisper defaults.
- **S5** VAD mode (`Super+Shift+D`).
- **S6** Continuous mode (talk-with-pauses).
- **S7** first-run model auto-download (you placed the model manually in 0.4).

## Report back

For each of A1–A5 and B1–B2: ✅ / ❌ + any log lines that look wrong. Especially:
1. A3's `pinning ... device N` line (must be the 6900 XT).
2. B1's whisper-server startup device + your warm-latency number.
3. Any character-drop behavior in B1/B2 (informs the `key_delay_ms` default).

With that, I can fix anything the hardware surfaces, then wire S3–S6 into the daemon on a
trusted foundation.
