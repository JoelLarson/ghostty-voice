//! Integration test: a `talk-to` **wrapper sink** registers over the control
//! socket, becomes the single active **Delivery sink**, receives pushed frames
//! (a state update and a Transcript), and on disconnect the **focused-window
//! sink** reactivates.
//!
//! Like `ordered_drain.rs`, this exercises the real composition the daemon relies
//! on — the real newline `Frame` protocol and the real `SinkRegistry` over a real
//! Unix socket — with a test double only at the socket peer (the in-test daemon).
//! No GPU, mic, or ydotool involved.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ghostty_voice_core::protocol::{Command, Frame, State};
use ghostty_voice_core::sink::{ActiveSink, SinkRegistry};

#[test]
fn a_registered_wrapper_sink_becomes_active_and_receives_pushed_frames() {
    let sock = std::env::temp_dir().join(format!(
        "gv-sink-reg-{}-{:?}.sock",
        std::process::id(),
        thread::current().id(),
    ));
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    // Reports the daemon-side active-sink observations back to the test body.
    let (tx, rx) = mpsc::channel::<bool>();

    // The "daemon": accept the connection, and if the first line is
    // `register-sink`, register a real wrapper sink (the active sink becomes the
    // wrapper), push a `state` frame and a `transcript` frame down the persistent
    // connection, then deregister on client disconnect (focused-window returns).
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();

        let mut registry = SinkRegistry::new();
        assert_eq!(
            registry.active(),
            ActiveSink::FocusedWindow,
            "the floor before any wrapper registers"
        );

        if matches!(Command::parse(&first), Ok(Command::RegisterSink)) {
            let id = registry.register();
            // Exactly one active sink, and it is now the wrapper.
            tx.send(registry.active() == ActiveSink::Wrapper(id))
                .unwrap();

            // Push live state, then a finished Transcript — newline-framed.
            stream
                .write_all(format!("{}\n", Frame::State(State::Recording).encode()).as_bytes())
                .unwrap();
            stream
                .write_all(
                    format!(
                        "{}\n",
                        Frame::Transcript("create a function that reverses a string".to_owned())
                            .encode()
                    )
                    .as_bytes(),
                )
                .unwrap();
            stream.flush().unwrap();

            // Block until the client drops (read returns 0), then deregister.
            let mut tail = String::new();
            let _ = reader.read_line(&mut tail);
            registry.deregister(id);
            tx.send(registry.active() == ActiveSink::FocusedWindow)
                .unwrap();
        }
    });

    // The wrapper-sink client (what `talk-to` does on launch): connect, register,
    // and read the pushed frames back, decoding with the real protocol.
    let mut client = UnixStream::connect(&sock).unwrap();
    client.write_all(b"register-sink\n").unwrap();
    let mut creader = BufReader::new(client.try_clone().unwrap());

    let mut state_line = String::new();
    creader.read_line(&mut state_line).unwrap();
    assert_eq!(
        Frame::parse(&state_line),
        Ok(Frame::State(State::Recording)),
        "the state frame must round-trip over the socket",
    );

    let mut transcript_line = String::new();
    creader.read_line(&mut transcript_line).unwrap();
    assert_eq!(
        Frame::parse(&transcript_line),
        Ok(Frame::Transcript(
            "create a function that reverses a string".to_owned()
        )),
        "the Transcript push frame must round-trip over the socket",
    );

    // The daemon observed the wrapper as the single active sink on registration.
    assert!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "the wrapper sink must become the active Delivery sink on register",
    );

    // Drop the client → the daemon deregisters and the focused-window sink returns.
    drop(creader);
    drop(client);
    assert!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        "the focused-window sink must reactivate when the wrapper disconnects",
    );

    server.join().unwrap();
    let _ = std::fs::remove_file(&sock);
}
