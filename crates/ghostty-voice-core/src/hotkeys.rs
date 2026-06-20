//! Pure helpers for `install-hotkeys` (S2).
//!
//! GNOME stores custom keybindings as a dconf array of object paths. Editing
//! it means parsing the existing list, merging ours in without clobbering,
//! and formatting it back. The `gsettings` calls live in the ctl binary; the
//! list-munging lives here, tested.

/// A keybinding to install: a unique slug, a display name, the command to run,
/// and the accelerator (e.g. `<Super>d`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub slug: &'static str,
    pub name: &'static str,
    pub command: String,
    pub binding: &'static str,
}

/// The dconf object path for a custom keybinding with the given slug.
pub fn keybinding_path(slug: &str) -> String {
    format!("/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/{slug}/")
}

/// Parse a dconf `as` array string (`['/a/', '/b/']`, `[]`, or `@as []`).
pub fn parse_path_list(s: &str) -> Vec<String> {
    let trimmed = s.trim().trim_start_matches("@as").trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|x| x.strip_suffix(']'))
        .unwrap_or("");
    inner
        .split(',')
        .map(|p| p.trim().trim_matches(['\'', '"']).to_owned())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Format a path list back into a dconf array literal.
pub fn format_path_list(paths: &[String]) -> String {
    if paths.is_empty() {
        return "@as []".to_owned();
    }
    let items: Vec<String> = paths.iter().map(|p| format!("'{p}'")).collect();
    format!("[{}]", items.join(", "))
}

/// Merge `add` into `existing`, preserving order and dropping duplicates.
pub fn merge_paths(existing: &[String], add: &[String]) -> Vec<String> {
    let mut out = existing.to_vec();
    for path in add {
        if !out.contains(path) {
            out.push(path.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_forms() {
        assert!(parse_path_list("@as []").is_empty());
        assert!(parse_path_list("[]").is_empty());
    }

    #[test]
    fn parses_populated_list() {
        assert_eq!(
            parse_path_list("['/a/', '/b/']"),
            vec!["/a/".to_owned(), "/b/".to_owned()],
        );
    }

    #[test]
    fn formats_empty_and_populated() {
        assert_eq!(format_path_list(&[]), "@as []");
        assert_eq!(
            format_path_list(&["/a/".to_owned(), "/b/".to_owned()]),
            "['/a/', '/b/']",
        );
    }

    #[test]
    fn merge_appends_without_duplicating() {
        let existing = vec!["/x/".to_owned()];
        let add = vec!["/x/".to_owned(), "/y/".to_owned()];
        assert_eq!(merge_paths(&existing, &add), vec!["/x/", "/y/"]);
    }

    #[test]
    fn keybinding_path_uses_the_slug() {
        assert!(keybinding_path("ghostty-voice-toggle").ends_with("ghostty-voice-toggle/"));
    }
}
