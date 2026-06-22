//! Integration test: a Transcript bound to a registered **wrapper sink** is
//! pushed down its connection and arrives in the wrapper exactly — the
//! hands-free happy path's delivery hop.
//!
//! Like `ordered_drain.rs`, this exercises the real composition the daemon's
//! `drain_queue` relies on — the real `DeliveryQueue` (head readiness + freshness),
//! the real `SinkRegistry` routing, and the real newline `Frame` protocol over a
//! real Unix socket — with a test double only at the socket peer. No GPU, mic, or
//! ydotool.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ghostty_voice_core::protocol::{Command, Frame};
use ghostty_voice_core::queue::DeliveryQueue;
use ghostty_voice_core::sink::{ActiveSink, Route, SinkRegistry};

#[test]
fn a_bound_wrapper_sink_receives_the_pushed_transcript_end_to_end() {
    let sock = std::env::temp_dir().join(format!(
        "gv-wrapper-deliver-{}-{:?}.sock",
        std::process::id(),
        thread::current().id(),
    ));
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    let transcript = "create a function that reverses a string";

    // The wrapper-sink client (`talk-to`): connect, register, read one Transcript
    // frame back and report the decoded text.
    let (got_tx, got_rx) = mpsc::channel::<String>();
    let csock = sock.clone();
    let client = thread::spawn(move || {
        let mut stream = UnixStream::connect(&csock).unwrap();
        stream.write_all(b"register-sink\n").unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if let Ok(Frame::Transcript(t)) = Frame::parse(&line) {
            got_tx.send(t).unwrap();
        }
    });

    // The "daemon": accept, register the wrapper sink, then drain a bound
    // utterance to it through the real queue + routing.
    let (mut conn, _) = listener.accept().unwrap();
    let mut reader = BufReader::new(conn.try_clone().unwrap());
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    assert!(matches!(Command::parse(&first), Ok(Command::RegisterSink)));

    let mut registry = SinkRegistry::new();
    let id = registry.register();

    let mut queue = DeliveryQueue::new();
    let seq = queue.enqueue_at(Duration::from_secs(0));
    // Bind the target sink at trigger time — the active sink is the wrapper.
    let bound = registry.active();
    assert_eq!(bound, ActiveSink::Wrapper(id));
    queue.set_ready(seq, transcript.to_owned());

    // Drain the head exactly as the daemon does: readiness + freshness from the
    // queue, then route by the bound sink.
    let now = Duration::from_secs(1);
    let window = Duration::from_secs(900);
    let (head_seq, head_text, _freshness) = queue.head_delivery(now, window).unwrap();
    assert_eq!(head_seq, seq);
    match registry.route(bound) {
        Route::Wrapper(wid) => {
            assert_eq!(wid, id, "routes to the bound wrapper sink");
            conn.write_all(
                format!("{}\n", Frame::Transcript(head_text.to_owned()).encode()).as_bytes(),
            )
            .unwrap();
            conn.flush().unwrap();
        }
        other => panic!("expected a wrapper route, got {other:?}"),
    }
    queue.resolve(head_seq);

    // The wrapper received exactly the Transcript, with NO trailing newline —
    // review-before-Enter survives.
    let received = got_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(received, transcript);
    assert!(
        !received.ends_with('\n') && !received.ends_with('\r'),
        "the delivered Transcript must not carry a trailing Enter",
    );

    client.join().unwrap();
    let _ = std::fs::remove_file(&sock);
}

#[test]
fn with_no_wrapper_registered_delivery_routes_to_the_focused_window_sink() {
    // Additivity: with nothing registered the bound sink is the focused-window
    // sink, so the daemon takes today's `ydotool` Auto-type path unchanged.
    let registry = SinkRegistry::new();
    let mut queue = DeliveryQueue::new();
    let seq = queue.enqueue_at(Duration::from_secs(0));
    let bound = registry.active();
    assert_eq!(bound, ActiveSink::FocusedWindow);
    queue.set_ready(seq, "hello".to_owned());

    let (head_seq, _text, _freshness) = queue
        .head_delivery(Duration::from_secs(1), Duration::from_secs(900))
        .unwrap();
    assert_eq!(head_seq, seq);
    assert_eq!(registry.route(bound), Route::FocusedWindow);
}
