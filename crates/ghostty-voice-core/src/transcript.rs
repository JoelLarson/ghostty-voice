//! Parse whisper-server's `/inference` JSON response into clean transcript
//! text ready for injection.
//!
//! whisper.cpp's server returns an OpenAI-style `{"text": "..."}` body.
//! whisper commonly prefixes a leading space and may split across lines; we
//! normalize all whitespace to single spaces and trim, so the result is a
//! single line safe to type (a stray newline would type as Enter).

use serde::Deserialize;

#[derive(Deserialize)]
struct InferenceResponse {
    text: String,
}

/// Why a `/inference` response could not be turned into a transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptError {
    /// The body was not valid JSON, or lacked a string `text` field.
    Parse(String),
}

/// Parse the `/inference` response body, returning the normalized transcript
/// (whitespace collapsed to single spaces, trimmed; may be empty).
pub fn parse_transcript(json: &str) -> Result<String, TranscriptError> {
    let response: InferenceResponse =
        serde_json::from_str(json).map_err(|e| TranscriptError::Parse(e.to_string()))?;
    Ok(response
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_field() {
        assert_eq!(
            parse_transcript(r#"{"text": "hello world"}"#).unwrap(),
            "hello world",
        );
    }

    #[test]
    fn trims_leading_whisper_space() {
        assert_eq!(
            parse_transcript(r#"{"text": " Rebase onto main."}"#).unwrap(),
            "Rebase onto main.",
        );
    }

    #[test]
    fn normalizes_internal_newlines_to_spaces() {
        // Raw string: the `\n` is a JSON escape (valid), decoding to a real
        // newline in the text, which the normalizer collapses to a space.
        assert_eq!(
            parse_transcript(r#"{"text": "line one\nline two"}"#).unwrap(),
            "line one line two",
        );
    }

    #[test]
    fn whitespace_only_text_is_empty() {
        assert_eq!(parse_transcript(r#"{"text": "  \n "}"#).unwrap(), "");
    }

    #[test]
    fn ignores_extra_fields() {
        assert_eq!(
            parse_transcript(r#"{"text": "ok", "segments": []}"#).unwrap(),
            "ok",
        );
    }

    #[test]
    fn rejects_non_json() {
        assert!(parse_transcript("not json at all").is_err());
    }

    #[test]
    fn rejects_missing_text_field() {
        assert!(parse_transcript(r#"{"segments": []}"#).is_err());
    }
}
