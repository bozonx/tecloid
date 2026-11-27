//! evdev chords of the paste shortcuts.

use crate::config::model::ClipboardKeys;
use crate::events::TextInputFailure;

// evdev keycodes for modifier keys used in text injection.
pub(super) const KEY_LEFTCTRL_EVDEV: u32 = evdev::KeyCode::KEY_LEFTCTRL.code() as u32;
pub(super) const KEY_LEFTSHIFT_EVDEV: u32 = evdev::KeyCode::KEY_LEFTSHIFT.code() as u32;
pub(super) const KEY_V_EVDEV: u32 = evdev::KeyCode::KEY_V.code() as u32;
pub(super) const KEY_INSERT_EVDEV: u32 = evdev::KeyCode::KEY_INSERT.code() as u32;
pub(super) const KEY_PASTE_EVDEV: u32 = evdev::KeyCode::KEY_PASTE.code() as u32;

/// Modifiers and key of a paste shortcut as evdev codes. `v_key` is the
/// key applications read as `v` in the active layout; without it the
/// physical `KEY_V` is sent, which is right for QWERTY-like layouts.
pub(super) fn paste_keys(shortcut: ClipboardKeys, v_key: Option<u32>) -> (&'static [u32], u32) {
    let v = v_key.unwrap_or(KEY_V_EVDEV);
    match shortcut {
        ClipboardKeys::ShiftInsert => (&[KEY_LEFTSHIFT_EVDEV], KEY_INSERT_EVDEV),
        ClipboardKeys::CtrlV => (&[KEY_LEFTCTRL_EVDEV], v),
        ClipboardKeys::CtrlShiftV => (&[KEY_LEFTCTRL_EVDEV, KEY_LEFTSHIFT_EVDEV], v),
        ClipboardKeys::PasteKey => (&[], KEY_PASTE_EVDEV),
    }
}

/// Resolve automatic paste without guessing a physical Latin key.
pub(super) fn checked_paste_keys(
    shortcut: ClipboardKeys,
    v_key: Option<u32>,
) -> Result<(&'static [u32], u32), TextInputFailure> {
    let letter = !matches!(
        shortcut,
        ClipboardKeys::ShiftInsert | ClipboardKeys::PasteKey
    );
    if letter && v_key.is_none() {
        return Err(TextInputFailure::LayoutUnknown);
    }
    Ok(paste_keys(shortcut, v_key))
}
