# talk-to is the sole interface: no global input capture, no focused-window typing

`talk-to` — the PTY wrapper that registers as a **wrapper sink** and injects transcripts
straight into a wrapped agent's pipe — has become the primary way the tool is used. Two
earlier desktop-integration paths predate it and are now redundant surface area and risk:

1. **A system-wide trigger listener.** The daemon opened a raw `/dev/input` evdev keyboard
   and reacted to Shift+F9/F10 *everywhere*, regardless of focus — keylogger-grade
   capability that fired even when the user was not in a dictation session.
2. **A focused-window delivery sink.** Finished transcripts were typed into *whatever
   window was focused* via `ydotool`, guarded by a time-based **Freshness window** because
   the daemon has no window identity — the documented "wrong-window" risk.

We make `talk-to` the **sole interface** for both triggering and delivery, and remove both
global paths.

- **Triggers move into `talk-to`.** It intercepts the Shift+F10 / Shift+F9 terminal escape
  sequences in its proxy loop (as it already does for the F12 debug key) and sends a daemon
  command over the control socket. Triggers therefore fire **only while the user is in the
  `talk-to` window**. A terminal reports key *presses* only — there is no release/hold
  timing — so the tap/hold/PTT/VAD gesture model collapses to discrete commands:
  **Shift+F10 = `toggle`** (start/stop batch recording), **Shift+F9 = `vad`** (hands-free,
  auto-stops on silence). `cancel` stays on `ghostty-voice-ctl`.
- **The focused-window sink is removed entirely.** No `ydotool`, no Freshness window, no
  "wrong-window" risk. The **wrapper sink is the only Delivery sink**; with no `talk-to`
  registered there is no active sink, so a triggered utterance is **Held-for-replay** and
  `replay-last` re-delivers it to the active wrapper (never to a focused window).

This reverses the global-evdev decision recorded for the earlier "remove the GNOME hotkey
path" work: that step traded GNOME `gsettings` hotkeys for a compositor-independent global
evdev listener so triggers worked anywhere; with `talk-to` as the interface, "anywhere" is
no longer wanted — triggers should fire only where dictation is happening.

## Consequences

- The daemon no longer opens any `/dev/input` device and no longer runs `ydotool`. The
  pure tactile modules (`input` key-tracker, `gesture` mapper, `key_combo` parser), the
  `[input]` config section, the `ghostty-voice-ctl bind` flow, and the `[inject]` config
  are deleted. Runtime deps drop `ydotool`/`ydotoold`.
- Tap/hold/push-to-talk semantics are **not** available inside a terminal (no key-release
  events). Recovering them later would require the Kitty keyboard protocol's key-release
  reports — deliberately out of scope.
- With no system-wide capture, the only way text reaches an agent is through a `talk-to`
  PTY the user is actively using — a smaller, clearer security and routing surface.
- The delivery model simplifies: one sink kind, no time-based staleness gate, no
  compositor-introspection caveats. `Held-for-replay` now means only "the bound wrapper is
  gone (or none was registered)."
