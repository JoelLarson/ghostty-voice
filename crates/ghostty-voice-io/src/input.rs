//! evdev input boundary (S8).
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
use ghostty_voice_core::key_combo::{Modifiers, codes};

/// Open the one input device named by `selector`:
///
/// - `auto` — the first device that looks like a real keyboard.
/// - `/dev/input/...` — that exact device path.
/// - `name:<substr>` — the first device whose name contains `<substr>`
///   (case-insensitive).
pub fn open_device(selector: &str) -> Result<(PathBuf, Device)> {
    let sel = selector.trim();
    if let Some(name) = sel.strip_prefix("name:") {
        let needle = name.trim().to_ascii_lowercase();
        for (path, device) in evdev::enumerate() {
            if device
                .name()
                .map(|n| n.to_ascii_lowercase().contains(&needle))
                .unwrap_or(false)
            {
                return Ok((path, device));
            }
        }
        bail!("no input device whose name contains {name:?}");
    }
    if sel == "auto" {
        for (path, device) in evdev::enumerate() {
            if is_keyboard(&device) {
                return Ok((path, device));
            }
        }
        bail!("no keyboard-like input device found under /dev/input");
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

/// A key captured by the bind flow: its evdev code and the modifiers held when
/// it went down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Captured {
    pub code: u16,
    pub modifiers: Modifiers,
}

/// Block until the next non-modifier key goes down, returning its code and the
/// modifier state at that instant. Used by `ghostty-voice-ctl bind` to capture
/// exactly what a key emits (so a remapped key is caught).
pub fn capture_combo(device: &mut Device) -> Result<Captured> {
    let mut mods = ModState::default();
    let mut captured: Option<Captured> = None;
    read_keys(device, |raw| {
        if mods.track(raw) {
            return ControlFlow::Continue(());
        }
        if raw.pressed {
            captured = Some(Captured {
                code: raw.code,
                modifiers: mods.modifiers(),
            });
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    })?;
    captured.context("device closed before a key was captured")
}

/// Minimal modifier tracker for the one-shot bind capture (the daemon uses the
/// core `KeyTracker`, which tracks its own).
#[derive(Default)]
struct ModState {
    left_shift: bool,
    right_shift: bool,
    left_ctrl: bool,
    right_ctrl: bool,
    left_alt: bool,
    right_alt: bool,
}

impl ModState {
    /// Update state if `raw` is a modifier; return whether it was one.
    fn track(&mut self, raw: RawKeyEvent) -> bool {
        let slot = match raw.code {
            codes::KEY_LEFTSHIFT => &mut self.left_shift,
            codes::KEY_RIGHTSHIFT => &mut self.right_shift,
            codes::KEY_LEFTCTRL => &mut self.left_ctrl,
            codes::KEY_RIGHTCTRL => &mut self.right_ctrl,
            codes::KEY_LEFTALT => &mut self.left_alt,
            codes::KEY_RIGHTALT => &mut self.right_alt,
            _ => return false,
        };
        *slot = raw.pressed;
        true
    }

    fn modifiers(&self) -> Modifiers {
        Modifiers {
            shift: self.left_shift || self.right_shift,
            ctrl: self.left_ctrl || self.right_ctrl,
            alt: self.left_alt || self.right_alt,
        }
    }
}

/// Resolve the device path the daemon will read, without holding it open — for
/// the `doctor`/bind flows to report what `auto` selects.
pub fn resolve_device_path(selector: &str) -> Result<PathBuf> {
    let (path, _device) = open_device(selector)?;
    Ok(path)
}

/// Does `selector` point at a readable device? (A cheap boundary probe for
/// `doctor`.)
pub fn device_available(selector: &str) -> bool {
    open_device(selector).is_ok()
}
