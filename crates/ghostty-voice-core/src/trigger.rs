//! In-terminal trigger recognition for `talk-to`.
//!
//! `talk-to` is the **sole interface** (see `docs/adr/0003`): there is no
//! system-wide hotkey. Instead the wrapper watches the keystrokes flowing through
//! its PTY proxy and recognizes two trigger key combos by their terminal escape
//! sequences, **consuming** them (they are not forwarded to the wrapped agent)
//! and turning them into daemon commands sent over the control socket:
//!
//! - **Shift+F10** (`ESC [ 21 ; 2 ~`) → [`Trigger::Toggle`] — start/stop a batch
//!   recording.
//! - **Shift+F9** (`ESC [ 20 ; 2 ~`) → [`Trigger::Vad`] — start a hands-free VAD
//!   recording (auto-stops on silence).
//!
//! A terminal reports key *presses* only — there is no key-release or hold
//! timing — so these are discrete commands, not the tap/hold/PTT gestures the old
//! global evdev path could express. Everything else flows through verbatim.
//!
//! Pure and unit-tested: feed a byte buffer, get back the ordered split of
//! forward-bytes and recognized triggers. The one boundary assumption (shared
//! with the existing debug-key handling) is that a terminal delivers a single
//! function-key escape sequence in one read — a sequence split across two reads
//! is not recognized.

/// A recognized in-terminal trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Shift+F10 — toggle a batch recording on/off.
    Toggle,
    /// Shift+F9 — start a hands-free VAD recording.
    Vad,
}

impl Trigger {
    /// The control-socket command word this trigger sends to the daemon.
    pub fn command_word(self) -> &'static str {
        match self {
            Trigger::Toggle => "toggle",
            Trigger::Vad => "vad",
        }
    }
}

/// Shift+F10 in the common xterm modifier encoding (`;2` = Shift).
const SHIFT_F10: &[u8] = b"\x1b[21;2~";
/// Shift+F9 in the common xterm modifier encoding (`;2` = Shift).
const SHIFT_F9: &[u8] = b"\x1b[20;2~";

/// The trigger combos in match order, longest-first is not needed (both are the
/// same length and share no prefix that would mis-bind).
const COMBOS: &[(&[u8], Trigger)] = &[(SHIFT_F10, Trigger::Toggle), (SHIFT_F9, Trigger::Vad)];

/// One piece of split proxy input: bytes to forward to the wrapped agent's PTY,
/// or a recognized trigger to dispatch to the daemon (and *not* forward).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment<'a> {
    /// Bytes to write verbatim to the child PTY.
    Forward(&'a [u8]),
    /// A recognized trigger — consumed, never forwarded.
    Trigger(Trigger),
}

/// Scan `buf`, extracting trigger escape sequences (consumed) and forwarding
/// everything else verbatim, preserving order. A buffer with no trigger yields a
/// single `Forward(buf)`; an empty buffer yields nothing.
pub fn scan(buf: &[u8]) -> Vec<Segment<'_>> {
    let mut out: Vec<Segment> = Vec::new();
    let mut run_start = 0usize; // start of the current pending Forward run
    let mut i = 0usize;
    while i < buf.len() {
        if let Some((seq, trigger)) = COMBOS
            .iter()
            .find(|(seq, _)| buf[i..].starts_with(seq))
            .copied()
        {
            // Flush any forward bytes before this trigger, then the trigger.
            if run_start < i {
                out.push(Segment::Forward(&buf[run_start..i]));
            }
            out.push(Segment::Trigger(trigger));
            i += seq.len();
            run_start = i;
        } else {
            i += 1;
        }
    }
    if run_start < buf.len() {
        out.push(Segment::Forward(&buf[run_start..]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_f10_alone_is_a_toggle_trigger_and_is_consumed() {
        assert_eq!(scan(b"\x1b[21;2~"), vec![Segment::Trigger(Trigger::Toggle)]);
    }

    #[test]
    fn shift_f9_alone_is_a_vad_trigger_and_is_consumed() {
        assert_eq!(scan(b"\x1b[20;2~"), vec![Segment::Trigger(Trigger::Vad)]);
    }

    #[test]
    fn the_triggers_map_to_their_wire_command_words() {
        assert_eq!(Trigger::Toggle.command_word(), "toggle");
        assert_eq!(Trigger::Vad.command_word(), "vad");
    }

    #[test]
    fn ordinary_text_passes_through_untouched() {
        assert_eq!(scan(b"ls -la\n"), vec![Segment::Forward(b"ls -la\n")]);
    }

    #[test]
    fn an_empty_buffer_yields_nothing() {
        assert_eq!(scan(b""), Vec::<Segment>::new());
    }

    #[test]
    fn bare_f9_and_f10_pass_through_untouched() {
        // Unshifted function keys (`ESC [ 20 ~` / `ESC [ 21 ~`) are NOT triggers;
        // they must reach the wrapped agent verbatim.
        assert_eq!(scan(b"\x1b[20~"), vec![Segment::Forward(b"\x1b[20~")]);
        assert_eq!(scan(b"\x1b[21~"), vec![Segment::Forward(b"\x1b[21~")]);
    }

    #[test]
    fn the_f12_debug_key_passes_through_untouched() {
        // F12 (`ESC [ 24 ~`) is talk-to's separate debug aid — not a trigger here.
        assert_eq!(scan(b"\x1b[24~"), vec![Segment::Forward(b"\x1b[24~")]);
    }

    #[test]
    fn a_trigger_embedded_in_text_splits_around_it() {
        // A trigger that lands in the same read as ordinary bytes is extracted,
        // and the surrounding bytes still forward in order.
        assert_eq!(
            scan(b"ab\x1b[21;2~cd"),
            vec![
                Segment::Forward(b"ab"),
                Segment::Trigger(Trigger::Toggle),
                Segment::Forward(b"cd"),
            ],
        );
    }

    #[test]
    fn two_triggers_back_to_back_both_resolve() {
        assert_eq!(
            scan(b"\x1b[21;2~\x1b[20;2~"),
            vec![
                Segment::Trigger(Trigger::Toggle),
                Segment::Trigger(Trigger::Vad),
            ],
        );
    }

    #[test]
    fn a_lone_escape_is_forwarded_not_swallowed() {
        // A bare ESC (e.g. the user pressing Escape) must not be eaten while
        // looking for a trigger sequence.
        assert_eq!(scan(b"\x1b"), vec![Segment::Forward(b"\x1b")]);
    }
}
