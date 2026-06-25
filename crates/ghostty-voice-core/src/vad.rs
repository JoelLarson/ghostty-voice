//! VAD recording arguments.
//!
//! Builds the `sox` `silence` effect that auto-stops on trailing silence. The
//! config-driven part (threshold, silence duration) is here and tested; the
//! exact `sox` recording flags still need validation on hardware (sox is a
//! dependency not yet present on the dev box).

/// The `sox` `silence` effect: trim leading silence, then stop after
/// `silence_seconds` below `threshold_pct`% — e.g. `silence 1 0.1 3% 1 2.0 3%`.
/// `threshold_pct` may be fractional (e.g. `0.3`) for a quiet mic.
pub fn silence_effect(silence_seconds: f32, threshold_pct: f32) -> Vec<String> {
    let threshold = format!("{threshold_pct}%");
    vec![
        "silence".to_owned(),
        "1".to_owned(),
        "0.1".to_owned(),
        threshold.clone(),
        "1".to_owned(),
        format!("{silence_seconds}"),
        threshold,
    ]
}

/// Full `sox` argv to record a 16 kHz mono s16 WAV that auto-stops on silence.
pub fn record_args(out: &str, silence_seconds: f32, threshold_pct: f32) -> Vec<String> {
    let mut argv = record_prefix(out);
    argv.extend(silence_effect(silence_seconds, threshold_pct));
    argv
}

/// The continuous-mode split effect: cut the current clip after
/// `clip_pause_seconds` below `threshold_pct`%, then `: newfile : restart` so
/// `sox` opens the next numbered clip and keeps recording the same session —
/// one long capture sprayed into silence-bounded clips. The daemon watches the
/// session dir, transcribes each finalized clip, and ends the session itself on
/// the long session-end silence (sox's own per-clip trim is just the cut point).
pub fn continuous_split_effect(clip_pause_seconds: f32, threshold_pct: f32) -> Vec<String> {
    let mut effect = silence_effect(clip_pause_seconds, threshold_pct);
    effect.extend([
        ":".to_owned(),
        "newfile".to_owned(),
        ":".to_owned(),
        "restart".to_owned(),
    ]);
    effect
}

/// Full `sox` argv for a continuous-mode session: record a 16 kHz mono s16 WAV
/// into `out_template` (with `%n` expanded by sox to the clip index, e.g.
/// `clip-%n.wav` → `clip-1.wav`, `clip-2.wav`, …), splitting on each clip-cut
/// pause via [`continuous_split_effect`].
pub fn continuous_record_args(
    out_template: &str,
    clip_pause_seconds: f32,
    threshold_pct: f32,
) -> Vec<String> {
    let mut argv = record_prefix(out_template);
    argv.extend(continuous_split_effect(clip_pause_seconds, threshold_pct));
    argv
}

/// The shared `sox` recording prefix: quiet, default device, WAV contract
/// (16 kHz mono s16), and the output path.
fn record_prefix(out: &str) -> Vec<String> {
    vec![
        "-q".to_owned(),
        "-d".to_owned(),
        "-r".to_owned(),
        "16000".to_owned(),
        "-c".to_owned(),
        "1".to_owned(),
        "-b".to_owned(),
        "16".to_owned(),
        "-e".to_owned(),
        "signed-integer".to_owned(),
        out.to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_effect_uses_threshold_and_duration() {
        assert_eq!(
            silence_effect(2.0, 3.0),
            vec!["silence", "1", "0.1", "3%", "1", "2", "3%"],
        );
    }

    #[test]
    fn threshold_is_rendered_as_a_percentage() {
        let effect = silence_effect(1.5, 5.0);
        assert!(effect.contains(&"5%".to_owned()));
        assert!(effect.contains(&"1.5".to_owned()));
    }

    #[test]
    fn a_fractional_threshold_renders_as_a_decimal_percent() {
        // A quiet mic whose speech peaks below 1% needs a sub-1% threshold; sox
        // accepts a decimal percent, so `0.3` must render as `0.3%`.
        let effect = silence_effect(2.0, 0.3);
        assert!(effect.contains(&"0.3%".to_owned()), "got {effect:?}");
    }

    #[test]
    fn record_args_target_the_output_and_end_with_silence() {
        let argv = record_args("/tmp/x.wav", 2.0, 3.0);
        assert!(argv.contains(&"/tmp/x.wav".to_owned()));
        assert_eq!(argv[argv.len() - 7], "silence");
    }

    // ---- continuous-mode multi-clip split --------------------------

    #[test]
    fn split_effect_cuts_on_clip_pause_and_restarts() {
        // Continuous mode: stop the current clip after `clip_pause` of silence,
        // then `: newfile : restart` so sox opens the next numbered clip and
        // keeps recording the same session.
        let effect = continuous_split_effect(1.0, 3.0);
        assert_eq!(
            effect,
            vec![
                "silence", "1", "0.1", "3%", "1", "1", "3%", ":", "newfile", ":", "restart",
            ],
        );
    }

    #[test]
    fn split_args_write_numbered_clips_via_a_template_path() {
        // sox expands `%n` in the output path to the clip index, so a session
        // dir gets clip-1.wav, clip-2.wav, ... The argv records from the default
        // device in the WAV contract and ends with the split effect.
        let argv = continuous_record_args("/tmp/sess/clip-%n.wav", 1.0, 3.0);
        assert!(argv.contains(&"/tmp/sess/clip-%n.wav".to_owned()));
        assert_eq!(argv[argv.len() - 11], "silence");
        assert_eq!(argv[argv.len() - 3], "newfile");
        assert_eq!(argv.last().unwrap(), "restart");
    }
}
