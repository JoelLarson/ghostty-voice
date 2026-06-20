//! First-run model-presence and integrity logic (S7).
//!
//! The ~3 GB `ggml-large-v3.bin` is **not** shipped in the package — the daemon
//! fetches it on first run. This module is the pure decision layer over that:
//! given whether the file is present and (optionally) its computed SHA-256
//! against the expected one, it classifies the model as healthy, missing, or
//! corrupt, and decides whether a download is needed.
//!
//! The IO (statting the file, hashing its bytes, the HTTP fetch) lives in the
//! `ghostty-voice-io` boundary; the rules — when a download is required, what
//! counts as corrupt — live here and are tested with real values, no mocks.

/// The HuggingFace download URL for `ggml-large-v3.bin` (the `ggerganov/
/// whisper.cpp` repo, `resolve/main`). The on-disk path is config-driven
/// (`[whisper].model_path`); this is where the first-run fetch pulls from.
pub const GGML_LARGE_V3_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin";

/// The expected SHA-256 of `ggml-large-v3.bin`, or empty to skip verification.
///
/// The canonical hash is published by HuggingFace alongside the LFS object; it
/// must be pinned here from the actual file (it cannot be fabricated offline,
/// and a wrong constant would reject every fetch). Until pinned on-hardware this
/// is empty, which means `verify_download` / `classify` accept by presence and
/// skip the SHA gate — the fetch still succeeds; only the integrity check is
/// deferred. See README "first run".
pub const GGML_LARGE_V3_SHA256: &str = "";

/// The classified state of the model file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStatus {
    /// The file is present and its SHA-256 matches the expected hash.
    Healthy,
    /// The file is absent — a first-run download is needed.
    Missing,
    /// The file is present but its SHA-256 does not match — a corrupt or
    /// truncated fetch. It must be removed and re-downloaded. Carries the
    /// computed hash for diagnostics.
    Corrupt { found: String },
}

/// Classify the model from a presence flag and an optional computed SHA-256.
///
/// `computed_sha` is `Some` only when the file is present and has been hashed;
/// callers that haven't hashed (or for which the file is absent) pass `None`.
/// SHA comparison is case-insensitive on the hex (hashers differ in casing).
pub fn classify(present: bool, computed_sha: Option<&str>, expected_sha: &str) -> ModelStatus {
    if !present {
        return ModelStatus::Missing;
    }
    // An empty expected hash means verification is disabled (no hash pinned yet):
    // presence alone is enough, the SHA gate is skipped.
    if expected_sha.is_empty() {
        return ModelStatus::Healthy;
    }
    match computed_sha {
        Some(found) if found.eq_ignore_ascii_case(expected_sha) => ModelStatus::Healthy,
        Some(found) => ModelStatus::Corrupt {
            found: found.to_owned(),
        },
        // Present but unhashed: treat as healthy-by-presence. SHA verification is
        // applied to *fetched* bytes (where corruption matters); re-hashing a
        // multi-GB local file on every boot is wasteful and not required to
        // decide whether a download is needed.
        None => ModelStatus::Healthy,
    }
}

/// True when a first-run download must run before the daemon can serve: the
/// file is absent or present-but-corrupt.
pub fn needs_download(status: &ModelStatus) -> bool {
    !matches!(status, ModelStatus::Healthy)
}

/// Verify freshly-fetched bytes' hash against the expected SHA-256
/// (case-insensitive hex). A mismatch means the download is corrupt and must be
/// discarded rather than installed. An empty `expected_sha` disables the gate
/// (no hash pinned yet) and accepts.
pub fn verify_download(computed_sha: &str, expected_sha: &str) -> bool {
    expected_sha.is_empty() || computed_sha.eq_ignore_ascii_case(expected_sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED: &str = "abc123";

    #[test]
    fn absent_file_is_missing_and_needs_download() {
        let status = classify(false, None, EXPECTED);
        assert_eq!(status, ModelStatus::Missing);
        assert!(needs_download(&status));
    }

    #[test]
    fn present_with_matching_sha_is_healthy_and_skips_download() {
        let status = classify(true, Some("abc123"), EXPECTED);
        assert_eq!(status, ModelStatus::Healthy);
        assert!(!needs_download(&status));
    }

    #[test]
    fn present_with_mismatched_sha_is_corrupt_and_needs_redownload() {
        let status = classify(true, Some("deadbeef"), EXPECTED);
        assert_eq!(
            status,
            ModelStatus::Corrupt {
                found: "deadbeef".to_owned()
            }
        );
        assert!(needs_download(&status));
    }

    #[test]
    fn sha_comparison_ignores_hex_casing() {
        // Different hashers emit upper/lower hex; a casing-only diff is a match.
        assert_eq!(
            classify(true, Some("ABC123"), "abc123"),
            ModelStatus::Healthy
        );
        assert!(verify_download("ABC123", "abc123"));
    }

    #[test]
    fn present_but_unhashed_is_healthy_by_presence() {
        // Skipping the expensive re-hash of a multi-GB local file: presence is
        // enough to decide no download is needed.
        assert_eq!(classify(true, None, EXPECTED), ModelStatus::Healthy);
    }

    #[test]
    fn verify_download_accepts_a_match_and_rejects_a_mismatch() {
        assert!(verify_download("abc123", "abc123"));
        assert!(!verify_download("deadbeef", "abc123"));
    }

    #[test]
    fn an_empty_expected_hash_disables_the_sha_gate() {
        // No hash pinned yet: presence is enough and any fetched bytes verify.
        assert_eq!(classify(true, Some("whatever"), ""), ModelStatus::Healthy);
        assert!(verify_download("whatever", ""));
    }

    #[test]
    fn the_pinned_hash_is_either_unset_or_lowercase_64_hex() {
        // Either deferred (empty) or a real lowercase 64-hex SHA-256 — never a
        // half-pasted constant that would silently reject every fetch.
        let h = GGML_LARGE_V3_SHA256;
        assert!(
            h.is_empty()
                || (h.len() == 64
                    && h.chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())),
            "pinned hash must be empty or a lowercase 64-hex string, got {h:?}"
        );
    }

    #[test]
    fn the_download_url_points_at_the_large_v3_lfs_object() {
        assert!(GGML_LARGE_V3_URL.ends_with("ggml-large-v3.bin"));
        assert!(GGML_LARGE_V3_URL.starts_with("https://huggingface.co/"));
    }
}
