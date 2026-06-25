//! Integration test: the streaming-dictation decode loop against a fake
//! whisper-server.
//!
//! Reconstructs the real composition the daemon's `drive_streaming` +
//! `finalize_streaming` rely on — successive `post_inference` round-trips on the
//! live lane (`beam_size=1`, no `initial_prompt`), the real `parse_transcript`,
//! the real `Frame::Live` append encoding, and on finalize the batch reconcile
//! (`finalize_transcript` with the correction dictionary) routed through the real
//! `SinkRegistry` + `DeliveryQueue` — with a test double only at the socket peer.
//! No GPU, mic, or second model. Mirrors the real-socket style of `transcribe.rs`.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use ghostty_voice_core::pipeline::finalize_transcript;
use ghostty_voice_core::protocol::Frame;
use ghostty_voice_core::queue::DeliveryQueue;
use ghostty_voice_core::sink::{Route, SinkRegistry};
use ghostty_voice_core::transcript::parse_transcript;
use ghostty_voice_io::audio::wav_duration;
use ghostty_voice_io::transcribe::{InferenceParams, post_inference};

/// Write a canonical 16 kHz mono s16 WAV with `data_bytes` of (zeroed) PCM. The
/// fake server ignores the audio; the WAV just has to be a real, long-enough file.
fn write_wav(path: &Path, data_bytes: u32) {
    let mut w = Vec::new();
    let fmt_size: u32 = 16;
    let riff_size = 4 + (8 + fmt_size) + (8 + data_bytes);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&riff_size.to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&fmt_size.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&16_000u32.to_le_bytes());
    w.extend_from_slice(&32_000u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_bytes.to_le_bytes());
    w.extend(std::iter::repeat_n(0u8, data_bytes as usize));
    std::fs::write(path, w).unwrap();
}

/// Fake whisper-server that answers a *scripted sequence* of requests: request N
/// gets `bodies[N]`. The self-paced loop opens one connection per decode, so a
/// growing hypothesis is just successive bodies, ending with the batch reconcile.
fn scripted_whisper(bodies: Vec<String>) -> (String, u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let mut buf = [0u8; 8192];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    });
    (addr.ip().to_string(), addr.port(), handle)
}

fn json(text: &str) -> String {
    format!(r#"{{"text": "{text}"}}"#)
}

fn live_params() -> InferenceParams {
    // The live lane: cheap beam, no initial_prompt — raw Whisper text.
    InferenceParams {
        beam_size: 1,
        temperature: 0.0,
        initial_prompt: String::new(),
        prompt_truncated: false,
    }
}

fn batch_params() -> InferenceParams {
    // The reconcile lane: the accuracy stack's wider beam.
    InferenceParams {
        beam_size: 8,
        temperature: 0.0,
        initial_prompt: String::new(),
        prompt_truncated: false,
    }
}

fn corrections() -> BTreeMap<String, String> {
    [("why do tool", "ydotool")]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ghostty-voice-streaming-{}-{name}.wav",
        std::process::id()
    ))
}

const MIN: Duration = Duration::from_millis(300);

/// One live decode pass: POST the growing capture, parse the hypothesis, and
/// return the append-only delta (the words past `appended`) as the wrapper would
/// receive it — a leading space joins it to the existing preview. Returns the new
/// `appended` count. Mirrors the daemon's `decode_live` + `push_live_preview`.
fn live_pass(host: &str, port: u16, wav: &Path, appended: usize) -> (Option<Frame>, usize) {
    let body = post_inference(host, port, wav, &live_params()).unwrap();
    let text = parse_transcript(&body).unwrap();
    let words: Vec<&str> = text.split_whitespace().collect();
    let start = appended.min(words.len());
    let new = &words[start..];
    if new.is_empty() {
        return (None, appended);
    }
    let joined = new.join(" ");
    let delta = if start == 0 {
        joined
    } else {
        format!(" {joined}")
    };
    (Some(Frame::Live(delta)), words.len())
}

#[test]
fn live_preview_appends_growing_text_then_reconciles_to_the_batch_accurate_transcript() {
    // The dictator speaks "run why do tool"; the live lane sees it grow word by
    // word (raw, uncorrected), and the batch reconcile corrects the jargon.
    let wav = tmp("happy");
    write_wav(&wav, 32_000); // 1 s — well over the min-duration decode floor
    assert!(wav_duration(&wav).unwrap() >= MIN);

    let live = ["run", "run why", "run why do", "run why do tool"];
    let mut bodies: Vec<String> = live.iter().map(|t| json(t)).collect();
    bodies.push(json("run why do tool")); // the final batch request
    let (host, port, server) = scripted_whisper(bodies);

    // The active wrapper sink, bound at trigger time.
    let mut registry = SinkRegistry::new();
    let wrapper = registry.register();
    let bound = registry.active();
    assert_eq!(bound, Some(wrapper));

    // Self-paced live loop: each decode pushes only the newly-grown tail.
    let mut appended = 0usize;
    let mut preview = String::new();
    for _ in 0..live.len() {
        let (frame, next) = live_pass(&host, port, &wav, appended);
        appended = next;
        if let Some(Frame::Live(delta)) = frame {
            // Each live frame round-trips through the wire protocol...
            assert_eq!(
                Frame::parse(&Frame::Live(delta.clone()).encode()).unwrap(),
                Frame::Live(delta.clone())
            );
            // ...and the wrapper appends it (no trailing newline) into the prompt.
            preview.push_str(&delta);
        }
    }
    // The live preview that landed in the prompt is the raw, uncorrected text,
    // grown append-only with no re-typing.
    assert_eq!(preview, "run why do tool");

    // Finalize: the batch reconcile over the complete capture, corrected.
    let body = post_inference(&host, port, &wav, &batch_params()).unwrap();
    server.join().unwrap();
    let raw = parse_transcript(&body).unwrap();
    let dur = wav_duration(&wav).unwrap();
    let reconciled = finalize_transcript(&raw, dur, MIN, &corrections()).unwrap();
    assert_eq!(
        reconciled, "run ydotool",
        "the reconcile applies the dictionary"
    );

    // The final Transcript flows through Delivery against the trigger-time binding.
    let mut queue = DeliveryQueue::new();
    let seq = queue.enqueue();
    queue.set_ready(seq, reconciled.clone());
    let (head, text) = queue
        .next_to_type()
        .map(|(s, t)| (s, t.to_owned()))
        .unwrap();
    assert_eq!(head, seq);
    match registry.route(bound) {
        Route::Wrapper(id) => assert_eq!(id, wrapper, "delivered to the bound wrapper"),
        other => panic!("expected a wrapper route, got {other:?}"),
    }
    // The delivered finalize text carries no trailing Enter.
    let injected = ghostty_voice_core::pty::injection_bytes(&text);
    assert!(!injected.ends_with(b"\n") && !injected.ends_with(b"\r"));

    let _ = std::fs::remove_file(&wav);
}

#[test]
fn with_no_wrapper_registered_the_live_preview_no_ops_and_the_final_is_held() {
    // No wrapper registered: there is no active sink, so the live lane has nowhere
    // to push (it no-ops) and the final Transcript binds to None ⇒ Held-for-replay,
    // exactly like a batch utterance.
    let wav = tmp("held");
    write_wav(&wav, 32_000);

    let live = ["create", "create a function"];
    let mut bodies: Vec<String> = live.iter().map(|t| json(t)).collect();
    bodies.push(json("create a function"));
    let (host, port, server) = scripted_whisper(bodies);

    let registry = SinkRegistry::new();
    let bound = registry.active();
    assert_eq!(bound, None, "no wrapper ⇒ no active sink");

    // The decode loop still runs (capture is independent of any wrapper), but with
    // no active sink there is nothing to push the live preview to.
    let mut appended = 0usize;
    let mut pushed = 0usize;
    for _ in 0..live.len() {
        let (frame, next) = live_pass(&host, port, &wav, appended);
        appended = next;
        if frame.is_some() {
            // A real daemon would skip the send when there is no live sender; here
            // we just count that there is no sink to deliver to.
            if bound.is_some() {
                pushed += 1;
            }
        }
    }
    assert_eq!(pushed, 0, "the live preview no-ops with no wrapper");

    let body = post_inference(&host, port, &wav, &batch_params()).unwrap();
    server.join().unwrap();
    let raw = parse_transcript(&body).unwrap();
    let dur = wav_duration(&wav).unwrap();
    let reconciled = finalize_transcript(&raw, dur, MIN, &corrections()).unwrap();
    assert_eq!(reconciled, "create a function");

    // Bound to nothing ⇒ Held-for-replay, never redirected.
    assert_eq!(registry.route(bound), Route::Held);

    let _ = std::fs::remove_file(&wav);
}
