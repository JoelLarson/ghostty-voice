# ghostty-voice

Voice dictation that types clean English prose into a coding agent you run inside
`talk-to`, so you can speak instructions to an agent running over SSH. The agent's own
voice features can't cross SSH, so Whisper runs locally and the text is delivered straight
into the agent's PTY. `talk-to` is the **sole interface**: triggers and delivery both go
through the wrapper you are actively using — there is no system-wide hotkey and no typing
into the focused window.

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

**Streaming dictation** _(Shift+F9)_:
A dictation that shows a **Live preview** in the wrapped agent's prompt *as you speak*, then
replaces it with the batch-accurate **Transcript** on stop (the **Reconcile**). A self-paced
loop decodes a bounded sliding window of the growing capture on the one existing whisper-server
and revises the preview in place; a ~10 s silence ends it hands-free, or Shift+F10 force-stops
it. The immediacy mode — distinct from **Continuous mode** (background accuracy) and **VAD**
(one batch utterance). The conscious extension of ADR-0002 (`docs/adr/0004`).

**Live preview** _(Streaming dictation)_:
The ephemeral, *rough* text pushed into the **active** wrapper sink during a **Streaming
dictation** — raw Whisper output (no **Correction dictionary**), revised in place. It bypasses
the **Delivery queue** (it is not an **Utterance**); only the final **Transcript** flows through
**Delivery**. A **Stable prefix** of committed words never flickers; the **Unstable tail** is
rewritten each decode.

**Stable prefix / Unstable tail** _(Streaming dictation)_:
The two parts of the **Live preview**. A word joins the **Stable prefix** (committed, never
rewritten) once two consecutive decodes agree on it (**LocalAgreement-2**); everything after it
is the **Unstable tail**, re-typed as Whisper firms up. The prefix grows monotonically and never
retracts.

**Reconcile** _(Streaming dictation)_:
The finalize step: on stop, the full-utterance **batch** transcription (beam-8, `initial_prompt`,
**Correction dictionary**) over the complete capture **replaces** the whole **Live preview** with
the accurate, jargon-corrected **Transcript** — delivered through **Delivery** (bound-at-trigger,
**Held-for-replay** if the bound wrapper died). Live immediacy without losing batch accuracy.

**Cancel**:
Abort the **Recorder**'s current recording (or **Streaming dictation**) and discard its audio —
a cancelled dictation also erases its **Live preview** from the prompt. Does not touch utterances
already in the **Delivery queue**.

**Delivery sink** _(or **Sink**)_:
A destination a **Transcript** is delivered to — always a **wrapper sink**, a running
`talk-to`. At most **one sink is active** at a time; with no `talk-to` registered there is
**no active sink** and nowhere to deliver. A wrapper registers over the control socket and
receives the **Transcript** *pushed* from the daemon, then writes it into the wrapped
agent's PTY. Delivery is to a known pipe, so there is no "wrong-window" risk.

Switching is sequential, never concurrent — launching a wrapper makes it the active sink;
only ever **one active at a time**. With **several wrappers** registered, closing the
active one hands off to the **most-recently-registered still-live** wrapper sink (the
*newest-live handoff*); the active sink falls back to **none** only when the **last**
wrapper exits — never while another wrapper is still live. An utterance's target sink is
**bound when the utterance is triggered**, not when its transcript is ready. If the bound
sink is gone at delivery, the transcript is **Held-for-replay** — it is **never** silently
redirected to whatever sink is active now (so an utterance bound to a now-dead wrapper is
still held even when a handoff kept another wrapper active).

**Auto-type**:
Delivering a **Transcript** to the **active Delivery sink**, **without** pressing Enter
(the human reviews before submitting). A **wrapper sink** writes it into the wrapped
agent's PTY. There is no time-based staleness gate — the PTY target is exact, so the only
reason a transcript is not delivered is that the bound wrapper is gone (then it is
**Held-for-replay**).

**Held for replay**:
A terminal state for an utterance whose **Transcript** was *not* delivered to its **bound
target** — the bound **wrapper sink** died before delivery, no wrapper was registered at
trigger time, or a delivery write failed. The transcript is cached; recovery is **never** a
silent redirect — it is re-routed on demand to the active wrapper sink via **Replay-last**.

**Replay-last**:
Re-deliver a cached **Transcript** to the **active wrapper sink** on demand — by default
the most-recent held one; errors when no `talk-to` is registered. **Recovery-only**: for
when a delivery's **bound target** was gone (the **wrapper sink** crashed, or none was
registered). The natural generalization is a transcript-history surface — cached
transcripts newest→oldest — of which Replay-last is the top entry. Not part of the
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
