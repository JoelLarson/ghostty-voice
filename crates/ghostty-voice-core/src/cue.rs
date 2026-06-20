//! Audio-cue source resolution (S7).
//!
//! The start/stop cues can come from two sources, decided here as pure logic so
//! the IO adapter stays a dumb dispatcher:
//!
//! - a **freedesktop theme event** (default) — a bare event id (e.g.
//!   `message-new-instant`) the IO adapter maps to its `.oga` under the
//!   freedesktop sound theme and plays via `paplay`; no binary assets to ship;
//! - an explicit **sound file** path, played via `paplay <path>`.
//!
//! Both play through `paplay` (PipeWire) — `libcanberra` is gone (S8). The config
//! value selects the source: a `theme:` prefix (or a bare event id with no path
//! separator) is a theme event; anything that looks like a path is a file. An
//! empty value disables the cue.

/// How a configured cue string should be played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CueSource {
    /// No cue (the config value was empty) — a deliberate, silent no-op.
    Disabled,
    /// Play `path` as a sound file via `paplay`.
    File(String),
    /// Play freedesktop theme event `id` (the IO adapter maps it to its `.oga`
    /// and plays it via `paplay`).
    ThemeEvent(String),
}

/// Resolve a configured cue string into its play strategy.
///
/// Rules: empty ⇒ `Disabled`. A `theme:` prefix forces a `ThemeEvent`. A value
/// containing a path separator or starting with `~` is a `File`. Otherwise a
/// bare token (e.g. `bell`, `message-new-instant`) is treated as a theme event,
/// since that is the asset-free default.
pub fn resolve(value: &str) -> CueSource {
    let v = value.trim();
    if v.is_empty() {
        return CueSource::Disabled;
    }
    if let Some(event) = v.strip_prefix("theme:") {
        return CueSource::ThemeEvent(event.trim().to_owned());
    }
    if v.contains('/') || v.starts_with('~') {
        return CueSource::File(v.to_owned());
    }
    CueSource::ThemeEvent(v.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_disabled() {
        assert_eq!(resolve(""), CueSource::Disabled);
        assert_eq!(resolve("   "), CueSource::Disabled);
    }

    #[test]
    fn an_absolute_or_relative_path_is_a_file() {
        assert_eq!(
            resolve("/usr/share/ghostty-voice/start.oga"),
            CueSource::File("/usr/share/ghostty-voice/start.oga".to_owned())
        );
        assert_eq!(
            resolve("~/sounds/stop.wav"),
            CueSource::File("~/sounds/stop.wav".to_owned())
        );
    }

    #[test]
    fn a_theme_prefixed_value_is_a_theme_event() {
        assert_eq!(
            resolve("theme:message-new-instant"),
            CueSource::ThemeEvent("message-new-instant".to_owned())
        );
        // The prefix is stripped and trimmed.
        assert_eq!(
            resolve("theme: bell"),
            CueSource::ThemeEvent("bell".to_owned())
        );
    }

    #[test]
    fn a_bare_token_defaults_to_a_theme_event() {
        // The asset-free default: a plain freedesktop event id needs no file.
        assert_eq!(resolve("bell"), CueSource::ThemeEvent("bell".to_owned()));
    }
}
