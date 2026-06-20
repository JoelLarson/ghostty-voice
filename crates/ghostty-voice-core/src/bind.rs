//! Bind-flow conflict evaluation (S8).
//!
//! `ghostty-voice-ctl bind` captures a key, shows what it emitted, and warns
//! before writing it to config. There is no unified binding registry on Linux,
//! so warnings are best-effort heuristics (the live "press once" test is the
//! ground-truth backstop). Capturing the key happens at the IO boundary; turning
//! the captured facts into named warnings is pure and tested here, mirroring
//! [`crate::doctor::evaluate`]. The `gsettings_bound` input models a known GNOME
//! conflict for completeness, but the bind flow no longer queries GNOME — evdev
//! sits beneath the compositor, so the live test is what actually confirms a key
//! is free.

use crate::key_combo::{KeyCombo, is_primary_key, key_name};

/// Facts gathered at the boundary about a captured combo.
#[derive(Debug, Clone)]
pub struct BindProbes {
    /// The combo the user pressed (already parsed from the captured event).
    pub combo: KeyCombo,
    /// The combo's accelerator is already a GNOME custom keybinding (from a
    /// `gsettings` query) — pressing it would fire that too.
    pub gsettings_bound: bool,
    /// The captured keycode differed from the key's label — the device or its
    /// vendor software has remapped it, so what you bind isn't what's printed.
    pub remapped: bool,
}

/// One actionable warning about a chosen combo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// A stable, machine-checkable kind (the test target).
    pub kind: WarningKind,
    /// A human-facing explanation.
    pub message: String,
}

/// The categories of bind-time warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
    /// The key is a normal typing key — without an exclusive grab it will both
    /// trigger and type a character.
    PrimaryKey,
    /// The accelerator is already bound in GNOME gsettings.
    AlreadyBound,
    /// The captured code didn't match the key's name — it's been remapped.
    Remapped,
}

/// Turn probe facts into the list of warnings (empty ⇒ the combo looks safe).
pub fn evaluate(probes: &BindProbes) -> Vec<Warning> {
    let mut warnings = Vec::new();

    if is_primary_key(probes.combo.key) {
        let name = key_name(probes.combo.key).unwrap_or("that key");
        warnings.push(Warning {
            kind: WarningKind::PrimaryKey,
            message: format!(
                "{name} is a primary typing key — it can't be grabbed exclusively, so it will \
                 both trigger recording AND type a character. Pick a spare key (an F-key works well)."
            ),
        });
    }

    if probes.gsettings_bound {
        warnings.push(Warning {
            kind: WarningKind::AlreadyBound,
            message: format!(
                "{} is already a GNOME custom keybinding — pressing it would fire that too. \
                 Remove the gsettings binding or choose another key.",
                probes.combo.display()
            ),
        });
    }

    if probes.remapped {
        warnings.push(Warning {
            kind: WarningKind::Remapped,
            message:
                "the key emitted a different code than its printed label — your keyboard/mouse \
                 software has remapped it. Bind what it actually emits, or undo the remap."
                    .to_owned(),
        });
    }

    warnings
}

/// True when a combo drew no warnings.
pub fn looks_safe(warnings: &[Warning]) -> bool {
    warnings.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_combo::codes;

    fn probes(combo: &str) -> BindProbes {
        BindProbes {
            combo: KeyCombo::parse(combo).unwrap(),
            gsettings_bound: false,
            remapped: false,
        }
    }

    #[test]
    fn a_spare_function_key_combo_is_clean() {
        let warnings = evaluate(&probes("Shift+F10"));
        assert!(looks_safe(&warnings), "got {warnings:?}");
    }

    #[test]
    fn a_primary_key_is_flagged() {
        let warnings = evaluate(&probes("Shift+D"));
        assert!(warnings.iter().any(|w| w.kind == WarningKind::PrimaryKey));
    }

    #[test]
    fn an_already_bound_accelerator_is_flagged() {
        let p = BindProbes {
            gsettings_bound: true,
            ..probes("Shift+F10")
        };
        let warnings = evaluate(&p);
        assert!(warnings.iter().any(|w| w.kind == WarningKind::AlreadyBound));
        // The message names the combo so the user knows which to unbind.
        let bound = warnings
            .iter()
            .find(|w| w.kind == WarningKind::AlreadyBound)
            .unwrap();
        assert!(bound.message.contains("Shift+F10"));
    }

    #[test]
    fn a_remapped_key_is_flagged() {
        let p = BindProbes {
            remapped: true,
            ..probes("Shift+F10")
        };
        let warnings = evaluate(&p);
        assert!(warnings.iter().any(|w| w.kind == WarningKind::Remapped));
    }

    #[test]
    fn multiple_problems_accumulate() {
        // A primary key that is also already bound yields both warnings.
        let p = BindProbes {
            gsettings_bound: true,
            ..probes("Shift+A")
        };
        let warnings = evaluate(&p);
        assert!(!looks_safe(&warnings));
        assert!(warnings.iter().any(|w| w.kind == WarningKind::PrimaryKey));
        assert!(warnings.iter().any(|w| w.kind == WarningKind::AlreadyBound));
    }

    #[test]
    fn the_default_start_key_is_a_spare() {
        // Sanity-check the shipped default steers clear of every warning.
        let p = BindProbes {
            combo: KeyCombo {
                modifiers: crate::key_combo::Modifiers {
                    shift: true,
                    ctrl: false,
                    alt: false,
                },
                key: codes::KEY_F10,
            },
            gsettings_bound: false,
            remapped: false,
        };
        assert!(looks_safe(&evaluate(&p)));
    }
}
