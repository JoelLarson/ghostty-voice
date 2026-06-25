//! Bottom status-strip geometry + renderer for the `talk-to` wrapper (IDEAS.md #4).
//!
//! The strip is a *bottom* strip, and that is load-bearing, not cosmetic. A
//! foreground proxy can forward the child's bytes verbatim only while the
//! child's coordinate space stays corner-aligned with the real screen: same
//! origin (1,1), same width, height ≤ available. Reserving the **bottom**
//! row(s) and giving the child a winsize of `(rows - strip_height, cols)`
//! preserves all three — the child cannot address the row we own, so we forward
//! untouched and paint the strip ourselves.
//!
//! A *top* strip moves the origin and a *side* pane changes the width; either
//! desyncs the child's absolute escape codes from the screen, and the only fix
//! is a full VT emulator. We are not building a multiplexer, so this module only
//! ever reserves from the bottom.

/// A terminal / PTY size in rows and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Winsize {
    pub rows: u16,
    pub cols: u16,
}

/// The result of reserving a bottom strip from a terminal of a given size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripGeometry {
    /// Winsize handed to the child: full width, height reduced by the strip.
    pub child: Winsize,
    /// 1-based row of the top of the reserved strip — the first row *we* own and
    /// the child never addresses.
    pub strip_row: u16,
}

/// Reserve `strip_height` rows at the **bottom** of a `(rows, cols)` terminal.
///
/// The invariant: the child keeps origin (1,1) and full width `cols`, with its
/// height reduced to `rows - strip_height`. Origin and width unchanged ⇒ the
/// child's bytes forward verbatim with no VT emulator.
///
/// Degenerate sizes never panic. If the terminal is too short to host both the
/// child and the strip (`rows <= strip_height`), the child gets 0 usable rows
/// and the strip is pinned to row 1 — the caller still runs the child, it just
/// has nowhere to draw until the terminal grows.
pub fn geometry(rows: u16, cols: u16, strip_height: u16) -> StripGeometry {
    let child_rows = rows.saturating_sub(strip_height);
    // The strip starts on the first row past the child (1-based); clamp to ≥ 1
    // so a 0-row terminal still yields a valid cursor address.
    let strip_row = child_rows.saturating_add(1).max(1);
    StripGeometry {
        child: Winsize {
            rows: child_rows,
            cols,
        },
        strip_row,
    }
}

// ANSI control kept readable. Save/restore use DECSC/DECRC so the child's cursor
// position is preserved exactly across a strip repaint.
const SAVE_CURSOR: &[u8] = b"\x1b7"; // DECSC
const RESTORE_CURSOR: &[u8] = b"\x1b8"; // DECRC

/// Paint the status strip at `strip_row`, showing `state` (e.g. `idle`,
/// `recording`, `transcribing`).
///
/// Saves the cursor, moves to `(strip_row, 1)`, clears that line, writes
/// `● <state>`, and restores the cursor — so the child's region (every row above
/// `strip_row`) and the child's cursor are never disturbed. Pure: the bytes are
/// returned for the caller to write to the real terminal.
///
/// Per the PRD the renderer is verified visually in v1; the one smoke test below
/// just guards the structural contract (right row, restores cursor, carries the
/// token).
pub fn render(strip_row: u16, state: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(SAVE_CURSOR);
    // Move to the strip row, column 1; clear the whole line (CSI 2K).
    out.extend_from_slice(format!("\x1b[{strip_row};1H\x1b[2K").as_bytes());
    out.extend_from_slice(format!("\u{25cf} {state}").as_bytes()); // "● state"
    out.extend_from_slice(RESTORE_CURSOR);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_terminal_reserves_one_bottom_row() {
        let g = geometry(24, 80, 1);
        // child loses exactly the reserved row; width is untouched.
        assert_eq!(g.child, Winsize { rows: 23, cols: 80 });
        // strip is the last (24th) row.
        assert_eq!(g.strip_row, 24);
    }

    #[test]
    fn width_is_never_changed_by_reserving_a_bottom_strip() {
        // The invariant that lets bytes forward verbatim: cols out == cols in.
        for cols in [1u16, 80, 132, 400] {
            assert_eq!(geometry(40, cols, 1).child.cols, cols);
        }
    }

    #[test]
    fn child_origin_stays_at_the_top_so_the_strip_is_below_it() {
        // The child occupies rows 1..=child.rows; the strip is strictly below.
        let g = geometry(50, 120, 1);
        assert_eq!(g.child.rows, 49);
        assert_eq!(g.strip_row, 50);
        assert!(g.strip_row > g.child.rows, "strip must sit below the child");
    }

    #[test]
    fn a_multi_row_strip_reserves_that_many_rows() {
        let g = geometry(24, 80, 3);
        assert_eq!(g.child, Winsize { rows: 21, cols: 80 });
        assert_eq!(g.strip_row, 22); // first reserved row
    }

    #[test]
    fn a_one_row_terminal_gives_the_child_zero_rows_without_panicking() {
        let g = geometry(1, 80, 1);
        assert_eq!(g.child, Winsize { rows: 0, cols: 80 });
        assert_eq!(g.strip_row, 1);
    }

    #[test]
    fn a_terminal_shorter_than_the_strip_never_underflows() {
        let g = geometry(2, 80, 5);
        assert_eq!(g.child.rows, 0); // saturating, not wrapped to 65531
        assert_eq!(g.strip_row, 1);
    }

    #[test]
    fn a_zero_sized_terminal_is_handled() {
        let g = geometry(0, 0, 1);
        assert_eq!(g.child, Winsize { rows: 0, cols: 0 });
        assert_eq!(g.strip_row, 1);
    }

    #[test]
    fn render_targets_the_strip_row_carries_the_state_and_restores_the_cursor() {
        let bytes = render(24, "recording");
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.starts_with("\x1b7"), "must save the cursor first");
        assert!(
            s.contains("\x1b[24;1H"),
            "must address the strip row, col 1"
        );
        assert!(s.contains("recording"), "must paint the state token");
        assert!(s.ends_with("\x1b8"), "must restore the cursor last");
    }

    #[test]
    fn render_shows_the_streaming_state_so_a_live_dictation_is_visible() {
        // While a streaming dictation is active (keystrokes suppressed), the strip
        // must surface it so the user can see input is going to the live preview.
        let s = String::from_utf8(render(24, "streaming")).unwrap();
        assert!(
            s.contains("streaming"),
            "must paint the streaming state token"
        );
    }
}
