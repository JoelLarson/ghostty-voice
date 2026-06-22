# IDEAS

Candidate directions not yet committed to `PLAN.md`. Each is a sketch, not a spec — the
point is to capture intent and the open questions before any slice is cut. Comparison
seed: [OpenSuperWhisper](https://github.com/Starmel/OpenSuperWhisper) (macOS dictation app)
surfaced a few things worth adapting to our Linux/Wayland, terminal-focused world.

Ground rules these ideas inherit from the existing design:

- **Local-only.** Whisper runs on the workstation GPU; no transcript leaves the machine.
  Any new processing stage must hold that line.
- **Never auto-submit.** Whatever we add, the human still reviews and presses Enter.
- **The daemon owns state.** New surfaces (UI, post-processing) read from / hook into the
  daemon; they don't grow a second source of truth.

---

## 1. LLM cleanup pass (extends the accuracy stack, S4)

**What:** an optional post-transcription stage that rewrites the raw **Transcript** into
clean English prose before it enters the **Delivery queue** — fixing run-ons, filler
("um", "you know", false starts), and disfluencies that Whisper faithfully transcribes but
that read badly as an instruction to a coding agent. This is the literal payoff of the
project's tagline ("clean English prose"), currently carried only by Whisper + the
**Correction Dictionary**.

**Why it fits us better than OpenSuperWhisper:** OSW has no LLM cleanup and lists "Agent
mode" as an open TODO. Our whole reason to exist is feeding a coding agent — prose that's
already structured ("create a function that…", not "uh so like make a— make a function")
is the differentiator that's natural to our niche and nobody adjacent is building.

**Where it sits in the pipeline:** `record → transcribe → [correction dict] → [LLM pass]
→ type`. It runs *after* the deterministic Correction Dictionary (so jargon/vocab fixes
are already applied and the LLM sees correct spellings), and *before* Auto-type. The
cached transcript safety net (write-before-type) should cache **both** the raw and the
cleaned text, so `replay-last` can fall back to raw if a cleanup pass goes wrong.

**Open questions:**

- **Local vs. external.** Local-only is the project's spine. A small instruction-tuned
  model (llama.cpp on the same Vulkan device, or sharing VRAM with whisper-server) keeps
  the guarantee but competes for the 16 GB. An external API would be faster/better but
  breaks "no transcript leaves the machine" — likely a hard no, or at most strictly
  opt-in with a loud config flag.
- **VRAM coexistence.** whisper-server already holds `large-v3` warm. A second warm model
  may not fit; options are a smaller cleanup model, load-on-demand, or time-sharing.
- **Latency budget.** A cleanup pass adds seconds. Acceptable for Toggle/VAD; needs care
  in Continuous mode where clips batch-transcribe in the background — does cleanup run
  per-clip or once over the assembled session transcript? (Probably once, at session end.)
- **Prompt as config.** The rewrite instruction ("clean dictation into a terse coding
  instruction; preserve meaning; don't answer it") belongs in config, like
  `[whisper].prompt_prefix`. Per-mode prompts? A "verbatim" escape hatch?
- **Default off.** Ships disabled; the deterministic Correction Dictionary stays the
  baseline. `[llm]` (or similar) slice, hot-reloadable like the rest.
- **Determinism risk.** An LLM can change meaning or hallucinate. Mitigations: low
  temperature, length guards (reject output wildly longer/shorter than input → fall back
  to raw), and the raw-text cache as the always-available undo.

---

## 2. Live status surface — what the daemon is doing, visibly

**What:** a glanceable, always-available indicator of daemon state — `idle`,
`recording`, `transcribing`, `typing`, `downloading`, `error` — plus transient detail
(recording elapsed time, queue depth, download %). OpenSuperWhisper does this with a
recording popup + a menu-bar item; we want the same *legibility* without inheriting a
desktop-environment dependency (we deliberately dropped GNOME/libcanberra, S8/ADR).

**Why:** today the daemon's state is only observable by running `ghostty-voice-ctl
status` or tailing the journal. For a push-to-talk / latch / VAD tool you use dozens of
times an hour, "is it listening right now?" and "did it finish?" need to be answerable at
a glance, not by typing a command. The audio cues (S7) cover *events*; this covers
*state*.

**The data already exists.** The daemon owns the state machine and exposes it over the
Unix socket (`status`). This is a presentation problem, not a state problem — the surface
subscribes to / polls the socket and renders. Keep the daemon authoritative; the indicator
is a thin reader, same as `ghostty-voice-ctl`.

**Candidate surfaces (compositor-agnostic is the constraint, like the evdev triggers):**

- **Status-bar module.** Emit machine-readable status the user pipes into Waybar / a
  custom module (the daemon already speaks over the socket; add a `status --watch` /
  streaming mode or a tiny JSON line protocol). Cheap, fits tiling-WM users, no GUI
  toolkit pulled in. Most idiomatic for our likely audience.
- **Layer-shell overlay.** A small always-on-top "● recording 0:04" pill via
  `wlr-layer-shell` — the true analogue of OSW's recording popup, works on any wlroots
  compositor, independent of any DE. Heavier (a rendering dependency), but the most
  *visible*.
- **Notifications (interim).** We already `notify-send` download progress; state
  transitions could reuse that. Cheapest, but notifications are transient and noisy for a
  high-frequency state — a stopgap, not the destination.

**Open questions:**

- Streaming protocol on the socket vs. poll-on-interval — recording elapsed time wants
  push or a fast poll.
- Which surface ships first? (Leaning status-bar module: smallest footprint, matches the
  tiling-WM / terminal user, no new GUI dependency.) Layer-shell overlay as the richer
  follow-up.
- Does this subsume the audio cues or complement them? (Complement — eyes vs. ears.)

---

## 3. Multiple transcription engines — parked

OpenSuperWhisper ships Whisper **and** Parakeet (moonshine / SenseVoice on branches).
Genuinely nice for speed/accuracy trade-offs. **Not a focus this round** — we're
Whisper-`large-v3` on Vulkan and that's the right single bet for now. Noted so the
recording layer and config keep the *seam* open (engine choice is a `[whisper]`-adjacent
concern, not hard-wired into the daemon's pipeline) rather than committing to the work.

---

## 4. `talk-to` — a PTY wrapper that injects voice into a wrapped agent

**What:** a foreground TUI launcher, `talk-to <command>` (e.g. `talk-to claude`, or
`talk-to ssh host claude`), that spawns the agent as its child on a pseudo-terminal,
passes the child's TUI straight through to the focused Ghostty window, and uses the PTY
master as a side channel: when a transcription finishes, the text is written into the
child's input **exactly as if typed** — no trailing `\r`, so the human still reviews and
presses Enter. A reserved status line shows live voice state (`idle` / `recording` /
`transcribing`).

**Why it fits us:** this is a *cleaner delivery surface* than evdev/`ydotool` injection
for the common case of "I'm talking to one coding agent." Injection is **deterministic** —
text goes into *this* agent, not whatever window happens to be focused — and it needs no
`/dev/uinput` for delivery (the PTY write replaces the synthetic keystroke). It pairs
naturally with the live-status surface (#2) and the LLM cleanup pass (#1), which both want
a single, known target.

**SSH is not a problem.** The wrapper wraps whatever command it's given, and
`ssh host claude` is just a command. Injected bytes flow `PTY master → ssh stdin → over
the existing SSH pipe → remote PTY → claude stdin`; the rendered TUI returns over ssh
stdout and is painted locally. No remote wrapper, no socket forwarding, no new transport.

**The status line must be a *bottom* strip — this is load-bearing, not cosmetic.** A
foreground proxy can forward the child's bytes **verbatim** only while the child's
coordinate space stays corner-aligned with the real screen: same origin (1,1), same
width, height ≤ available. Reserving the **bottom** row(s) and setting the child winsize to
`(H−1, W)` preserves all three — the child cannot address the row we own, so we forward
untouched and paint the strip ourselves. A *top* strip moves the origin and a *side* pane
changes the width; either one desyncs the child's absolute escape codes from the screen,
and the only fix is to run a full VT emulator on the child's output and composite it
(i.e. reimplement the expensive 80% of tmux/zellij). So: bottom strip = nearly free;
anything else = an embedded terminal emulator. We are **not** building a multiplexer.

**Where it sits in the architecture:** `talk-to` is a **daemon client**, like
`ghostty-voice-ctl` — it grows no second source of truth. It subscribes to daemon state
over the Unix socket (renders the strip) and receives the finished transcript (writes it
to its child PTY). The daemon keeps owning whisper, recording, the cache, and the state
machine. The everyday evdev trigger (Shift+F10/F9) can stay as-is; `talk-to` owns the PTY
and the strip, not the hotkey.

**v1 — the one happy path (build this first):**

`talk-to ssh host claude` is running → you press the evdev hotkey (or call
`ghostty-voice-ctl`) → the daemon records and transcribes on the desktop GPU → the
transcript is **pushed** to the wrapper sink and typed into claude **with no
intervention**. That single path is the whole v1. Scope:

- **PTY proxy + bottom status strip.** Spawn the child on a PTY, forward bytes verbatim
  with child winsize `(H−1, W)`, paint voice state in the reserved bottom row. (A top strip
  or side pane would force a VT emulator — see the geometry note above; not built.)
- **Registered push-sink.** `talk-to` opens a *persistent* connection to the control
  socket, registers, and the daemon pushes the finished **Transcript** down it; `talk-to`
  writes it into the child PTY (no trailing `\r` — review-before-Enter survives).
- **Lifecycle switching only.** Launch `talk-to` → its **wrapper sink** is active; exit →
  back to the **focused-window sink** (today's `ydotool` behavior, untouched). One sink
  active at a time. No explicit switch command in v1.
- **Works over SSH** with no extra machinery — `talk-to ssh host claude`; injected bytes
  ride the existing ssh stdin pipe to the remote claude.
- **Dead wrapper → Held-for-replay.** If `talk-to` dies before delivery, the transcript is
  cached and recoverable via `replay-last` — never redirected into the current focus. No
  new recovery UI in v1.

**The framing this rests on (kept in CONTEXT.md):** delivery is a single **active Delivery
sink**; `ydotool` is the *focused-window sink*, `talk-to` a *wrapper sink*; an utterance
binds its target at trigger time and is never silently redirected.

**Deferred — keep the seam open, *not rejected*:**

- **Compositor introspection** to give the *focused-window* sink the same strong
  "bound-target / hold-and-ask" guarantee the wrapper sink has natively. Left out of v1 to
  stay simple — a deliberate **deferral**, not a rejection of the DE-agnostic question.
  Until then the focused-window sink stays best-effort via the **Freshness window**.
- **Explicit `ghostty-voice-ctl sink <target>`** mid-session switching (lifecycle-implicit
  is enough for v1).
- **Transcript-history surface** — list cached transcripts newest→oldest, route any to any
  sink; the generalization of **Replay-last** and the "ask me where to send it" UI.
- **Read-back** — the wrapper writes *in*; it does not parse the child's output stream.
  Revisit only if a concrete need (spoken responses, cleanup context) justifies an emulator.

**Still open (execution detail, for the PRD / slicing):**

- **Passthrough fidelity.** Raw-mode forwarding, `SIGWINCH`/resize (recompute child winsize
  to `H−1`), signal forwarding (Ctrl-C to the child, not the wrapper). The real engineering.
- **Protocol payload shape.** The pushed-transcript message + `register-sink` command — the
  point at which the "deliberately dumb" line protocol finally needs structure (JSON?).
- **Cache safety net.** Unchanged — write-before-deliver caches the transcript so a crashed
  `talk-to` loses nothing.

---

## Explicitly out of scope

- **GUI onboarding / settings app.** OpenSuperWhisper's menu-bar config UI and in-app
  model manager are deliberately *not* a goal here — separate plans exist for the
  onboarding experience. The CLI + `doctor` + config file remain the control surface.
