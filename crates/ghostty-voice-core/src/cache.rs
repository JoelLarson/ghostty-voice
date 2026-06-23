//! Cache retention policy.
//!
//! The WAV and transcript caches are count-capped: on each new entry the
//! oldest beyond the cap are pruned. Entries are named by ISO timestamp, so a
//! lexical sort is chronological; this pure policy decides which to drop.

/// Given cache entries sorted oldest-first and a keep cap, return the slice of
/// oldest entries to prune.
pub fn stale_entries<T>(sorted_oldest_first: &[T], keep: usize) -> &[T] {
    let remove = sorted_oldest_first.len().saturating_sub(keep);
    &sorted_oldest_first[..remove]
}

/// Build a cache filename from a UTC timestamp broken into its parts and an
/// extension, e.g. `2026-06-20T08-15-30.123Z.wav`. Colons are avoided (they're
/// awkward on disk) so a lexical sort stays chronological. The clock read
/// happens at the IO boundary; the formatting is pure and tested here.
#[allow(clippy::too_many_arguments)]
pub fn iso_filename(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: u32,
    ext: &str,
) -> String {
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}.{millis:03}Z.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_pruned_under_the_cap() {
        let entries = ["a", "b", "c"];
        assert!(stale_entries(&entries, 5).is_empty());
    }

    #[test]
    fn nothing_pruned_exactly_at_the_cap() {
        let entries = ["a", "b", "c"];
        assert!(stale_entries(&entries, 3).is_empty());
    }

    #[test]
    fn oldest_pruned_over_the_cap() {
        let entries = ["a", "b", "c", "d", "e"];
        assert_eq!(stale_entries(&entries, 3), &["a", "b"]);
    }

    #[test]
    fn keep_zero_prunes_everything() {
        let entries = ["a", "b"];
        assert_eq!(stale_entries(&entries, 0), &["a", "b"]);
    }

    #[test]
    fn iso_filename_is_zero_padded_and_colon_free() {
        assert_eq!(
            iso_filename(2026, 6, 20, 8, 15, 30, 123, "wav"),
            "2026-06-20T08-15-30.123Z.wav",
        );
    }

    #[test]
    fn iso_filenames_sort_chronologically_lexically() {
        let earlier = iso_filename(2026, 6, 20, 8, 15, 30, 123, "txt");
        let later = iso_filename(2026, 6, 20, 8, 15, 30, 124, "txt");
        let next_minute = iso_filename(2026, 6, 20, 8, 16, 0, 0, "txt");
        assert!(earlier < later);
        assert!(later < next_minute);
    }
}
