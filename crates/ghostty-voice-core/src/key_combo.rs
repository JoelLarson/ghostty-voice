//! Key-combo parsing and matching (evdev input layer, S8).
//!
//! A combo such as `Shift+F10` is a main key plus a set of required modifiers.
//! Parsing turns the human string into evdev keycodes (the same numbers as
//! Linux's `input-event-codes.h`, so the IO boundary feeds raw codes straight
//! in); matching asks whether a pressed keycode under the current modifier state
//! is this combo. Pure: the boundary owns the device, this owns the rules.

use std::fmt;

/// Linux evdev keycodes we name (subset of `input-event-codes.h`). The values
/// are the kernel's, so the IO layer passes `evdev` codes through unchanged.
pub mod codes {
    pub const KEY_LEFTSHIFT: u16 = 42;
    pub const KEY_RIGHTSHIFT: u16 = 54;
    pub const KEY_LEFTCTRL: u16 = 29;
    pub const KEY_RIGHTCTRL: u16 = 97;
    pub const KEY_LEFTALT: u16 = 56;
    pub const KEY_RIGHTALT: u16 = 100;

    pub const KEY_F9: u16 = 67;
    pub const KEY_F10: u16 = 68;
}

/// Which modifiers a combo requires (or, derived from held keys, which are down).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };
}

/// A parsed key combo: a main key plus the exact modifier set it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyCombo {
    pub modifiers: Modifiers,
    pub key: u16,
}

/// Why a combo string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComboError {
    /// The string was empty (no key).
    Empty,
    /// A token names neither a modifier nor a known key.
    UnknownKey(String),
    /// More than one non-modifier key was given (e.g. `F9+F10`).
    MultipleKeys,
}

impl fmt::Display for ComboError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComboError::Empty => write!(f, "empty key combo"),
            ComboError::UnknownKey(k) => write!(f, "unknown key: {k}"),
            ComboError::MultipleKeys => write!(f, "a combo may have only one non-modifier key"),
        }
    }
}

impl KeyCombo {
    /// Parse a combo like `Shift+F10` (case-insensitive, `+`-separated). The
    /// last non-modifier token is the key; `shift`/`ctrl`/`control`/`alt` are
    /// modifiers. Whitespace around tokens is ignored.
    pub fn parse(s: &str) -> Result<KeyCombo, ComboError> {
        let mut modifiers = Modifiers::NONE;
        let mut key: Option<u16> = None;
        for raw in s.split('+') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            match modifier_of(token) {
                Some(m) => match m {
                    Modifier::Shift => modifiers.shift = true,
                    Modifier::Ctrl => modifiers.ctrl = true,
                    Modifier::Alt => modifiers.alt = true,
                },
                None => {
                    let code =
                        key_code(token).ok_or_else(|| ComboError::UnknownKey(token.to_owned()))?;
                    if key.is_some() {
                        return Err(ComboError::MultipleKeys);
                    }
                    key = Some(code);
                }
            }
        }
        match key {
            Some(key) => Ok(KeyCombo { modifiers, key }),
            None => Err(ComboError::Empty),
        }
    }

    /// Does a press of `code` under `held` modifiers ring this combo? The
    /// modifier set must match exactly, so `Shift+F10` does not fire when
    /// `Ctrl+Shift+F10` is pressed (avoids cross-combo collisions).
    pub fn matches(&self, code: u16, held: Modifiers) -> bool {
        self.key == code && self.modifiers == held
    }

    /// Render back to canonical `Mod+Mod+Key` form (modifiers in a fixed order).
    pub fn display(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Ctrl");
        }
        if self.modifiers.alt {
            parts.push("Alt");
        }
        if self.modifiers.shift {
            parts.push("Shift");
        }
        let key = key_name(self.key).unwrap_or("Unknown");
        parts.push(key);
        parts.join("+")
    }
}

#[derive(Debug, Clone, Copy)]
enum Modifier {
    Shift,
    Ctrl,
    Alt,
}

/// The modifier a token names, if any (case-insensitive).
fn modifier_of(token: &str) -> Option<Modifier> {
    match token.to_ascii_lowercase().as_str() {
        "shift" => Some(Modifier::Shift),
        "ctrl" | "control" => Some(Modifier::Ctrl),
        "alt" => Some(Modifier::Alt),
        _ => None,
    }
}

/// The named-key table: human name ⇄ evdev keycode. Letters/digits use QWERTY
/// scancode order (the kernel's), not alphabetical.
const KEYS: &[(&str, u16)] = &[
    // Function keys.
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("F11", 87),
    ("F12", 88),
    // Digits.
    ("1", 2),
    ("2", 3),
    ("3", 4),
    ("4", 5),
    ("5", 6),
    ("6", 7),
    ("7", 8),
    ("8", 9),
    ("9", 10),
    ("0", 11),
    // Letters (scancode order).
    ("Q", 16),
    ("W", 17),
    ("E", 18),
    ("R", 19),
    ("T", 20),
    ("Y", 21),
    ("U", 22),
    ("I", 23),
    ("O", 24),
    ("P", 25),
    ("A", 30),
    ("S", 31),
    ("D", 32),
    ("F", 33),
    ("G", 34),
    ("H", 35),
    ("J", 36),
    ("K", 37),
    ("L", 38),
    ("Z", 44),
    ("X", 45),
    ("C", 46),
    ("V", 47),
    ("B", 48),
    ("N", 49),
    ("M", 50),
    // Whitespace / editing — recognized so they can be named in warnings.
    ("Backspace", 14),
    ("Tab", 15),
    ("Enter", 28),
    ("Space", 57),
    ("Esc", 1),
    ("Insert", 110),
    ("Delete", 111),
    ("Home", 102),
    ("End", 107),
    ("PageUp", 104),
    ("PageDown", 109),
    ("Up", 103),
    ("Down", 108),
    ("Left", 105),
    ("Right", 106),
    ("PrintScreen", 99),
    ("ScrollLock", 70),
    ("Pause", 119),
];

/// The evdev keycode for a named key (case-insensitive), or `None` if unknown.
pub fn key_code(name: &str) -> Option<u16> {
    let n = name.trim();
    KEYS.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(n))
        .map(|(_, c)| *c)
}

/// The canonical name for a keycode, or `None` if we don't name it.
pub fn key_name(code: u16) -> Option<&'static str> {
    KEYS.iter().find(|(_, c)| *c == code).map(|(k, _)| *k)
}

/// True if `code` is a *primary* typing key — a letter, digit, or whitespace/
/// editing key. Binding a primary key is a footgun (no `EVIOCGRAB`, so it would
/// double-fire as both a trigger and a typed character); the bind flow warns.
pub fn is_primary_key(code: u16) -> bool {
    // Letters, digits.
    let letters_digits = (2..=11).contains(&code) // digits 1..0
        || (16..=25).contains(&code) // qwertyuiop
        || (30..=38).contains(&code) // asdfghjkl
        || (44..=50).contains(&code); // zxcvbnm
    // Space, enter, tab, backspace.
    let whitespace = matches!(code, 14 | 15 | 28 | 57);
    letters_digits || whitespace
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_modifier_plus_function_key() {
        let combo = KeyCombo::parse("Shift+F10").unwrap();
        assert_eq!(combo.key, codes::KEY_F10);
        assert_eq!(
            combo.modifiers,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        );
    }

    #[test]
    fn parsing_is_case_insensitive_and_trims() {
        assert_eq!(
            KeyCombo::parse("  shift + f9 ").unwrap(),
            KeyCombo {
                modifiers: Modifiers {
                    shift: true,
                    ctrl: false,
                    alt: false
                },
                key: codes::KEY_F9,
            }
        );
    }

    #[test]
    fn parses_a_bare_key_with_no_modifiers() {
        let combo = KeyCombo::parse("F10").unwrap();
        assert_eq!(combo.modifiers, Modifiers::NONE);
        assert_eq!(combo.key, codes::KEY_F10);
    }

    #[test]
    fn parses_multiple_modifiers() {
        let combo = KeyCombo::parse("Ctrl+Alt+D").unwrap();
        assert_eq!(
            combo.modifiers,
            Modifiers {
                shift: false,
                ctrl: true,
                alt: true
            }
        );
        assert_eq!(combo.key, key_code("D").unwrap());
    }

    #[test]
    fn empty_string_is_an_error() {
        assert_eq!(KeyCombo::parse(""), Err(ComboError::Empty));
        assert_eq!(KeyCombo::parse("Shift+"), Err(ComboError::Empty));
    }

    #[test]
    fn unknown_key_is_an_error() {
        assert_eq!(
            KeyCombo::parse("Shift+Frobnicate"),
            Err(ComboError::UnknownKey("Frobnicate".to_owned()))
        );
    }

    #[test]
    fn two_non_modifier_keys_is_an_error() {
        assert_eq!(KeyCombo::parse("F9+F10"), Err(ComboError::MultipleKeys));
    }

    #[test]
    fn matches_exact_modifier_state() {
        let combo = KeyCombo::parse("Shift+F10").unwrap();
        assert!(combo.matches(
            codes::KEY_F10,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        ));
    }

    #[test]
    fn does_not_match_when_a_modifier_is_missing() {
        let combo = KeyCombo::parse("Shift+F10").unwrap();
        assert!(!combo.matches(codes::KEY_F10, Modifiers::NONE));
    }

    #[test]
    fn does_not_match_with_extra_modifiers_held() {
        // Shift+F10 must NOT fire under Ctrl+Shift+F10 — exact match avoids
        // a more-specific combo accidentally triggering a looser one.
        let combo = KeyCombo::parse("Shift+F10").unwrap();
        assert!(!combo.matches(
            codes::KEY_F10,
            Modifiers {
                shift: true,
                ctrl: true,
                alt: false
            }
        ));
    }

    #[test]
    fn does_not_match_a_different_key() {
        let combo = KeyCombo::parse("Shift+F10").unwrap();
        assert!(!combo.matches(
            codes::KEY_F9,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        ));
    }

    #[test]
    fn round_trips_through_display() {
        for s in ["Shift+F10", "Ctrl+Alt+Shift+D", "F9"] {
            let combo = KeyCombo::parse(s).unwrap();
            // display() is canonical (Ctrl, Alt, Shift order); re-parsing it
            // yields the same combo.
            assert_eq!(KeyCombo::parse(&combo.display()).unwrap(), combo);
        }
    }

    #[test]
    fn names_a_keycode_for_the_bind_flow() {
        assert_eq!(key_name(codes::KEY_F10), Some("F10"));
        assert_eq!(key_name(codes::KEY_LEFTSHIFT), None); // modifiers aren't named keys
    }

    #[test]
    fn flags_primary_typing_keys() {
        assert!(is_primary_key(key_code("A").unwrap()));
        assert!(is_primary_key(key_code("5").unwrap()));
        assert!(is_primary_key(key_code("Space").unwrap()));
        assert!(is_primary_key(key_code("Enter").unwrap()));
    }

    #[test]
    fn function_keys_are_not_primary() {
        // F-keys are the spare keys the bind flow steers toward.
        assert!(!is_primary_key(codes::KEY_F9));
        assert!(!is_primary_key(codes::KEY_F10));
    }
}
