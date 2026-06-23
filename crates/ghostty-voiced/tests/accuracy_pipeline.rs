//! Integration test for the accuracy path against sample WAVs.
//!
//! Exercises the real composition the daemon's transcribe path runs — a real
//! WAV measured by `audio::wav_duration`, a real `post_inference` round-trip
//! against a stdlib fake whisper-server, and the pure `pipeline::finalize_transcript`
//! (filter + correction dictionary) — with no GPU, mic, or ydotool. Asserts
//! jargon comes out corrected and a silence WAV types nothing.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use ghostty_voice_core::pipeline::finalize_transcript;
use ghostty_voice_core::transcript::parse_transcript;
use ghostty_voice_io::audio::wav_duration;
use ghostty_voice_io::transcribe::{InferenceParams, post_inference};

/// Write a canonical 16 kHz mono s16 WAV with `data_bytes` of (zeroed) PCM.
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

/// Fake whisper-server: one request, reply with `{"text": "<reply>"}`.
fn fake_whisper(reply: &'static str) -> (String, u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
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
        let body = format!(r#"{{"text": "{reply}"}}"#);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes()).unwrap();
        stream.flush().unwrap();
    });
    (addr.ip().to_string(), addr.port(), handle)
}

fn params() -> InferenceParams {
    InferenceParams {
        beam_size: 8,
        temperature: 0.0,
        initial_prompt: String::new(),
        prompt_truncated: false,
    }
}

fn corrections() -> BTreeMap<String, String> {
    [("why do tool", "ydotool"), ("ghosty", "Ghostty")]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

const MIN: Duration = Duration::from_millis(300);

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ghostty-voice-accuracy-{name}.wav"))
}

#[test]
fn jargon_wav_comes_out_corrected() {
    // A real 1 s WAV; the server "mishears" the jargon the way Whisper does.
    let wav = tmp("jargon");
    write_wav(&wav, 32_000); // 1 s of audio, well over the min
    let (host, port, server) = fake_whisper("run why do tool on ghosty");

    let dur = wav_duration(&wav).unwrap();
    let body = post_inference(&host, port, &wav, &params()).unwrap();
    server.join().unwrap();

    let raw = parse_transcript(&body).unwrap();
    let final_text = finalize_transcript(&raw, dur, MIN, &corrections());

    assert_eq!(final_text.as_deref(), Some("run ydotool on Ghostty"));
    let _ = std::fs::remove_file(&wav);
}

#[test]
fn silence_wav_types_nothing() {
    // A real WAV of adequate length, but the server returns a stock silence
    // hallucination — the filter must discard it (type nothing).
    let wav = tmp("silence");
    write_wav(&wav, 32_000);
    let (host, port, server) = fake_whisper("[BLANK_AUDIO]");

    let dur = wav_duration(&wav).unwrap();
    let body = post_inference(&host, port, &wav, &params()).unwrap();
    server.join().unwrap();

    let raw = parse_transcript(&body).unwrap();
    let final_text = finalize_transcript(&raw, dur, MIN, &corrections());

    assert_eq!(final_text, None);
    let _ = std::fs::remove_file(&wav);
}

#[test]
fn sub_min_duration_wav_types_nothing() {
    // A 0.1 s blip is under the 0.3 s floor — discarded regardless of text.
    let wav = tmp("blip");
    write_wav(&wav, 3_200); // 0.1 s
    let dur = wav_duration(&wav).unwrap();
    assert!(dur < MIN);

    let final_text = finalize_transcript("rebase onto main", dur, MIN, &corrections());
    assert_eq!(final_text, None);
    let _ = std::fs::remove_file(&wav);
}
