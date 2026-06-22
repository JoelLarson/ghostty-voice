# ghostty-voice

Voice dictation that types clean English prose into the focused Ghostty terminal on
GNOME/Wayland, so you can speak instructions to a coding agent running over SSH. The
agent's own voice features can't cross SSH, so Whisper runs locally and the text is
injected as if typed from the keyboard.

**North-star:** fully **hands-free**, **conversational** dictation — talk naturally with
pauses, words flow in behind you, a long silence ends it. That experience is **Continuous
mode**; batch **Toggle**/**VAD** are the foundation built and validated on the way there,
not the destination. Build order is forced by dependencies (Continuous mode sits atop
recording + supervision + injection + accuracy), but the project isn't "done" until
Continuous mode lands. Earlier milestones must keep the seams open for it — chiefly, the
recording layer must allow continuous-capture-with-segmentation, not hard-wire
one-file-per-utterance.

## Language

**Utterance**:
One recording session and the single transcript it produces — the unit of work that
moves through record → transcribe → type.

**Transcript**:
The final text produced from one **Utterance**, after the **Correction Dictionary** has
been applied. This is what gets typed.

**Recorder**:
The single mic-capture facility. There is only ever one (you have one mouth); its state is
`idle` or `recording`. `Toggle`, `VAD`, and `Cancel` act on it.
_Avoid_: "the recording" as a stand-in for the daemon's whole state.

**Delivery queue**:
The FIFO of stopped **Utterances** awaiting transcribe → type. Drains in strict
record-order; **Auto-type** is serialized so utterances never interleave.

**Toggle**:
Press to start recording, press again to stop and enqueue. The human decides when the
utterance ends.

**VAD**:
Press to start; recording auto-stops on the **first** detected silence, then enqueues.
Hands-free, single utterance.

**Continuous mode** _(future, separate mode — not v1)_:
Talk continuously; short pauses cut the audio into **Clips** that transcribe in the
background (pipelined), and a long silence (~10s) ends the **Session**. The latency-hiding
long-form mode for 5–10 minute dictation. Distinct from **VAD** (which stops at the first
silence). _Naming provisional._

**Clip** _(Continuous mode)_:
One silence-bounded audio segment of a **Session**. Transcribed as a full batch job
(preserving accuracy), seeded with the previous clip's transcript tail as `initial_prompt`
to retain cross-clip context.

**Session** _(Continuous mode)_:
A full **Continuous mode** dictation: an ordered sequence of **Clips** whose transcripts
are assembled, in order, into one **Transcript** delivered at the end.

**Cancel**:
Abort the **Recorder**'s current recording and discard its audio. Does not touch
utterances already in the **Delivery queue**.

**Delivery sink** _(or **Sink**)_:
A destination a **Transcript** is delivered to. Exactly **one sink is active** at any
moment; the daemon always delivers to the active sink. Two kinds:
- **Focused-window sink** — the default. Types into whatever window is focused via
  `ydotool`. Reproduces the original behavior; the only sink when nothing else is
  registered. Carries the "wrong-window" risk that the **Freshness window** guards against.
- **Wrapper sink** — a running `talk-to`. Registers over the control socket and receives
  the **Transcript** *pushed* from the daemon, then writes it into the wrapped agent's PTY.
  Delivery is to a known pipe, so there is no "wrong-window" risk.

Switching is sequential, never concurrent — launching a wrapper makes it the active sink;
it can be switched back to the focused-window sink explicitly; only ever **one active at a
time**. An utterance's target sink is **bound when the utterance is triggered**, not when
its transcript is ready. If the bound sink is gone at delivery, the transcript is
**Held-for-replay** and you are asked where to send it — it is **never** silently
redirected to whatever sink is active now.

**Auto-type**:
Delivering a **Transcript** to the **active Delivery sink**, **without** pressing Enter
(the human reviews before submitting). The default sink types into the focused window via
`ydotool`; a **wrapper sink** writes into the wrapped agent's PTY. Gated by the
**Freshness window** *for the focused-window sink* (see that entry).

**Freshness window**:
A generous time-based backstop (~15 min) after a recording ends, gating **Auto-type** **for
the focused-window sink only**. That sink has no window identity (the daemon has no
compositor introspection — S8), so it cannot tell whether you are still in the originally
targeted window; the window is a **best-effort proxy** for "you're probably still there." A
realistic batch transcription types itself well within it; past it, the transcript is
**Held-for-replay**. It can still mis-target on a fast focus switch — the accepted limit of
a sink with no target identity. The **wrapper sink** does **not** use it: its target is an
exact PTY, so it delivers whenever ready, or holds if that PTY died.

**Held for replay**:
A terminal state for an utterance whose **Transcript** was *not* delivered to its **bound
target** — the focused-window sink went stale (past the **Freshness window**), a delivery
failed, or a **wrapper sink** died before delivery. The transcript is cached; recovery is
**never** a silent redirect to the current focus — it is re-routed on demand to a chosen
sink via **Replay-last**.

**Replay-last**:
Re-route a cached **Transcript** to a chosen **Delivery sink** on demand — by default the
most-recent held one. **Recovery-only**: for when a delivery's **bound target** was gone
(you left the focused window, or a **wrapper sink** crashed). The natural generalization is
a transcript-history surface — cached transcripts newest→oldest, route any to any sink — of
which Replay-last is the top entry. Not part of the hands-free happy path.

**Correction dictionary**:
A deterministic, case-insensitive find/replace post-processing step that fixes jargon
Whisper reliably mishears the same way (`"why do tool" → "ydotool"`). A jargon
spell-fixer — explicitly **not** a code-symbol munger.

## Relationships

- One **Utterance** produces exactly one **Transcript**.
- The **Recorder** produces **Utterances**; the **Delivery queue** consumes them in order.
- Every **Utterance** reaches one terminal state: `typed`, `dropped-empty`, or
  `held-for-replay`.
- **Transcript** is cached to disk *before* **Auto-type** is attempted — so delivery is
  never lost even if typing fails.

## Flagged ambiguities

- The original plan said `toggle` during transcription is *ignored* (strictly one
  utterance at a time). Resolved: superseded by the **Recorder** + **Delivery queue**
  model — a new recording can start while prior utterances transcribe/type, and they
  deliver in strict record-order.
