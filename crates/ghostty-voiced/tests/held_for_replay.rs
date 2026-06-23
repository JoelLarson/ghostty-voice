//! Integration test: a Transcript bound to a **wrapper sink** that dies before
//! delivery is **Held-for-replay** — never delivered to the dead wrapper and
//! never silently redirected into the now-focused window — and stays recoverable
//! from the cache (the write-before-deliver safety net that `replay-last` reads).
//!
//! Mirrors `ordered_drain.rs`: the real `SinkRegistry` routing, the real
//! `DeliveryQueue`, and the real on-disk transcript cache, over a real Unix
//! socket whose peer (the wrapper) drops mid-flight. Test double only at the
//! socket peer.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use ghostty_voice_core::protocol::Command;
use ghostty_voice_core::queue::DeliveryQueue;
use ghostty_voice_core::sink::{ActiveSink, Route, SinkRegistry};
use ghostty_voice_io::cache::{latest_transcript, store_transcript};

/// A unique scratch transcripts dir, removed on drop.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gv-held-{}-{:?}",
            std::process::id(),
            thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_transcript_bound_to_a_dead_wrapper_is_held_and_recoverable_not_redirected() {
    let scratch = Scratch::new();
    let tdir = &scratch.0;

    let sock = std::env::temp_dir().join(format!(
        "gv-held-{}-{:?}.sock",
        std::process::id(),
        thread::current().id(),
    ));
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    // The wrapper-sink client: register, then immediately die (drop the stream).
    let csock = sock.clone();
    let client = thread::spawn(move || {
        let mut s = UnixStream::connect(&csock).unwrap();
        s.write_all(b"register-sink\n").unwrap();
        // returning drops `s` → the connection closes (the wrapper "crashes")
    });

    // The "daemon": accept, register the wrapper, bind an utterance to it at
    // trigger time, then observe the wrapper die before the transcript is ready.
    let (conn, _) = listener.accept().unwrap();
    let mut reader = BufReader::new(conn.try_clone().unwrap());
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    assert!(matches!(
        Command::parse(&first),
        Ok(Command::RegisterSink(_))
    ));

    let mut registry = SinkRegistry::new();
    let id = registry.register();
    // Trigger-time binding: captured while the wrapper is still alive.
    let bound = registry.active();
    assert_eq!(bound, ActiveSink::Wrapper(id));

    // The wrapper dies: block until EOF on the connection, then deregister — the
    // focused-window sink reactivates (the default floor).
    let mut tail = String::new();
    let _ = reader.read_line(&mut tail); // 0 once the client drops
    registry.deregister(id);
    assert_eq!(
        registry.active(),
        ActiveSink::FocusedWindow,
        "focused-window sink reactivates when the wrapper disconnects",
    );
    assert!(!registry.is_live(id));

    // The transcript becomes ready. Cache it BEFORE attempting delivery — the
    // write-before-deliver safety net.
    let transcript = "refactor the parser into its own module";
    let mut queue = DeliveryQueue::new();
    let seq = queue.enqueue_at(Duration::from_secs(0));
    store_transcript(tdir, transcript, 5).unwrap();
    queue.set_ready(seq, transcript.to_owned());

    // Drain: the bound wrapper is dead → Held-for-replay. NOT Wrapper (not
    // delivered to the dead PTY) and NOT FocusedWindow (not redirected into the
    // window focused now), even though FocusedWindow is the active sink.
    let (head_seq, _text, _freshness) = queue
        .head_delivery(Duration::from_secs(1), Duration::from_secs(900))
        .unwrap();
    assert_eq!(head_seq, seq);
    assert_eq!(
        registry.route(bound),
        Route::Held,
        "a dead bound wrapper must Hold, never redirect to the current focus",
    );
    queue.resolve(head_seq);

    // The held transcript is recoverable via the cache (what `replay-last` reads).
    assert_eq!(
        latest_transcript(tdir).unwrap().as_deref(),
        Some(transcript),
        "the held Transcript must be recoverable from the cache",
    );

    client.join().unwrap();
    let _ = std::fs::remove_file(&sock);
}
