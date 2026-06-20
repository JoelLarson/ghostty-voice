# Batch transcription first; segmented-pipeline as a separate, deferred mode

Accuracy on technical jargon is this project's primary goal, and full-utterance **batch**
decoding (`large-v3`, `beam-8`, `initial_prompt` biasing, full `condition_on_previous_text`
context) is what maximizes it. We therefore build batch transcription first and reject live
**sliding-window streaming** outright — streaming decodes on partial context, measurably
hurting accuracy, and you can't type revising text into the prompt live anyway.

The real pain batch leaves is **post-stop latency**: a 5–10 minute recording (the user's
normal workflow) takes minutes to transcribe *after* recording ends. The chosen fix is
**not** streaming but a **Continuous mode** (deferred, separate): `sox` splits the session
into silence-bounded **Clips** that batch-transcribe in the background while talking
continues, chained via each clip's transcript tail as the next clip's `initial_prompt`.
This overlaps compute with recording and preserves batch accuracy, collapsing the post-stop
wait to ~one clip. Clips transcribe serially anyway (one GPU), so context-chaining is free.

## Consequences

- v1 ships three modes: **Toggle** and **VAD** (batch, M2–M5) and, later, **Continuous**
  mode as a *distinct* mode (not a replacement for VAD).
- Until Continuous mode lands, long batch sessions incur a multi-minute post-stop wait,
  handled by the delivery model (see the auto-type / deliver-hotkey decision).
- Continuous mode is real complexity (continuous capture, dual-threshold silence
  detection, segment watching, ordered chaining queue, clip-transcript assembly) and is
  deliberately out of M1–M5.
