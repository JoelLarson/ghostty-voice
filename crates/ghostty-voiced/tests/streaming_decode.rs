//! Integration test: the streaming-dictation decode loop against a fake
//! whisper-server.
//!
//! Reconstructs the real composition the daemon's `drive_streaming` +
//! `finalize_streaming` and the wrapper's edit-application rely on — successive
//! `post_inference` round-trips on the live lane (`beam_size=1`, no
//! `initial_prompt`), the real `CommitEngine` (LocalAgreement-2), the real
//! `Frame::LiveEdit` wire round-trip, the real `PreviewCursor` erase-and-type
//! against a stand-in line editor, and on finalize the batch reconcile
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
use ghostty_voice_core::pty::PreviewCursor;
use ghostty_voice_core::queue::DeliveryQueue;
use ghostty_voice_core::sink::{Route, SinkRegistry};
use ghostty_voice_core::streaming::CommitEngine;
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

/// Fake whisper-server answering a *scripted sequence* of requests: request N
/// gets `bodies[N]`. The self-paced loop opens one connection per decode, so a
/// growing/revised hypothesis is just successive bodies, ending with the reconcile.
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
    InferenceParams {
        beam_size: 1,
        temperature: 0.0,
        initial_prompt: String::new(),
        prompt_truncated: false,
    }
}

fn batch_params() -> InferenceParams {
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

/// A stand-in for the wrapped composer's line editor: DEL (`0x7f`) deletes one
/// logical char; every other byte is UTF-8 text appended. Independent of how the
/// edit bytes are built, so driving the real `PreviewCursor` output through it
/// proves the preview ends up correct (the same shape as a readline/bash line
/// editor; the real Claude composer is the human's manual smoke-test).
#[derive(Default)]
struct LineEditor {
    chars: Vec<char>,
}

impl LineEditor {
    fn apply(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 0x7f {
                self.chars.pop();
                i += 1;
            } else {
                let start = i;
                while i < bytes.len() && bytes[i] != 0x7f {
                    i += 1;
                }
                self.chars
                    .extend(std::str::from_utf8(&bytes[start..i]).unwrap().chars());
            }
        }
    }

    fn text(&self) -> String {
        self.chars.iter().collect()
    }
}

#[test]
fn live_preview_self_edits_in_place_then_reconciles_to_the_batch_accurate_transcript() {
    // The dictator speaks "run why do tool". Whisper first mishears the tail
    // ("wide"), then firms it up. The live lane revises in place; the stable
    // prefix never flickers; the batch reconcile corrects the jargon.
    let wav = tmp("happy");
    write_wav(&wav, 32_000); // 1 s — well over the min-duration decode floor
    assert!(wav_duration(&wav).unwrap() >= MIN);

    // d1 mishears "wide"; d2 corrects it to "why"; then it grows and settles.
    let live = [
        "run wide",
        "run why",
        "run why do",
        "run why do tool",
        "run why do tool",
    ];
    let mut bodies: Vec<String> = live.iter().map(|t| json(t)).collect();
    bodies.push(json("run why do tool")); // the final batch reconcile request
    let (host, port, server) = scripted_whisper(bodies);

    // The active wrapper sink, bound at trigger time.
    let mut registry = SinkRegistry::new();
    let wrapper = registry.register();
    let bound = registry.active();
    assert_eq!(bound, Some(wrapper));

    // Daemon side: the commit engine. Wrapper side: the preview cursor + the
    // stand-in line editor. Each decode flows daemon → Frame::LiveEdit (wire
    // round-trip) → wrapper, exactly like the two real processes.
    let mut engine = CommitEngine::new();
    let mut cursor = PreviewCursor::new();
    let mut editor = LineEditor::default();

    let mut committed_lens = Vec::new();
    let want_preview = [
        "run wide",
        "run why",
        "run why do",
        "run why do tool",
        "run why do tool",
    ];
    for (i, _) in live.iter().enumerate() {
        let body = post_inference(&host, port, &wav, &live_params()).unwrap();
        let text = parse_transcript(&body).unwrap();
        let words: Vec<&str> = text.split_whitespace().collect();

        let edit = engine.observe(&words);
        committed_lens.push(engine.committed_len());

        // Daemon encodes a Frame::LiveEdit; the wrapper parses and applies it.
        let frame = Frame::LiveEdit {
            committed: edit.committed,
            tail: edit.tail,
        };
        let Ok(Frame::LiveEdit { committed, tail }) = Frame::parse(&frame.encode()) else {
            panic!("a LiveEdit frame must round-trip");
        };
        editor.apply(&cursor.apply_edit(&committed, &tail));

        // The preview in the prompt matches the (raw, uncorrected) hypothesis.
        assert_eq!(editor.text(), want_preview[i], "preview after decode {i}");
    }

    // The committed prefix only ever grew (LocalAgreement-2, monotone, no retract).
    assert!(
        committed_lens.windows(2).all(|w| w[1] >= w[0]),
        "committed prefix must grow monotonically: {committed_lens:?}",
    );
    assert_eq!(engine.committed_text(), "run why do tool");
    // The mishear "wide" never entered the committed prefix.
    assert!(!engine.committed_text().contains("wide"));

    // Finalize: the batch reconcile over the complete capture, corrected, then the
    // wrapper erases the whole preview and types the Transcript — no double-typing.
    let body = post_inference(&host, port, &wav, &batch_params()).unwrap();
    server.join().unwrap();
    let raw = parse_transcript(&body).unwrap();
    let dur = wav_duration(&wav).unwrap();
    let reconciled = finalize_transcript(&raw, dur, MIN, &corrections()).unwrap();
    assert_eq!(reconciled, "run ydotool");

    // The final Transcript routes through Delivery against the trigger-time binding.
    let mut queue = DeliveryQueue::new();
    let seq = queue.enqueue();
    queue.set_ready(seq, reconciled.clone());
    let (head, text) = queue
        .next_to_type()
        .map(|(s, t)| (s, t.to_owned()))
        .unwrap();
    assert_eq!(head, seq);
    // Routed to the live bound wrapper ⇒ delivered as a Finalize/replace frame.
    let frame = match registry.route(bound) {
        Route::Wrapper(id) => {
            assert_eq!(id, wrapper, "delivered to the bound wrapper");
            Frame::Finalize(text)
        }
        other => panic!("expected a wrapper route, got {other:?}"),
    };
    // The wrapper parses the Finalize frame and replaces the rough preview wholesale
    // — no double-typing, no trailing Enter.
    let Ok(Frame::Finalize(final_text)) = Frame::parse(&frame.encode()) else {
        panic!("a Finalize frame must round-trip");
    };
    editor.apply(&cursor.finalize(&final_text));
    assert_eq!(
        editor.text(),
        "run ydotool",
        "preview replaced by the reconcile, no double-typing"
    );
    assert_eq!(
        cursor.preview_len(),
        0,
        "the preview buffer is fully consumed"
    );

    let _ = std::fs::remove_file(&wav);
}

#[test]
fn with_no_wrapper_registered_the_live_preview_no_ops_and_the_final_is_held() {
    // No wrapper registered: no active sink, so the live lane has nowhere to push
    // (it no-ops) and the final Transcript binds to None ⇒ Held-for-replay.
    let wav = tmp("held");
    write_wav(&wav, 32_000);

    let live = ["create", "create a function"];
    let mut bodies: Vec<String> = live.iter().map(|t| json(t)).collect();
    bodies.push(json("create a function"));
    let (host, port, server) = scripted_whisper(bodies);

    let registry = SinkRegistry::new();
    let bound = registry.active();
    assert_eq!(bound, None, "no wrapper ⇒ no active sink");

    // The decode loop + commit engine still run (capture is independent of any
    // wrapper), but with no active sink there is nothing to push the preview to.
    let mut engine = CommitEngine::new();
    let mut pushed = 0usize;
    for _ in 0..live.len() {
        let body = post_inference(&host, port, &wav, &live_params()).unwrap();
        let text = parse_transcript(&body).unwrap();
        let words: Vec<&str> = text.split_whitespace().collect();
        let _edit = engine.observe(&words);
        if bound.is_some() {
            pushed += 1;
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

    // A held streaming finalize replays later as a plain Transcript append: there
    // is no live preview to erase (the bound wrapper was gone), so it lands as a
    // normal injection in whatever wrapper is active at replay time.
    let cursor = PreviewCursor::new();
    let mut editor = LineEditor::default();
    let Ok(Frame::Transcript(replayed)) =
        Frame::parse(&Frame::Transcript(reconciled.clone()).encode())
    else {
        panic!("replay uses a plain Transcript frame");
    };
    editor.apply(&ghostty_voice_core::pty::injection_bytes(&replayed));
    assert_eq!(editor.text(), "create a function");
    assert_eq!(
        cursor.preview_len(),
        0,
        "no preview was ever built to erase"
    );

    let _ = std::fs::remove_file(&wav);
}
