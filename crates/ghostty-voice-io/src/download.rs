//! First-run model-download boundary adapter.
//!
//! Fetches `ggml-large-v3.bin` (the ~3 GB model the package deliberately omits)
//! over HTTP, streaming to a temp file while computing its SHA-256, then
//! verifies the hash with the pure `ghostty_voice_core::model` logic and atomically
//! moves it into place. A mismatch leaves nothing installed. Progress is reported
//! through a caller-supplied callback so the daemon can `notify-send` percentages.
//!
//! The fetch/hashing IO lives here; the verification decision and the
//! healthy/missing/corrupt classification are pure core logic.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ghostty_voice_core::model::verify_download;
use sha2::{Digest, Sha256};

/// How a download progressed: bytes received so far and the total if the server
/// reported a `Content-Length`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub received: u64,
    pub total: Option<u64>,
}

impl Progress {
    /// Whole-percent complete, if a total is known.
    pub fn percent(&self) -> Option<u8> {
        self.total.and_then(|total| {
            if total == 0 {
                None
            } else {
                Some(((self.received.saturating_mul(100)) / total).min(100) as u8)
            }
        })
    }
}

/// Stream `url` to `dest`, computing SHA-256 as bytes arrive and verifying it
/// against `expected_sha` (empty = skip the gate). On success the file is in
/// place; on a hash mismatch nothing is installed and an error is returned.
///
/// `on_progress` is invoked periodically with cumulative byte counts so the
/// caller can surface download progress (e.g. via `notify-send`).
pub fn download_model(
    url: &str,
    dest: &Path,
    expected_sha: &str,
    mut on_progress: impl FnMut(Progress),
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    // Download to a sibling temp file first; only an end-to-end verified fetch
    // is promoted into the real path, so a crashed/corrupt download never leaves
    // a half-written model the daemon would try to load.
    let tmp = tmp_path(dest);
    let computed = stream_to_file(url, &tmp, &mut on_progress)
        .with_context(|| format!("downloading {url}"))?;

    if !verify_download(&computed, expected_sha) {
        let _ = fs::remove_file(&tmp);
        bail!(
            "downloaded model failed SHA-256 verification (expected {expected_sha}, got {computed}) — discarded"
        );
    }

    fs::rename(&tmp, dest)
        .with_context(|| format!("moving {} -> {}", tmp.display(), dest.display()))?;
    Ok(())
}

/// Stream the HTTP body to `path`, returning the lowercase-hex SHA-256 of all
/// bytes written.
fn stream_to_file(
    url: &str,
    path: &Path,
    on_progress: &mut impl FnMut(Progress),
) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None) // a multi-GB fetch can take a long time
        .build()
        .context("building HTTP client")?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("requesting {url}"))?
        .error_for_status()
        .context("download returned an error status")?;

    let total = response.content_length();
    let mut file =
        fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut received: u64 = 0;

    loop {
        let n = response.read(&mut buf).context("reading download stream")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .with_context(|| format!("writing {}", path.display()))?;
        hasher.update(&buf[..n]);
        received += n as u64;
        on_progress(Progress { received, total });
    }
    file.flush().ok();

    Ok(hex_lower(&hasher.finalize()))
}

/// Lowercase-hex encode a byte slice.
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The temp download path beside `dest` (`<dest>.download`).
fn tmp_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".download");
    dest.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// A stand-in HTTP server: accept one GET, reply 200 with `body` and a
    /// matching Content-Length. No real network/model involved.
    fn serve_once(body: &'static [u8]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // Drain the request line/headers enough to respond.
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
            stream.flush().unwrap();
        });
        (format!("http://{addr}/ggml-large-v3.bin"), handle)
    }

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "gv-download-it-{tag}-{}-{:?}",
                std::process::id(),
                thread::current().id(),
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
        fn dest(&self) -> PathBuf {
            self.0.join("models/ggml-large-v3.bin")
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sha_of(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        hex_lower(&h.finalize())
    }

    #[test]
    fn downloads_streams_to_disk_and_reports_progress() {
        let body: &'static [u8] = b"hello whisper model bytes";
        let (url, server) = serve_once(body);
        let s = Scratch::new("ok");
        let dest = s.dest();

        let mut last = Progress {
            received: 0,
            total: None,
        };
        download_model(&url, &dest, &sha_of(body), |p| last = p).unwrap();
        server.join().unwrap();

        // The file is in place with exactly the served bytes...
        assert_eq!(fs::read(&dest).unwrap(), body);
        // ...and progress reached the full size with a known total.
        assert_eq!(last.received, body.len() as u64);
        assert_eq!(last.total, Some(body.len() as u64));
        assert_eq!(last.percent(), Some(100));
        // ...and the temp file was promoted (not left behind).
        assert!(!tmp_path(&dest).exists());
    }

    #[test]
    fn an_empty_expected_hash_skips_verification() {
        let body: &'static [u8] = b"unverified but accepted";
        let (url, server) = serve_once(body);
        let s = Scratch::new("skip");
        let dest = s.dest();

        download_model(&url, &dest, "", |_| {}).unwrap();
        server.join().unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);
    }

    #[test]
    fn a_sha_mismatch_is_rejected_and_installs_nothing() {
        let body: &'static [u8] = b"corrupt fetch";
        let (url, server) = serve_once(body);
        let s = Scratch::new("corrupt");
        let dest = s.dest();

        let err = download_model(&url, &dest, "deadbeef", |_| {}).unwrap_err();
        server.join().unwrap();

        assert!(
            err.to_string().contains("SHA-256"),
            "error should mention the failed verification, got: {err}"
        );
        // Neither the real file nor the temp file survives a bad fetch.
        assert!(!dest.exists(), "no model installed on a hash mismatch");
        assert!(!tmp_path(&dest).exists(), "temp download cleaned up");
    }

    #[test]
    fn progress_percent_is_none_without_a_total() {
        let p = Progress {
            received: 10,
            total: None,
        };
        assert_eq!(p.percent(), None);
    }
}
