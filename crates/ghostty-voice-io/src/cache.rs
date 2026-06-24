//! Filesystem cache adapter.
//!
//! Persists WAV recordings and transcripts under the XDG cache dir with
//! ISO-timestamp names (`recordings/`, `transcripts/`), count-cap pruned on each
//! write. The retention policy and the name format are pure core logic
//! (`ghostty_voice_core::cache`); this adapter is the fs boundary around them.
//!
//! The transcript is written here *before* delivery is attempted, so a delivery
//! is never lost even if the wrapper-sink PTY write then fails — `replay-last`
//! reads the most recent transcript back via [`latest_transcript`].

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use ghostty_voice_core::cache::{iso_filename, stale_entries};

/// An ISO-timestamped filename for *now* (UTC) with the given extension.
fn now_filename(ext: &str) -> String {
    use chrono::{Datelike, Timelike};
    let t = Utc::now();
    iso_filename(
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.timestamp_subsec_millis(),
        ext,
    )
}

/// Directory entries (file names only) sorted oldest-first. ISO names sort
/// chronologically by their lexical order.
fn sorted_names(dir: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    names.sort();
    Ok(names)
}

/// Prune `dir` down to the newest `keep` entries (oldest removed first).
fn prune(dir: &Path, keep: usize) -> Result<()> {
    let names = sorted_names(dir)?;
    for stale in stale_entries(&names, keep) {
        let _ = fs::remove_file(dir.join(stale));
    }
    Ok(())
}

/// Copy `src` into `dir/<iso>.wav`, then prune to `keep`. Returns the new path.
pub fn store_wav(dir: &Path, src: &Path, keep: usize) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let dest = dir.join(now_filename("wav"));
    fs::copy(src, &dest)
        .with_context(|| format!("copying {} -> {}", src.display(), dest.display()))?;
    prune(dir, keep)?;
    Ok(dest)
}

/// Write `text` to `dir/<iso>.txt`, then prune to `keep`. Returns the new path.
pub fn store_transcript(dir: &Path, text: &str, keep: usize) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let dest = dir.join(now_filename("txt"));
    fs::write(&dest, text).with_context(|| format!("writing {}", dest.display()))?;
    prune(dir, keep)?;
    Ok(dest)
}

/// Read back the most-recent transcript text in `dir`, or `None` if empty.
/// Backs `replay-last`, which targets the latest transcript only.
pub fn latest_transcript(dir: &Path) -> Result<Option<String>> {
    let Some(name) = sorted_names(dir)?.pop() else {
        return Ok(None);
    };
    let path = dir.join(name);
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch dir for one test; removed on drop.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "gv-cache-it-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id(),
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn stores_and_reads_back_the_latest_transcript() {
        let s = Scratch::new("transcript-rt");
        store_transcript(&s.0, "first", 5).unwrap();
        // ensure a distinct, later ISO name
        std::thread::sleep(std::time::Duration::from_millis(5));
        store_transcript(&s.0, "second", 5).unwrap();

        assert_eq!(latest_transcript(&s.0).unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn latest_transcript_is_none_for_a_missing_or_empty_dir() {
        let s = Scratch::new("empty");
        assert_eq!(latest_transcript(&s.0).unwrap(), None);
    }

    #[test]
    fn store_wav_copies_the_audio_into_the_cache() {
        let s = Scratch::new("wav-rt");
        let src = std::env::temp_dir().join(format!("gv-src-{}.wav", std::process::id()));
        fs::write(&src, b"RIFFdummywav").unwrap();

        let dest = store_wav(&s.0, &src, 30).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"RIFFdummywav");
        assert!(dest.extension().unwrap() == "wav");
        let _ = fs::remove_file(&src);
    }

    #[test]
    fn transcript_cache_is_pruned_to_the_keep_cap() {
        let s = Scratch::new("prune");
        for i in 0..7 {
            store_transcript(&s.0, &format!("t{i}"), 3).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        let remaining = sorted_names(&s.0).unwrap();
        assert_eq!(remaining.len(), 3, "kept only the newest 3");
        // newest kept is the last written
        assert_eq!(latest_transcript(&s.0).unwrap().as_deref(), Some("t6"));
    }
}
