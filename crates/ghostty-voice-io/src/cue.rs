//! Audio-cue boundary adapter (S3/S7).
//!
//! Plays the hot-path start/stop cues. A start cue fires when recording begins
//! ("now listening"); the stop cue fires when recording ends and on an
//! empty/silence transcript ("working / done") — there is no distinct error
//! sound. An empty configured value means "no cue": a deliberate no-op, never an
//! error, so disabling a cue never breaks delivery.
//!
//! The cue source (S7) is resolved by pure core logic
//! ([`ghostty_voice_core::cue`]): a freedesktop **theme event** (default,
//! asset-free) is played via `canberra-gtk-play -i <event>`; an explicit **file**
//! path is played via `paplay <path>`. This adapter is the dumb dispatcher.

use std::ffi::OsString;
use std::process::Command;

use anyhow::{Result, anyhow, bail};
use ghostty_voice_core::cue::{CueSource, resolve};

/// The external command (program + args) for a resolved cue source, or `None`
/// when the cue is disabled. Pure selection — split out so the dispatch is
/// testable without spawning a process.
fn command_for(source: &CueSource) -> Option<(&'static str, Vec<OsString>)> {
    match source {
        CueSource::Disabled => None,
        CueSource::File(path) => Some(("paplay", vec![OsString::from(path)])),
        CueSource::ThemeEvent(event) => Some((
            "canberra-gtk-play",
            vec![OsString::from("-i"), OsString::from(event)],
        )),
    }
}

/// Play the configured cue. An empty value is a no-op (cue disabled). A `theme:`
/// or bare-token value plays a freedesktop theme event via `canberra-gtk-play`;
/// a path plays a sound file via `paplay`. Best-effort: a missing player or
/// sound should not abort delivery, so callers typically ignore the error — but
/// it is surfaced for the rare caller that wants it.
pub fn play(cue: &str) -> Result<()> {
    let Some((program, args)) = command_for(&resolve(cue)) else {
        return Ok(()); // disabled
    };
    let status = Command::new(program)
        .args(&args)
        .status()
        .map_err(|e| anyhow!("failed to run {program}: {e}"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_value_is_a_silent_noop() {
        // No player is spawned and no error is raised — disabling a cue is safe.
        assert!(play("").is_ok());
        assert_eq!(command_for(&CueSource::Disabled), None);
    }

    #[test]
    fn a_file_path_dispatches_to_paplay() {
        let (prog, args) = command_for(&CueSource::File("/x/start.oga".into())).unwrap();
        assert_eq!(prog, "paplay");
        assert_eq!(args, vec![OsString::from("/x/start.oga")]);
    }

    #[test]
    fn a_theme_event_dispatches_to_canberra() {
        let (prog, args) = command_for(&CueSource::ThemeEvent("bell".into())).unwrap();
        assert_eq!(prog, "canberra-gtk-play");
        assert_eq!(args, vec![OsString::from("-i"), OsString::from("bell")]);
    }
}
