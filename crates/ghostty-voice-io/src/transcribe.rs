//! Transcription transport boundary adapter.
//!
//! POSTs a WAV to whisper-server's `/inference` endpoint as multipart form
//! data and returns the raw JSON response body for the core parser.

use std::path::Path;

use anyhow::{Context, Result};

/// POST `wav` to `http://<host>:<port>/inference` and return the response body.
pub fn post_inference(host: &str, port: u16, wav: &Path) -> Result<String> {
    let url = format!("http://{host}:{port}/inference");
    let form = reqwest::blocking::multipart::Form::new()
        .file("file", wav)
        .context("failed to attach WAV to request")?
        .text("response_format", "json");

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .with_context(|| format!("failed to reach whisper-server at {url} (is it running?)"))?
        .error_for_status()
        .context("whisper-server returned an error status")?;

    response
        .text()
        .context("failed to read whisper-server response body")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    /// Stand-in whisper-server: accept one request, drain it, reply with the
    /// given JSON body. No GPU or whisper.cpp involved.
    fn mock_server(body: &'static str) -> (String, u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let mut buf = [0u8; 8192];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => continue,
                    Err(_) => break, // client finished sending; timed out waiting for more
                }
            }
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

    #[test]
    fn posts_wav_and_returns_response_body() {
        let (host, port, server) = mock_server(r#"{"text": " hi there "}"#);

        let wav = std::env::temp_dir().join("ghostty-voice-transport-it.wav");
        std::fs::write(&wav, b"RIFFdummywavbytes").unwrap();

        let body = post_inference(&host, port, &wav).unwrap();
        server.join().unwrap();

        assert_eq!(body, r#"{"text": " hi there "}"#);
        // End-to-end with the core parser: clean, single-line transcript.
        let text = ghostty_voice_core::transcript::parse_transcript(&body).unwrap();
        assert_eq!(text, "hi there");
    }
}
