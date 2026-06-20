//! Transcription transport boundary adapter.
//!
//! POSTs a WAV to whisper-server's `/inference` endpoint as multipart form
//! data — together with the S4 accuracy-stack request params (`beam_size`,
//! `temperature`, `initial_prompt`) — and returns the raw JSON response body
//! for the core parser.

use std::path::Path;

use anyhow::{Context, Result};
use ghostty_voice_core::config::WhisperConfig;
use ghostty_voice_core::prompt::build_initial_prompt;

/// Whisper's `initial_prompt` token cap; the builder bounds the vocab to this.
const INITIAL_PROMPT_TOKEN_CAP: usize = 224;

/// whisper-server `/inference` request params beyond the WAV itself. The
/// accuracy stack (S4): a larger beam, deterministic temperature, and an
/// `initial_prompt` that biases the decoder toward the domain vocabulary.
#[derive(Debug, Clone)]
pub struct InferenceParams {
    pub beam_size: u32,
    pub temperature: f64,
    pub initial_prompt: String,
    /// True when the vocab overflowed the token cap and later terms were
    /// dropped — the caller should warn so biasing isn't silently lost.
    pub prompt_truncated: bool,
}

impl InferenceParams {
    /// Build the request params from `[whisper]` config, assembling the
    /// `initial_prompt` from `prompt_prefix` + `vocab`, bounded to the token cap.
    pub fn from_whisper_config(cfg: &WhisperConfig) -> Self {
        let prompt = build_initial_prompt(&cfg.prompt_prefix, &cfg.vocab, INITIAL_PROMPT_TOKEN_CAP);
        Self {
            beam_size: cfg.beam_size,
            temperature: cfg.temperature,
            initial_prompt: prompt.text,
            prompt_truncated: prompt.truncated,
        }
    }
}

/// POST `wav` to `http://<host>:<port>/inference` with `params` and return the
/// response body.
pub fn post_inference(
    host: &str,
    port: u16,
    wav: &Path,
    params: &InferenceParams,
) -> Result<String> {
    let url = format!("http://{host}:{port}/inference");
    let mut form = reqwest::blocking::multipart::Form::new()
        .file("file", wav)
        .context("failed to attach WAV to request")?
        .text("response_format", "json")
        .text("beam_size", params.beam_size.to_string())
        .text("temperature", params.temperature.to_string());
    if !params.initial_prompt.is_empty() {
        form = form.text("initial_prompt", params.initial_prompt.clone());
    }

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
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    /// Stand-in whisper-server: accept one request, capture its full raw bytes,
    /// reply with the given JSON body, and hand the captured request back over a
    /// channel so a test can assert on the multipart fields sent. No GPU or
    /// whisper.cpp involved.
    fn mock_server(
        body: &'static str,
    ) -> (String, u16, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                    Err(_) => break, // client finished sending; timed out waiting for more
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&request).into_owned());
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
        });
        (addr.ip().to_string(), addr.port(), rx, handle)
    }

    fn params() -> InferenceParams {
        InferenceParams {
            beam_size: 8,
            temperature: 0.0,
            initial_prompt: "Transcript of technical instructions. Vocabulary: ydotool".to_owned(),
            prompt_truncated: false,
        }
    }

    #[test]
    fn builds_params_from_whisper_config_with_bounded_prompt() {
        let cfg = WhisperConfig {
            beam_size: 8,
            temperature: 0.0,
            prompt_prefix: "Transcript of technical instructions.".to_owned(),
            vocab: vec!["ydotool".to_owned(), "Ghostty".to_owned()],
            ..WhisperConfig::default()
        };

        let p = InferenceParams::from_whisper_config(&cfg);
        assert_eq!(p.beam_size, 8);
        assert_eq!(p.temperature, 0.0);
        assert!(p.initial_prompt.contains("Vocabulary:"));
        assert!(p.initial_prompt.contains("ydotool"));
        assert!(p.initial_prompt.contains("Ghostty"));
        assert!(!p.prompt_truncated);
    }

    #[test]
    fn flags_truncation_when_vocab_overflows_the_cap() {
        // 300 terms cannot fit a 224-token initial_prompt.
        let cfg = WhisperConfig {
            vocab: (0..300).map(|i| format!("term{i}")).collect(),
            ..WhisperConfig::default()
        };
        let p = InferenceParams::from_whisper_config(&cfg);
        assert!(p.prompt_truncated);
    }

    #[test]
    fn posts_wav_and_returns_response_body() {
        let (host, port, _rx, server) = mock_server(r#"{"text": " hi there "}"#);

        let wav = std::env::temp_dir().join("ghostty-voice-transport-it.wav");
        std::fs::write(&wav, b"RIFFdummywavbytes").unwrap();

        let body = post_inference(&host, port, &wav, &params()).unwrap();
        server.join().unwrap();

        assert_eq!(body, r#"{"text": " hi there "}"#);
        // End-to-end with the core parser: clean, single-line transcript.
        let text = ghostty_voice_core::transcript::parse_transcript(&body).unwrap();
        assert_eq!(text, "hi there");
    }

    #[test]
    fn sends_beam_temperature_and_initial_prompt_as_multipart_fields() {
        let (host, port, rx, server) = mock_server(r#"{"text": "ok"}"#);

        let wav = std::env::temp_dir().join("ghostty-voice-transport-params-it.wav");
        std::fs::write(&wav, b"RIFFdummywavbytes").unwrap();

        post_inference(&host, port, &wav, &params()).unwrap();
        server.join().unwrap();

        let request = rx.recv().unwrap();
        // Each param is sent as its own multipart form-data part.
        assert!(request.contains(r#"name="beam_size""#));
        assert!(request.contains("\r\n\r\n8\r\n") || request.contains("\n\n8\r\n"));
        assert!(request.contains(r#"name="temperature""#));
        assert!(request.contains(r#"name="initial_prompt""#));
        assert!(request.contains("Vocabulary: ydotool"));
    }

    #[test]
    fn omits_initial_prompt_when_empty() {
        let (host, port, rx, server) = mock_server(r#"{"text": "ok"}"#);

        let wav = std::env::temp_dir().join("ghostty-voice-transport-noprompt-it.wav");
        std::fs::write(&wav, b"RIFFdummywavbytes").unwrap();

        let empty = InferenceParams {
            beam_size: 1,
            temperature: 0.0,
            initial_prompt: String::new(),
            prompt_truncated: false,
        };
        post_inference(&host, port, &wav, &empty).unwrap();
        server.join().unwrap();

        let request = rx.recv().unwrap();
        assert!(!request.contains(r#"name="initial_prompt""#));
    }
}
