//! evdev input boundary.
//!
//! Reads key events directly from one `/dev/input` device — beneath the
//! compositor, so triggers work on any Wayland compositor (or X), not just
//! GNOME. This is the only true boundary of the tactile layer: it opens the one
//! configured device, converts raw `evdev` events into
//! [`RawKeyEvent`](ghostty_voice_core::input::RawKeyEvent)s, and hands them to
//! the pure [`KeyTracker`](ghostty_voice_core::input::KeyTracker), which owns all
//! the timing/modifier logic.
//!
//! **Security**: only the single selected device is ever opened, and the daemon
//! reacts only to the two configured keycodes — no other input is read or logged.
//! The device is never grabbed (`EVIOCGRAB`), so normal typing/clicking is
//! unaffected (the bind flow's job is to steer toward a spare key).

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use evdev::{Device, EventType, InputEvent, Key};
use ghostty_voice_core::input::RawKeyEvent;

/// Open the one input device named by `selector`:
///
/// - `auto` — the lowest-numbered **real hardware** keyboard (virtual/injector
///   devices like `ydotoold`'s are skipped — otherwise the daemon would read its
///   own injected keystrokes instead of yours).
/// - `/dev/input/...` — that exact device path.
/// - `name:<substr>` — the first device whose name contains `<substr>`
///   (case-insensitive).
///
/// With more than one physical keyboard, prefer pinning `name:<substr>` (e.g.
/// `name:daskeyboard`) — `auto` is a best-effort default, not a guess at which
/// keyboard you actually type on.
pub fn open_device(selector: &str) -> Result<(PathBuf, Device)> {
    let sel = selector.trim();
    if let Some(name) = sel.strip_prefix("name:") {
        let needle = name.trim().to_ascii_lowercase();
        // A name substring can match several collections of one device (e.g.
        // "daskeyboard" + "daskeyboard System Control"); prefer the one that is
        // an actual keyboard (emits the alpha/F-keys), lowest event number.
        let mut matches: Vec<(PathBuf, Device)> = evdev::enumerate()
            .filter(|(_, d)| {
                d.name()
                    .map(|n| n.to_ascii_lowercase().contains(&needle))
                    .unwrap_or(false)
            })
            .collect();
        matches.sort_by_key(|(p, _)| event_number(p));
        if let Some(kbd) = matches
            .iter()
            .position(|(_, d)| is_keyboard(d) && !is_virtual(d))
        {
            return Ok(matches.swap_remove(kbd));
        }
        // No keyboard-shaped match — fall back to the first name match so an
        // explicit non-keyboard selection still works.
        if let Some(pick) = matches.into_iter().next() {
            return Ok(pick);
        }
        bail!("no input device whose name contains {name:?}");
    }
    if sel == "auto" {
        // Collect real keyboards (skip our own injector and other virtual
        // devices), then pick the lowest event number for a stable choice.
        let mut candidates: Vec<(PathBuf, Device)> = evdev::enumerate()
            .filter(|(_, d)| is_keyboard(d) && !is_virtual(d))
            .collect();
        candidates.sort_by_key(|(p, _)| event_number(p));
        if let Some(pick) = candidates.into_iter().next() {
            return Ok(pick);
        }
        bail!("no real keyboard found under /dev/input (only virtual/injector devices?)");
    }
    // Otherwise treat it as a device path.
    let device = Device::open(sel).with_context(|| format!("opening input device {sel}"))?;
    Ok((PathBuf::from(sel), device))
}

/// A keyboard-like device supports the alpha keys (filters out mice, lid
/// switches, power buttons, and other non-keyboard input nodes).
fn is_keyboard(device: &Device) -> bool {
    device
        .supported_keys()
        .map(|keys| keys.contains(Key::KEY_A) && keys.contains(Key::KEY_ENTER))
        .unwrap_or(false)
}

/// A virtual/injector keyboard (e.g. `ydotoold`'s uinput device) — never a valid
/// trigger source: reading it would feed the daemon its own injected keystrokes,
/// not the user's. Matched by name since uinput devices carry a descriptive one.
fn is_virtual(device: &Device) -> bool {
    device
        .name()
        .map(|n| {
            let n = n.to_ascii_lowercase();
            n.contains("ydotool") || n.contains("virtual") || n.contains("uinput")
        })
        .unwrap_or(false)
}

/// The trailing `eventN` number of a `/dev/input/eventN` path (for a stable,
/// numeric `auto` ordering); `u32::MAX` if it can't be parsed.
fn event_number(path: &std::path::Path) -> u32 {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("event"))
        .and_then(|n| n.parse().ok())
        .unwrap_or(u32::MAX)
}

/// The device's human name (for logging which one device we opened).
pub fn device_name(device: &Device) -> String {
    device.name().unwrap_or("<unnamed>").to_owned()
}

/// Convert a raw evdev event into a [`RawKeyEvent`], or `None` for anything that
/// is not a key press/release (autorepeat `value == 2` is dropped — the tracker
/// only cares about the down and up edges).
fn to_raw(ev: &InputEvent) -> Option<RawKeyEvent> {
    if ev.event_type() != EventType::KEY {
        return None;
    }
    let pressed = match ev.value() {
        1 => true,
        0 => false,
        _ => return None, // autorepeat
    };
    let time = ev
        .timestamp()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    Some(RawKeyEvent {
        code: ev.code(),
        pressed,
        time,
    })
}

/// Run the blocking read loop on an already-opened device, calling `on_key` for
/// each key press/release. Returns `Ok(())` when `on_key` asks to stop
/// ([`ControlFlow::Break`]); returns `Err` on a device read failure so the
/// caller can reconnect (unplug/replug recovery).
pub fn read_keys(
    device: &mut Device,
    mut on_key: impl FnMut(RawKeyEvent) -> ControlFlow<()>,
) -> Result<()> {
    loop {
        let events = device.fetch_events().context("reading input events")?;
        for ev in events {
            if let Some(raw) = to_raw(&ev)
                && on_key(raw).is_break()
            {
                return Ok(());
            }
        }
    }
}

/// Does `selector` point at a readable device? (A cheap boundary probe for
/// `doctor`.)
pub fn device_available(selector: &str) -> bool {
    open_device(selector).is_ok()
}
