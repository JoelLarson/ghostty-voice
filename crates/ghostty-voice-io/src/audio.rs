//! Audio capture boundary adapter.
//!
//! Records a 16 kHz mono s16 WAV (Whisper's native format) via `pw-record`.
//! pw-record is stopped with SIGINT so it can flush and backpatch the WAV
//! header — SIGKILL would truncate the file.

use std::io::BufRead;
use std::path::Path;
use std::process::{Child, Command};
use std::time::Duration;

use anyhow::{Context, Result};
use ghostty_voice_core::filter::pcm_duration;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

/// Start capturing from `device` ("default" for the default source) into
/// `out`. The caller stops it with [`stop_recorder`]. Used by the daemon,
/// where the stop signal comes from a socket command, not Enter.
pub fn spawn_recorder(device: &str, out: &Path) -> Result<Child> {
    let mut cmd = Command::new("pw-record");
    cmd.arg("--rate=16000")
        .arg("--channels=1")
        .arg("--format=s16");
    if device != "default" {
        cmd.arg(format!("--target={device}"));
    }
    cmd.arg(out);
    cmd.spawn()
        .context("failed to start pw-record (is PipeWire running?)")
}

/// Stop a recorder cleanly (SIGINT, then wait for exit).
pub fn stop_recorder(child: &mut Child) -> Result<()> {
    let pid = Pid::from_raw(child.id() as i32);
    let _ = kill(pid, Signal::SIGINT);
    child.wait().context("pw-record did not exit cleanly")?;
    Ok(())
}

/// Record until the user presses Enter (the S1 standalone flow).
pub fn record_to_wav(device: &str, out: &Path) -> Result<()> {
    let mut child = spawn_recorder(device, out)?;
    println!("● Recording — press Enter to stop.");
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok();
    stop_recorder(&mut child)
}

/// Audio duration of a 16 kHz mono s16 WAV, read from its `data` chunk size.
/// Used by the daemon to apply the sub-min-duration filter (S4) without a full
/// WAV decoder. The pure length-from-bytes math lives in core
/// (`filter::pcm_duration`); this is just the RIFF `data`-chunk scan.
pub fn wav_duration(wav: &Path) -> Result<Duration> {
    let bytes = std::fs::read(wav).with_context(|| format!("reading WAV {}", wav.display()))?;
    let data_len =
        wav_data_len(&bytes).with_context(|| format!("no RIFF data chunk in {}", wav.display()))?;
    Ok(pcm_duration(data_len))
}

/// Find the `data` chunk's byte length in a canonical RIFF/WAVE file. Returns
/// `None` if the header is absent or malformed.
fn wav_data_len(bytes: &[u8]) -> Option<u64> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as u64;
        if id == b"data" {
            // Clamp to what's actually present (a streamed header may overstate).
            let available = (bytes.len() - (pos + 8)) as u64;
            return Some(size.min(available));
        }
        // Chunks are word-aligned: skip the body plus any pad byte.
        pos += 8 + size as usize + (size as usize & 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal canonical 16 kHz mono s16 WAV with `data_bytes` of PCM payload.
    fn wav_with_data(data_bytes: u32) -> Vec<u8> {
        let mut w = Vec::new();
        let fmt_size: u32 = 16;
        let riff_size = 4 + (8 + fmt_size) + (8 + data_bytes);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&riff_size.to_le_bytes());
        w.extend_from_slice(b"WAVE");
        w.extend_from_slice(b"fmt ");
        w.extend_from_slice(&fmt_size.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&16_000u32.to_le_bytes()); // sample rate
        w.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
        w.extend_from_slice(&2u16.to_le_bytes()); // block align
        w.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data_bytes.to_le_bytes());
        w.extend(std::iter::repeat_n(0u8, data_bytes as usize));
        w
    }

    #[test]
    fn reads_data_len_from_canonical_wav() {
        // 1 s of audio = 32000 bytes.
        assert_eq!(wav_data_len(&wav_with_data(32_000)), Some(32_000));
    }

    #[test]
    fn data_len_clamps_to_bytes_present() {
        // Header claims 32000 but only 8 bytes follow (a truncated/streamed file).
        let mut w = wav_with_data(0);
        // Overwrite the data size field to claim 32000 while no payload follows.
        let len = w.len();
        let size_pos = len - 4;
        w[size_pos..].copy_from_slice(&32_000u32.to_le_bytes());
        assert_eq!(wav_data_len(&w), Some(0));
    }

    #[test]
    fn rejects_non_wav() {
        assert_eq!(wav_data_len(b"not a wav file at all"), None);
    }

    #[test]
    fn wav_duration_of_one_second_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("ghostty-voice-wavdur-it.wav");
        std::fs::write(&path, wav_with_data(32_000)).unwrap();
        assert_eq!(wav_duration(&path).unwrap(), Duration::from_secs(1));
        let _ = std::fs::remove_file(&path);
    }
}
