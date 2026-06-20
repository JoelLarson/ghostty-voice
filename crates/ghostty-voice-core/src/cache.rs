//! Cache retention policy (S3).
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
}
