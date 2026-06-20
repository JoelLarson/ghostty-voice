//! Integration test: the ordered delivery queue drains in strict record-order
//! even when a *later* utterance is transcribed *first* by a slow/racy server.
//!
//! This exercises the real composition the daemon relies on — the pure
//! `DeliveryQueue` fed by real `post_inference` round-trips against a stdlib
//! fake whisper-server — with no GPU, mic, or ydotool involved. The fake server
//! replies to utterance #1 only after a delay, so #2's transcript becomes ready
//! first; the queue must still deliver #1 before #2 (no interleave, no
//! supersede-drop).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ghostty_voice_core::queue::DeliveryQueue;
use ghostty_voice_io::transcribe::{InferenceParams, post_inference};

/// Minimal request params for the transport round-trip in this test.
fn test_params() -> InferenceParams {
    InferenceParams {
        beam_size: 8,
        temperature: 0.0,
        initial_prompt: String::new(),
        prompt_truncated: false,
    }
}

/// A fake whisper-server: each accepted connection drains the request, waits
/// `delay`, then replies with `{"text": "<reply>"}`. One thread per request.
fn fake_whisper(reply: &'static str, delay: Duration) -> (String, u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let reply = reply.to_owned();
            thread::spawn(move || {
                stream
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                let mut buf = [0u8; 8192];
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => continue,
                    }
                }
                thread::sleep(delay);
                let body = format!("{{\"text\": \"{reply}\"}}");
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            });
        }
    });
    (addr.ip().to_string(), addr.port(), handle)
}

fn sample_wav() -> std::path::PathBuf {
    let wav = std::env::temp_dir().join(format!(
        "gv-ordered-drain-{}-{:?}.wav",
        std::process::id(),
        thread::current().id(),
    ));
    std::fs::write(&wav, b"RIFFdummywavbytes").unwrap();
    wav
}

#[test]
fn queue_drains_in_record_order_despite_a_slow_first_transcription() {
    // Two fake servers: #1 is slow, #2 is fast — so #2 finishes transcribing
    // first, racing ahead of the earlier utterance.
    let (slow_host, slow_port, slow) = fake_whisper("first", Duration::from_millis(300));
    let (fast_host, fast_port, fast) = fake_whisper("second", Duration::from_millis(0));

    let mut queue = DeliveryQueue::new();
    let a = queue.enqueue_at(Duration::from_secs(0)); // recorded first
    let b = queue.enqueue_at(Duration::from_secs(0)); // recorded second

    let wav_a = sample_wav();
    let wav_b = sample_wav();

    // Transcribe both concurrently; report (seq, transcript) over a channel as
    // each completes — mirroring the daemon's background transcription tasks.
    let (tx, rx) = mpsc::channel::<(u64, String)>();
    let tx2 = tx.clone();
    let t_a = {
        let (h, w) = (slow_host.clone(), wav_a.clone());
        thread::spawn(move || {
            let body = post_inference(&h, slow_port, &w, &test_params()).unwrap();
            let text = ghostty_voice_core::transcript::parse_transcript(&body).unwrap();
            tx.send((a, text)).unwrap();
        })
    };
    let t_b = {
        let (h, w) = (fast_host.clone(), wav_b.clone());
        thread::spawn(move || {
            let body = post_inference(&h, fast_port, &w, &test_params()).unwrap();
            let text = ghostty_voice_core::transcript::parse_transcript(&body).unwrap();
            tx2.send((b, text)).unwrap();
        })
    };

    // The window: the daemon would set transcripts ready as they arrive and
    // drain the head whenever it is ready. Mirror that here.
    let mut delivered: Vec<String> = Vec::new();
    let now = Duration::from_secs(1);
    let window = Duration::from_secs(900);

    // Collect both transcripts (order of arrival is #2 then #1).
    let mut arrivals = Vec::new();
    for _ in 0..2 {
        arrivals.push(rx.recv_timeout(Duration::from_secs(5)).unwrap());
    }
    // #2 must have arrived before #1 — proving the race we set up.
    assert_eq!(
        arrivals[0].0, b,
        "fast utterance #2 should transcribe first"
    );

    // Feed readiness in arrival order, draining the head whenever ready.
    for (seq, text) in arrivals {
        queue.set_ready(seq, text);
        while let Some((head_seq, head_text, _delivery)) = queue.head_delivery(now, window) {
            delivered.push(head_text.to_owned());
            let resolved = head_seq;
            queue.resolve(resolved);
        }
    }

    t_a.join().unwrap();
    t_b.join().unwrap();
    drop(slow);
    drop(fast);

    // Despite #2 finishing first, delivery order is strict record-order.
    assert_eq!(delivered, vec!["first".to_owned(), "second".to_owned()]);
    assert!(queue.is_empty());

    let _ = std::fs::remove_file(&wav_a);
    let _ = std::fs::remove_file(&wav_b);
}
