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

/// Start a hands-free VAD capture from `device` into `out` using `sox`: it
/// records a 16 kHz mono s16 WAV (the same contract as [`spawn_recorder`]) and
/// **self-terminates** after `silence_seconds` of trailing silence below
/// `threshold_pct`. The caller still tracks the [`Child`] so a `toggle` can stop
/// it early and the `max_recording_seconds` cap can backstop a never-speak hang.
pub fn spawn_vad_recorder(
    device: &str,
    out: &Path,
    silence_seconds: f32,
    threshold_pct: u32,
) -> Result<Child> {
    let argv = ghostty_voice_core::vad::record_args(
        &out.to_string_lossy(),
        silence_seconds,
        threshold_pct,
    );
    let mut cmd = Command::new("sox");
    cmd.args(&argv);
    if device != "default" {
        // sox reads the default PipeWire/ALSA source via `-d`; pin a device by
        // pointing AUDIODEV at it (honored by the `-d` default-device driver).
        cmd.env("AUDIODEV", device);
    }
    cmd.spawn()
        .context("failed to start sox (is sox installed?)")
}

/// Start a continuous-mode (S6) session capture from `device` into the clip
/// `out_template` (a path with `%n`, e.g. `<dir>/clip-%n.wav`, that `sox`
/// expands to the clip index). `sox` records one long session and splits it
/// into numbered, silence-bounded clips via `silence ... : newfile : restart`,
/// cutting a new clip after `clip_pause_seconds` of trailing silence below
/// `threshold_pct`. The daemon watches the dir, transcribes each finalized clip
/// in order, and ends the session itself on the long session-end silence; the
/// caller stops `sox` (SIGINT) on session-end or cancel via [`stop_recorder`].
pub fn spawn_continuous_recorder(
    device: &str,
    out_template: &Path,
    clip_pause_seconds: f32,
    threshold_pct: u32,
) -> Result<Child> {
    let argv = ghostty_voice_core::vad::continuous_record_args(
        &out_template.to_string_lossy(),
        clip_pause_seconds,
        threshold_pct,
    );
    let mut cmd = Command::new("sox");
    cmd.args(&argv);
    if device != "default" {
        cmd.env("AUDIODEV", device);
    }
    cmd.spawn()
        .context("failed to start sox (is sox installed?)")
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

    fn sox_available() -> bool {
        Command::new("sox")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Real `sox`, file source: synthesize a 0.5 s tone followed by 3 s of
    /// silence, then run the VAD `silence` effect (the exact args the recorder
    /// builds) over it. The trailing-silence trim must cut the file well below
    /// its 3.5 s length — proof the auto-stop effect fires on real sox.
    #[test]
    fn sox_silence_effect_auto_stops_on_trailing_silence() {
        if !sox_available() {
            eprintln!("skipping: sox not installed");
            return;
        }
        let dir = std::env::temp_dir();
        let src = dir.join(format!("gv-vad-src-{}.wav", std::process::id()));
        let out = dir.join(format!("gv-vad-out-{}.wav", std::process::id()));

        // 0.5 s of 440 Hz tone then 3 s of silence, in the WAV contract.
        let synth = Command::new("sox")
            .args([
                "-r",
                "16000",
                "-c",
                "1",
                "-b",
                "16",
                "-e",
                "signed-integer",
                "-n",
            ])
            .arg(&src)
            .args(["synth", "0.5", "sine", "440"])
            .args(["pad", "0", "3"])
            .status()
            .expect("sox synth");
        assert!(synth.success(), "sox synth failed");
        let full = wav_duration(&src).unwrap();
        assert!(full >= Duration::from_secs(3), "source should be ~3.5 s");

        // Apply the same trailing-silence trim the VAD recorder uses (2 s @ 3%).
        let effect = ghostty_voice_core::vad::silence_effect(2.0, 3);
        let status = Command::new("sox")
            .arg(&src)
            .arg(&out)
            .args(&effect)
            .status()
            .expect("sox silence");
        assert!(status.success(), "sox silence effect failed");

        let trimmed = wav_duration(&out).unwrap();
        assert!(
            trimmed < Duration::from_secs(2),
            "trailing silence should be trimmed: got {trimmed:?} from {full:?}",
        );

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&out);
    }

    /// Real `sox`, file source: a fully-silent input never arms the leading
    /// "above-threshold" trigger of the `silence` effect, so no audio is ever
    /// retained — the output is empty. On a *live* mic this is the "never speak"
    /// hang: sox blocks waiting for the trigger that never arms and never writes
    /// a stop. Proof the VAD trim cannot rescue dead silence and the daemon's
    /// `max_recording_seconds` cap is the necessary backstop.
    #[test]
    fn sox_never_auto_stops_when_no_speech_ever_rises_above_threshold() {
        if !sox_available() {
            eprintln!("skipping: sox not installed");
            return;
        }
        let dir = std::env::temp_dir();
        let src = dir.join(format!("gv-vad-silsrc-{}.wav", std::process::id()));
        let out = dir.join(format!("gv-vad-silout-{}.wav", std::process::id()));

        // 3 s of pure silence — never crosses the 3% threshold.
        let synth = Command::new("sox")
            .args([
                "-r",
                "16000",
                "-c",
                "1",
                "-b",
                "16",
                "-e",
                "signed-integer",
                "-n",
            ])
            .arg(&src)
            .args(["trim", "0", "3"])
            .status()
            .expect("sox synth silence");
        assert!(synth.success(), "sox synth failed");

        let effect = ghostty_voice_core::vad::silence_effect(2.0, 3);
        let status = Command::new("sox")
            .arg(&src)
            .arg(&out)
            .args(&effect)
            .status()
            .expect("sox silence");
        assert!(status.success(), "sox silence effect failed");

        // The leading trigger never armed, so nothing above threshold was ever
        // retained: the output holds no speech. On a live mic sox would instead
        // block forever — exactly the never-speak hang the time cap backstops.
        let kept = wav_duration(&out).unwrap();
        assert!(
            kept < Duration::from_millis(100),
            "dead silence retains no speech: kept {kept:?}",
        );

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&out);
    }

    /// Real `sox`, file source: the continuous-mode multi-clip split. A session
    /// of three tone bursts separated by 1.5 s pauses is fed through the exact
    /// `silence ... : newfile : restart` effect the continuous recorder builds.
    /// sox must spray it into multiple numbered clip files (`clip-1.wav`,
    /// `clip-2.wav`, …) — proof the splitter cuts on the clip-cut pause and
    /// reopens the next clip, so the daemon's dir-watcher sees ordered clips.
    #[test]
    fn sox_continuous_split_writes_multiple_numbered_clips() {
        if !sox_available() {
            eprintln!("skipping: sox not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("gv-cont-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.wav");

        // Three 0.6 s tones separated by 1.5 s of silence (well over a 1 s
        // clip-cut pause), plus trailing silence — a segmented "session".
        let synth = Command::new("sox")
            .args([
                "-r",
                "16000",
                "-c",
                "1",
                "-b",
                "16",
                "-e",
                "signed-integer",
                "-n",
            ])
            .arg(&src)
            .args(["synth", "0.6", "sine", "440"])
            .args(["pad", "0", "1.5"])
            .args(["repeat", "2"])
            .status()
            .expect("sox synth session");
        assert!(synth.success(), "sox synth failed");

        // The exact continuous split effect: cut after 1 s of silence @ 3%,
        // newfile + restart. Output template uses %n for the clip index.
        let template = dir.join("clip-%n.wav");
        let effect = ghostty_voice_core::vad::continuous_split_effect(1.0, 3);
        let status = Command::new("sox")
            .arg(&src)
            .arg(&template)
            .args(&effect)
            .status()
            .expect("sox split");
        assert!(status.success(), "sox split effect failed");

        // sox must have written more than one numbered clip, in order.
        let mut clips: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("clip-") && n.ends_with(".wav"))
                    .unwrap_or(false)
            })
            .collect();
        clips.sort();
        // sox zero-pads the index (clip-01.wav, clip-02.wav, …); a final
        // `restart` after the trailing silence may open one empty header-only
        // clip — the daemon skips zero-duration clips, so we count the real ones.
        let with_audio: Vec<&std::path::PathBuf> = clips
            .iter()
            .filter(|c| {
                wav_duration(c)
                    .map(|d| d > Duration::from_millis(100))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            with_audio.len() >= 3,
            "expected one clip per tone burst (3), got {} from {clips:?}",
            with_audio.len(),
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real `sox`: a SIGINT (what [`stop_recorder`] sends, the `toggle`
    /// manual-early-stop path) makes sox exit cleanly and flush a valid,
    /// readable WAV — proof an in-flight VAD recording can be cut short and its
    /// audio still survives for the pipeline.
    #[test]
    fn sigint_cleanly_stops_an_in_flight_sox_recording() {
        if !sox_available() {
            eprintln!("skipping: sox not installed");
            return;
        }
        let dir = std::env::temp_dir();
        let src = dir.join(format!("gv-vad-longsrc-{}.wav", std::process::id()));
        let out = dir.join(format!("gv-vad-stopout-{}.wav", std::process::id()));

        // A long tone source so sox is still running when we interrupt it.
        let synth = Command::new("sox")
            .args([
                "-r",
                "16000",
                "-c",
                "1",
                "-b",
                "16",
                "-e",
                "signed-integer",
                "-n",
            ])
            .arg(&src)
            .args(["synth", "30", "sine", "440"])
            .status()
            .expect("sox synth long");
        assert!(synth.success(), "sox synth failed");

        // Slow the read so the process is alive across the SIGINT (real-time).
        let mut child = Command::new("sox")
            .arg("--buffer")
            .arg("1024")
            .arg(&src)
            .arg(&out)
            .args(["trim", "0", "30"])
            .spawn()
            .expect("spawn sox");

        std::thread::sleep(Duration::from_millis(300));
        stop_recorder(&mut child).expect("clean SIGINT stop");

        // The flushed WAV must be readable (header backpatched on clean exit).
        assert!(wav_duration(&out).is_ok(), "stopped WAV must be readable");

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&out);
    }
}
