//! Integration test: a `talk-to` **wrapper sink** receives a pushed download
//! `state` frame and renders it on its **status strip** as `downloading 42%`.
//!
//! Like `sink_registration.rs`, this exercises the real composition the daemon
//! relies on — the real newline `Frame` protocol over a real Unix socket, with a
//! test double only at the socket peer (the in-test daemon). It proves the
//! download percent reaches a wrapper sink through the *same* `Frame::State`
//! mechanism as every other state, and that the client renders it via
//! `State::label()` exactly as `talk-to`'s `apply_frame` does. No GPU, mic, or
//! network involved.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;

use ghostty_voice_core::protocol::{Frame, State};
use ghostty_voice_core::sink::{ActiveSink, SinkRegistry};

#[test]
fn a_pushed_download_percent_renders_on_the_strip() {
    let sock = std::env::temp_dir().join(format!(
        "gv-download-progress-{}-{:?}.sock",
        std::process::id(),
        thread::current().id(),
    ));
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    // The "daemon": register the connecting client as the active wrapper sink,
    // then push the percent-unknown phase and a concrete `downloading 42` percent
    // down the persistent connection — exactly the `Frame::State` traffic the real
    // daemon broadcasts as the download streams in.
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();

        let mut registry = SinkRegistry::new();
        let id = registry.register();
        assert_eq!(registry.active(), ActiveSink::Wrapper(id));

        stream
            .write_all(format!("{}\n", Frame::State(State::Downloading(None)).encode()).as_bytes())
            .unwrap();
        stream
            .write_all(
                format!("{}\n", Frame::State(State::Downloading(Some(42))).encode()).as_bytes(),
            )
            .unwrap();
        stream.flush().unwrap();

        // Block until the client drops, then deregister.
        let mut tail = String::new();
        let _ = reader.read_line(&mut tail);
        registry.deregister(id);
    });

    // The wrapper-sink client (what `talk-to` does on launch): connect, register,
    // read the pushed frames, decode with the real protocol, and render each via
    // `State::label()` — the strip token `talk-to`'s `apply_frame` paints.
    let mut client = UnixStream::connect(&sock).unwrap();
    client.write_all(b"register-sink\n").unwrap();
    let mut creader = BufReader::new(client.try_clone().unwrap());

    let mut unknown_line = String::new();
    creader.read_line(&mut unknown_line).unwrap();
    let unknown = match Frame::parse(&unknown_line) {
        Ok(Frame::State(s)) => s,
        other => panic!("expected a state frame, got {other:?}"),
    };
    assert_eq!(unknown, State::Downloading(None));
    assert_eq!(
        unknown.label(),
        "downloading",
        "the percent-unknown phase shows a bare `downloading` (never a bogus number)",
    );

    let mut percent_line = String::new();
    creader.read_line(&mut percent_line).unwrap();
    let percent = match Frame::parse(&percent_line) {
        Ok(Frame::State(s)) => s,
        other => panic!("expected a state frame, got {other:?}"),
    };
    assert_eq!(
        percent,
        State::Downloading(Some(42)),
        "the pushed `state downloading 42` round-trips over the socket",
    );
    assert_eq!(
        percent.label(),
        "downloading 42%",
        "the strip renders the carried percent",
    );

    drop(creader);
    drop(client);
    server.join().unwrap();
    let _ = std::fs::remove_file(&sock);
}
