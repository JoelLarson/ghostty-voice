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

**Auto-type**:
Injecting a **Transcript** into the focused window via `ydotool`, **without** pressing
Enter (the human reviews before submitting). Gated by the **Freshness window**.

**Freshness window**:
A generous safety backstop (~15 min) after a recording ends, within which **Auto-type** is
allowed. The guiding priority is **hands-free** delivery, so the window is sized so any
realistic batch transcription types itself when done; it only holds a transcript in the
genuine pathological case (server down well beyond the window). Not a routine gate.

**Held for replay**:
A terminal state for an utterance whose transcript was *not* auto-typed (stale, server was
down, or type failed). The transcript is cached and recovered on demand via **Replay-last**.

**Replay-last**:
Re-inject the most-recent cached **Transcript** on demand (after refocusing Ghostty).
**Recovery-only** — for when **Auto-type** landed in the wrong window; not part of the
hands-free happy path.

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
