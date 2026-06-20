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
