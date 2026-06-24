//! Integration test: `status` reports how many **wrapper sinks** are registered
//! over a real socket — so a user can confirm there is somewhere to deliver
//! (`wrappers>0`) without tailing journald.
//!
//! Like `ordered_drain.rs` / `sink_registration.rs`, this exercises the real
//! composition the daemon's status path relies on — the real `SinkRegistry`
//! (`wrapper_count`) and the real `StatusReport` encode/parse over a real Unix
//! socket — with a test double only at the socket peer. No GPU/mic.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;

use ghostty_voice_core::protocol::{State, StatusReport};
use ghostty_voice_core::sink::SinkRegistry;

/// Build the `status` reply from a registry exactly as the daemon does: the
/// state plus the count from `wrapper_count()`.
fn report_from(registry: &SinkRegistry, state: State) -> StatusReport {
    StatusReport {
        state,
        wrapper_count: registry.wrapper_count(),
    }
}

/// Run a one-shot fake daemon that answers a single `status` request from the
/// given registry, and return the parsed reply the client received.
fn ask_status(registry: SinkRegistry) -> StatusReport {
    let sock = std::env::temp_dir().join(format!(
        "gv-status-{}-{:?}.sock",
        std::process::id(),
        thread::current().id(),
    ));
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut req = String::new();
        stream.read_to_string(&mut req).unwrap();
        assert_eq!(req.trim(), "status");
        let report = report_from(&registry, State::Idle);
        stream
            .write_all(format!("{}\n", report.encode()).as_bytes())
            .unwrap();
    });

    let mut client = UnixStream::connect(&sock).unwrap();
    client.write_all(b"status\n").unwrap();
    client.shutdown(std::net::Shutdown::Write).ok();
    let mut reply = String::new();
    client.read_to_string(&mut reply).unwrap();
    server.join().unwrap();
    let _ = std::fs::remove_file(&sock);

    StatusReport::parse(reply.trim()).expect("status reply must parse")
}

#[test]
fn status_reports_the_registered_wrapper_count() {
    let mut registry = SinkRegistry::new();
    let _a = registry.register();
    let _b = registry.register(); // two wrappers; the newest is active

    let report = ask_status(registry);
    assert_eq!(report.wrapper_count, 2);
    assert_eq!(report.state, State::Idle);
}

#[test]
fn status_reports_zero_wrappers_when_none_registered() {
    let registry = SinkRegistry::new();
    let report = ask_status(registry);
    assert_eq!(report.wrapper_count, 0);
}
