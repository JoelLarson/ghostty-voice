//! Integration test: the **newest-live handoff**. With two `talk-to`
//! **wrapper sinks** registered, closing the *active* one hands the active
//! **Delivery sink** off to the still-live wrapper — never down to *no sink*
//! while a wrapper remains — and a transcript bound afterward is pushed to that
//! survivor. There is no active sink only when the **last** wrapper exits.
//!
//! Like `sink_registration.rs` / `held_for_replay.rs`, this exercises the real
//! composition the daemon's connection handler + `drain_queue` rely on — the real
//! `SinkRegistry` newest-live handoff, the real `DeliveryQueue` (head readiness),
//! and the real newline `Frame` protocol over a real Unix socket — with a test
//! double only at the socket peer (the in-test `talk-to` clients). No GPU or mic.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ghostty_voice_core::protocol::{Command, Frame};
use ghostty_voice_core::queue::DeliveryQueue;
use ghostty_voice_core::sink::{Route, SinkRegistry};

/// Read one line from a wrapper connection and assert it is `register-sink`.
fn expect_register(reader: &mut BufReader<UnixStream>) {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(
        matches!(Command::parse(&line), Ok(Command::RegisterSink(_))),
        "expected register-sink, got {line:?}",
    );
}

#[test]
fn closing_the_active_wrapper_hands_off_to_the_other_live_wrapper() {
    let sock = std::env::temp_dir().join(format!(
        "gv-handoff-{}-{:?}.sock",
        std::process::id(),
        thread::current().id(),
    ));
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    let transcript = "refactor the parser into its own module";

    // Wrapper A: registers first (older), survives the handoff, and receives the
    // transcript bound after B closes. It drops only once told to.
    let (a_registered_tx, a_registered_rx) = mpsc::channel::<()>();
    let (a_drop_tx, a_drop_rx) = mpsc::channel::<()>();
    let (a_got_tx, a_got_rx) = mpsc::channel::<String>();
    let a_sock = sock.clone();
    let client_a = thread::spawn(move || {
        let mut stream = UnixStream::connect(&a_sock).unwrap();
        stream.write_all(b"register-sink\n").unwrap();
        a_registered_tx.send(()).unwrap();
        // Block until the daemon pushes the handed-off transcript, then report it.
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if let Ok(Frame::Transcript(t)) = Frame::parse(&line) {
            a_got_tx.send(t).unwrap();
        }
        a_drop_rx.recv().unwrap();
        // returning drops `stream` → A disconnects (the last wrapper exits)
    });

    // Wrapper B: registers second (newest → active), then closes on cue — the
    // active wrapper leaving, which must hand off to A rather than focused-window.
    let (b_registered_tx, b_registered_rx) = mpsc::channel::<()>();
    let (b_drop_tx, b_drop_rx) = mpsc::channel::<()>();
    let b_sock = sock.clone();
    let client_b = thread::spawn(move || {
        let mut stream = UnixStream::connect(&b_sock).unwrap();
        stream.write_all(b"register-sink\n").unwrap();
        b_registered_tx.send(()).unwrap();
        b_drop_rx.recv().unwrap();
        // returning drops `stream` → B (the active wrapper) disconnects
        drop(stream);
    });

    let mut registry = SinkRegistry::new();

    // Accept + register A first so registration order is deterministic (A older).
    let (conn_a, _) = listener.accept().unwrap();
    let mut reader_a = BufReader::new(conn_a.try_clone().unwrap());
    expect_register(&mut reader_a);
    a_registered_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let id_a = registry.register();
    assert_eq!(registry.active(), Some(id_a));

    // Accept + register B second — the newest wrapper becomes the active sink.
    let (conn_b, _) = listener.accept().unwrap();
    let mut reader_b = BufReader::new(conn_b.try_clone().unwrap());
    expect_register(&mut reader_b);
    b_registered_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let id_b = registry.register();
    assert_eq!(
        registry.active(),
        Some(id_b),
        "the newest wrapper is the active Delivery sink",
    );

    // Close the ACTIVE wrapper (B). The daemon sees EOF and deregisters it; the
    // active sink must hand off to the still-live A — NOT the focused-window sink.
    b_drop_tx.send(()).unwrap();
    let mut tail = String::new();
    let _ = reader_b.read_line(&mut tail); // 0 once B drops
    registry.deregister(id_b);
    assert_eq!(
        registry.active(),
        Some(id_a),
        "closing the active wrapper hands off to the other live wrapper, not no-sink",
    );
    assert_eq!(registry.wrapper_count(), 1, "one wrapper still registered");

    // A transcript triggered NOW binds to the active sink (A) and drains to it via
    // the real queue + routing — exactly the daemon's drain path.
    let mut queue = DeliveryQueue::new();
    let seq = queue.enqueue();
    let bound = registry.active();
    assert_eq!(bound, Some(id_a));
    queue.set_ready(seq, transcript.to_owned());

    let (head_seq, head_text) = queue
        .next_to_type()
        .map(|(s, t)| (s, t.to_owned()))
        .unwrap();
    assert_eq!(head_seq, seq);
    match registry.route(bound) {
        Route::Wrapper(wid) => {
            assert_eq!(wid, id_a, "routes to the surviving wrapper A");
            let mut conn_a_w = conn_a.try_clone().unwrap();
            conn_a_w
                .write_all(format!("{}\n", Frame::Transcript(head_text).encode()).as_bytes())
                .unwrap();
            conn_a_w.flush().unwrap();
        }
        other => panic!("expected a wrapper route to A, got {other:?}"),
    }
    queue.resolve(head_seq);

    // A received exactly the transcript handed off to it.
    let received = a_got_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(received, transcript);

    // Now close the LAST wrapper (A): only now is there no active sink.
    a_drop_tx.send(()).unwrap();
    let mut tail_a = String::new();
    let _ = reader_a.read_line(&mut tail_a);
    registry.deregister(id_a);
    assert_eq!(
        registry.active(),
        None,
        "no active sink remains only when the last wrapper exits",
    );
    assert_eq!(registry.wrapper_count(), 0);

    client_a.join().unwrap();
    client_b.join().unwrap();
    let _ = std::fs::remove_file(&sock);
}
