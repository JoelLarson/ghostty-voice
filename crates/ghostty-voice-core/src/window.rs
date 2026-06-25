//! Bounded sliding-window PCM math for streaming dictation.
//!
//! A 5–10 minute dictation must stay cheap: each live decode should cost the same
//! whether the capture is 5 seconds or 5 minutes old. So the decode loop feeds
//! whisper-server only a **bounded window** of the growing capture — the last
//! `window_seconds`, never reaching back before the audio whose words are already
//! committed. This is the pure byte math; the file read + WAV slicing stay at the
//! I/O boundary (the RIFF `data`-chunk scan in the io crate).

/// Whisper's capture format: 16 kHz mono signed-16-bit PCM ⇒ 32000 bytes/second.
const SAMPLE_RATE_HZ: u64 = 16_000;
const BYTES_PER_SAMPLE: u64 = 2;
const BYTES_PER_SECOND: u64 = SAMPLE_RATE_HZ * BYTES_PER_SAMPLE;

/// PCM byte count for `seconds` of the 16 kHz mono s16 capture, aligned down to a
/// whole sample so a window never splits an s16 frame. A negative or non-finite
/// input clamps to zero.
pub fn seconds_to_bytes(seconds: f32) -> u64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    let bytes = (seconds as f64 * BYTES_PER_SECOND as f64) as u64;
    align_down(bytes)
}

/// Align `bytes` down to an s16 sample boundary (even byte) so a window slice
/// starts/ends on a whole sample.
fn align_down(bytes: u64) -> u64 {
    bytes - (bytes % BYTES_PER_SAMPLE)
}

/// The bounded PCM window to decode now, as a `(start, len)` byte range into the
/// capture's `data` chunk.
///
/// `total_data_len` is the bytes of PCM present, `committed_offset` the byte
/// position before which audio is already committed (never re-decoded), and
/// `window_len` the bound (e.g. `seconds_to_bytes(window_seconds)`). The window is
/// the **last `window_len` bytes**, but never starting before `committed_offset`:
///
/// - lots of uncommitted audio ⇒ `start = total - window_len` (the trailing
///   window; older committed audio has dropped out — cost stays bounded);
/// - little audio since the last commit ⇒ `start = committed_offset` (everything
///   not yet committed, a sub-window shorter than the bound).
///
/// `len` is therefore always `≤ window_len` and clamped to what is present. `start`
/// is aligned to a sample boundary.
pub fn window_range(total_data_len: u64, committed_offset: u64, window_len: u64) -> (u64, u64) {
    let floor = committed_offset.min(total_data_len);
    let trailing = total_data_len.saturating_sub(window_len);
    let start = align_down(floor.max(trailing));
    (start, total_data_len - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1 second = 32000 bytes; a 2 s window = 64000 bytes.
    const SEC: u64 = 32_000;

    #[test]
    fn seconds_to_bytes_uses_the_16k_mono_s16_rate() {
        assert_eq!(seconds_to_bytes(1.0), 32_000);
        assert_eq!(seconds_to_bytes(15.0), 480_000);
        assert_eq!(seconds_to_bytes(0.0), 0);
        assert_eq!(seconds_to_bytes(-3.0), 0);
    }

    #[test]
    fn a_capture_longer_than_the_window_decodes_only_the_trailing_window() {
        // 10 s of audio, 2 s window, nothing committed ⇒ decode the last 2 s.
        let (start, len) = window_range(10 * SEC, 0, 2 * SEC);
        assert_eq!(start, 8 * SEC);
        assert_eq!(len, 2 * SEC);
        assert!(len <= 2 * SEC, "per-decode length is bounded by the window");
    }

    #[test]
    fn a_capture_shorter_than_the_window_decodes_the_whole_file() {
        // Only 0.5 s present, 2 s window ⇒ the whole (sub-window) capture.
        let (start, len) = window_range(SEC / 2, 0, 2 * SEC);
        assert_eq!(start, 0);
        assert_eq!(len, SEC / 2);
    }

    #[test]
    fn committed_audio_drops_out_so_the_stable_prefix_is_never_re_decoded() {
        // 3 s present, the first 1 s already committed, 5 s window. The window must
        // start at the committed offset (1 s), not at 0 — committed audio excluded.
        let (start, len) = window_range(3 * SEC, SEC, 5 * SEC);
        assert_eq!(start, SEC, "window starts at the committed offset");
        assert_eq!(len, 2 * SEC, "only the uncommitted tail is decoded");
    }

    #[test]
    fn the_committed_offset_and_the_window_bound_compose() {
        // 200000 bytes present, committed at 10000, 2 s (64000) window. The trailing
        // bound (200000-64000=136000) is past the commit, so it wins ⇒ bounded.
        let (start, len) = window_range(200_000, 10_000, 64_000);
        assert_eq!(start, 136_000);
        assert_eq!(len, 64_000);
    }

    #[test]
    fn the_window_never_starts_before_the_committed_offset() {
        // Even a huge window cannot reach back past committed audio.
        let (start, len) = window_range(100_000, 40_000, 10 * SEC);
        assert_eq!(start, 40_000);
        assert_eq!(len, 60_000);
    }

    #[test]
    fn the_start_is_aligned_to_a_sample_boundary() {
        // An odd trailing start is aligned down so an s16 frame is never split.
        let (start, _len) = window_range(100_001, 0, 50_001);
        assert_eq!(start % 2, 0, "start must land on a whole s16 sample");
    }

    #[test]
    fn a_committed_offset_past_the_end_clamps_to_the_end() {
        // Defensive: a stale committed offset beyond what's present yields an empty
        // window rather than an underflow.
        let (start, len) = window_range(1000, 5000, 2 * SEC);
        assert_eq!(start, 1000);
        assert_eq!(len, 0);
    }
}
