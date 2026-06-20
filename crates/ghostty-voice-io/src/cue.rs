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
//! asset-free) resolves to its `.oga` under the freedesktop sound theme and is
//! played via `paplay`; an explicit **file** path is played via `paplay` too.
//! Everything goes through `paplay` (PipeWire) — no `libcanberra` (S8). This
//! adapter is the dumb dispatcher.

use std::ffi::OsString;
use std::process::Command;

use anyhow::{Result, anyhow, bail};
use ghostty_voice_core::cue::{CueSource, resolve};

/// Where the freedesktop sound theme keeps its stereo `.oga` event sounds; a
/// bare/`theme:` event id resolves to `<dir>/<id>.oga` (shipped by
/// `sound-theme-freedesktop`).
const FREEDESKTOP_STEREO: &str = "/usr/share/sounds/freedesktop/stereo";

/// The external command (program + args) for a resolved cue source, or `None`
/// when the cue is disabled. Pure selection — split out so the dispatch is
/// testable without spawning a process. Everything plays via `paplay`: a file
/// path directly, a theme event by mapping it to its freedesktop `.oga`.
fn command_for(source: &CueSource) -> Option<(&'static str, Vec<OsString>)> {
    match source {
        CueSource::Disabled => None,
        CueSource::File(path) => Some(("paplay", vec![OsString::from(path)])),
        CueSource::ThemeEvent(event) => Some((
            "paplay",
            vec![OsString::from(format!("{FREEDESKTOP_STEREO}/{event}.oga"))],
        )),
    }
}

/// Play the configured cue. An empty value is a no-op (cue disabled). A `theme:`
/// or bare-token value plays the matching freedesktop `.oga` via `paplay`; a path
/// plays that sound file via `paplay`. Best-effort: a missing player or sound
/// should not abort delivery, so callers typically ignore the error — but it is
/// surfaced for the rare caller that wants it.
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
    fn a_theme_event_maps_to_its_freedesktop_oga_via_paplay() {
        // No libcanberra (S8): a theme event id resolves to its .oga and plays
        // through paplay like any other file.
        let (prog, args) = command_for(&CueSource::ThemeEvent("bell".into())).unwrap();
        assert_eq!(prog, "paplay");
        assert_eq!(
            args,
            vec![OsString::from("/usr/share/sounds/freedesktop/stereo/bell.oga")]
        );
    }
}
