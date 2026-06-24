//! ghostty-voice walking skeleton.
//!
//! Record one utterance, transcribe it via a manually-started warm
//! whisper-server, and print the transcript. No daemon, no supervision, no
//! accuracy stack — those come later. The point of this skeleton is to prove
//! Vulkan transcription end-to-end and to capture a real warm-latency number.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use ghostty_voice_core::config::Config;
use ghostty_voice_core::transcript::parse_transcript;
use ghostty_voice_io::{audio, transcribe};

#[derive(Parser)]
#[command(name = "ghostty-voice", about = "Voice dictation walking skeleton")]
struct Cli {
    /// Path to config.toml (default: ~/.config/ghostty-voice/config.toml).
    #[arg(long)]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = match cli.config {
        Some(path) => path,
        None => ghostty_voice_core::config::config_path().context("HOME is not set")?,
    };
    let cfg = load_config(&config_path)?;

    let wav = std::env::temp_dir().join("ghostty-voice-s1.wav");
    audio::record_to_wav(&cfg.audio.device, &wav)?;

    // Time the round-trip — this is the warm-latency measurement.
    let started = Instant::now();
    let params = transcribe::InferenceParams::from_whisper_config(&cfg.whisper);
    let body = transcribe::post_inference(&cfg.whisper.host, cfg.whisper.port, &wav, &params)?;
    let transcript =
        parse_transcript(&body).map_err(|e| anyhow!("could not parse transcript: {e:?}"))?;
    eprintln!("transcribed in {:.2}s", started.elapsed().as_secs_f64());

    if transcript.is_empty() {
        eprintln!("no speech detected — nothing typed");
        return Ok(());
    }

    println!("{transcript}");
    Ok(())
}

fn load_config(path: &Path) -> Result<Config> {
    match std::fs::read_to_string(path) {
        Ok(s) => Config::from_toml_str(&s)
            .map_err(|e| anyhow!("invalid config {}: {e:?}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("no config at {} — using defaults", path.display());
            Ok(Config::default())
        }
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}
