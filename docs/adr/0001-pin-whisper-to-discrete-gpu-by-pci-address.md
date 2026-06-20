# Pin whisper-server to the discrete GPU by PCI address

This workstation exposes **two** Vulkan (RADV) devices: the discrete RX 6900 XT
(`0000:03:00.0`) and the 7950X's integrated Raphael GPU (`0000:1a:00.0`).
whisper.cpp/ggml-vulkan selects a device by enumeration **index**
(`GGML_VK_VISIBLE_DEVICES`) and will **silently** run on the wrong device — the iGPU,
which lacks the VRAM for `large-v3` — if not pinned. We therefore pin the device
explicitly and verify it at load.

The config key `[whisper].vulkan_device` is a **PCI address** (default `0000:03:00.0`),
not an index, name substring, or Vulkan UUID. The daemon resolves PCI → enumeration index
at startup and sets `GGML_VK_VISIBLE_DEVICES` internally; the env var is hidden plumbing,
never user-facing. At load it **asserts** the selected device's reported name, refusing to
go ready (with `notify-send`) if the wrong GPU loaded.

## Considered options

- **Integer index** — rejected: Vulkan device ordering is not guaranteed stable across
  driver/kernel updates; a `mesa` bump could renumber the iGPU to 0 and silently mispoint.
- **Device-name substring** (`"RX 6900 XT"`) — works today (the two device names differ
  wildly) but cannot disambiguate two identical cards.
- **Vulkan `deviceUUID`** — stable and unique, but under RADV it is *literally the PCI
  address* zero-padded into UUID shape (`00000000-0300-...` ⇒ `03:00.0`); less legible than
  the PCI address it encodes. (`driverUUID`/`pipelineCacheUUID` are identical across both
  devices and useless for disambiguation.)
- **PCI address** — chosen: the genuine stable hardware identifier the UUID is derived
  from, human-findable via `lspci` and `/dev/dri/by-path/`, and disambiguates identical
  cards.

## Consequences

- The daemon needs a PCI → index resolver (match against `deviceUUID` / enumeration order)
  plus a load-name assertion backstop. This is M1 work and is the cleanest "model loaded on
  the right GPU" readiness signal (stronger than a bare HTTP 200).
