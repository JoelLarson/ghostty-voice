//! VAD recording arguments (S5).
//!
//! Builds the `sox` `silence` effect that auto-stops on trailing silence. The
//! config-driven part (threshold, silence duration) is here and tested; the
//! exact `sox` recording flags still need validation on hardware (sox is a
//! dependency not yet present on the dev box).

/// The `sox` `silence` effect: trim leading silence, then stop after
/// `silence_seconds` below `threshold_pct`% — e.g. `silence 1 0.1 3% 1 2.0 3%`.
pub fn silence_effect(silence_seconds: f32, threshold_pct: u32) -> Vec<String> {
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
pub fn record_args(out: &str, silence_seconds: f32, threshold_pct: u32) -> Vec<String> {
    let mut argv = vec![
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
    ];
    argv.extend(silence_effect(silence_seconds, threshold_pct));
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_effect_uses_threshold_and_duration() {
        assert_eq!(
            silence_effect(2.0, 3),
            vec!["silence", "1", "0.1", "3%", "1", "2", "3%"],
        );
    }

    #[test]
    fn threshold_is_rendered_as_a_percentage() {
        let effect = silence_effect(1.5, 5);
        assert!(effect.contains(&"5%".to_owned()));
        assert!(effect.contains(&"1.5".to_owned()));
    }

    #[test]
    fn record_args_target_the_output_and_end_with_silence() {
        let argv = record_args("/tmp/x.wav", 2.0, 3);
        assert!(argv.contains(&"/tmp/x.wav".to_owned()));
        assert_eq!(argv[argv.len() - 7], "silence");
    }
}
