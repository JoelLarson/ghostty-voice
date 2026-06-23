//! Integration test: the `register-sink` **version handshake** lets a
//! `talk-to` **wrapper sink** tell an *incompatible* daemon apart from an
//! unreachable one. A client sends its `PROTOCOL_VERSION`; a daemon that speaks a
//! different version refuses with an explicit `err incompatible …`, and an old
//! daemon answers `err unknown command …` — both classify as `incompatible`,
//! never as a generic offline. A matching version registers cleanly.
//!
//! Like the other daemon-level tests this exercises the real composition over a
//! real Unix socket — the real `Command::parse`, `version_compatible`, `Response`
//! encode, and the client-side `classify_first_line` — with a test double only at
//! the socket peer. No GPU/mic/ydotool.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;

use ghostty_voice_core::link::{Registration, classify_first_line};
use ghostty_voice_core::protocol::{
    Command, Frame, PROTOCOL_VERSION, Response, State, version_compatible,
};

/// Spawn a fake daemon that applies the real version check to one `register-sink`
/// and returns the strip path: connect, send `line`, read the first reply line,
/// classify it the way `talk-to` does.
fn handshake_outcome(
    daemon: impl FnOnce(&str, UnixStream) + Send + 'static,
    line: &str,
) -> Registration {
    let sock = std::env::temp_dir().join(format!(
        "gv-incompat-{}-{:?}.sock",
        std::process::id(),
        thread::current().id(),
    ));
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        daemon(&first, stream);
    });

    let mut client = UnixStream::connect(&sock).unwrap();
    client.write_all(format!("{line}\n").as_bytes()).unwrap();
    let mut reply = String::new();
    // Read just the first reply line (the handshake outcome).
    let mut reader = BufReader::new(client.try_clone().unwrap());
    reader.read_line(&mut reply).unwrap();
    // Drain any remainder so the server side can finish cleanly.
    let _ = client.read(&mut [0u8; 64]);

    server.join().unwrap();
    let _ = std::fs::remove_file(&sock);
    classify_first_line(&reply)
}

#[test]
fn a_version_mismatch_is_reported_as_incompatible() {
    // The real daemon-side decision: parse the versioned register-sink, and refuse
    // an incompatible version with an explicit `err incompatible …`.
    let daemon = |first: &str, mut stream: UnixStream| match Command::parse(first) {
        Ok(Command::RegisterSink(Some(v))) if !version_compatible(v, PROTOCOL_VERSION) => {
            let msg = format!(
                "incompatible protocol version (daemon speaks {PROTOCOL_VERSION}, client sent {v})"
            );
            stream
                .write_all(format!("{}\n", Response::Err(msg).encode()).as_bytes())
                .unwrap();
        }
        other => panic!("expected an incompatible versioned register-sink, got {other:?}"),
    };
    // A client one version ahead of the daemon.
    let line = format!("register-sink {}", PROTOCOL_VERSION + 1);
    assert_eq!(handshake_outcome(daemon, &line), Registration::Incompatible);
}

#[test]
fn an_old_daemon_that_doesnt_speak_the_handshake_is_incompatible_not_unreachable() {
    // An old daemon predates the versioned register-sink: it sees an unknown
    // command and answers a one-shot error. The client must read that as
    // incompatible (the stale-daemon-after-upgrade case), never as unreachable.
    let daemon = |first: &str, mut stream: UnixStream| {
        let _ = first;
        stream
            .write_all(b"err unknown command: register-sink 1\n")
            .unwrap();
    };
    assert_eq!(
        handshake_outcome(daemon, "register-sink 1"),
        Registration::Incompatible,
    );
}

#[test]
fn a_matching_version_registers_cleanly() {
    // A compatible client: the daemon accepts it and pushes a state frame (the
    // implicit registration ack), which the client reads as Registered.
    let daemon = |first: &str, mut stream: UnixStream| match Command::parse(first) {
        Ok(Command::RegisterSink(version)) => {
            assert!(version.is_none_or(|v| version_compatible(v, PROTOCOL_VERSION)));
            stream
                .write_all(format!("{}\n", Frame::State(State::Idle).encode()).as_bytes())
                .unwrap();
        }
        other => panic!("expected a register-sink, got {other:?}"),
    };
    let line = format!("register-sink {PROTOCOL_VERSION}");
    assert_eq!(handshake_outcome(daemon, &line), Registration::Registered);
}
