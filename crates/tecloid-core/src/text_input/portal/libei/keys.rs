//! Typing through the libei keyboard device: keysyms of the keymap the
//! compositor sent, and the whole-string clipboard paste of
//! `libeiWithClipboardFallback` when the keymap lacks a character.
//!
//! Needs no layout reported by the desktop; only the group falls back to the
//! active layout index when the compositor has not reported one yet.

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use reis::ei::keyboard::KeyState;

use super::{LibeiBackend, monotonic_micros};
use crate::config::model::ClipboardKeys;
use crate::events::TextInputFailure;
use crate::text_input::job::TextRequest;
use crate::text_input::portal::hardware::{HardwareState, wait_for_modifiers};
use crate::text_input::portal::inject::prepare_clipboard;
use crate::text_input::portal::paste_keys::checked_paste_keys;
use crate::text_input::portal::xkb::{
    KeyTable, missing_characters, paste_v_key_from_keymap_text, table_from_keymap_text, text_keysym,
};
use crate::text_input::portal::{
    EI_EMULATION_TIMEOUT, EI_KEYBOARD_TIMEOUT, MODIFIER_RELEASE_TIMEOUT, POLL_INTERVAL,
};

impl LibeiBackend {
    /// Type `text`, pasting it with `clipboard_fallback` when the layout
    /// lacks a character. `Err` means the EI connection is unusable; other
    /// failures are reported and leave the session open.
    pub(in crate::text_input::portal) fn type_text(
        &mut self,
        text: &str,
        clipboard_fallback: Option<ClipboardKeys>,
        request: &TextRequest,
    ) -> Result<(), TextInputFailure> {
        request.check()?;
        self.pump_events()
            .map_err(TextInputFailure::BackendUnavailable)?;
        if !self.has_keyboard() {
            self.wait_until(EI_KEYBOARD_TIMEOUT, Self::has_keyboard)
                .map_err(TextInputFailure::BackendUnavailable)?;
        }
        // Events sent while the compositor paused the device are a protocol
        // error, e.g. on the lock screen.
        if !self.keyboard_emulating
            && self
                .wait_until(EI_EMULATION_TIMEOUT, |backend| backend.keyboard_emulating)
                .is_err()
        {
            return Err(TextInputFailure::BackendUnavailable(
                "the compositor paused the input device".into(),
            ));
        }
        if self.depressed != 0 || self.latched != 0 {
            return Err(TextInputFailure::NativeInput(
                "release or unlatch keyboard modifiers before text input".into(),
            ));
        }
        self.synchronize(request)
            .map_err(TextInputFailure::BackendUnavailable)?;
        let hardware = HardwareState::capture();
        hardware
            .modifiers_released()
            .map_err(temporary_input_failure)?;
        let table = self.key_table()?;
        let table_identity = self.table.as_ref().map(|(identity, _)| *identity);
        request.check()?;
        let text = text.replace("\r\n", "\n");
        if missing_characters(&table, &text) {
            if let Some(shortcut) = clipboard_fallback {
                log::debug!(
                    "[text] libei + clipboard: pasting entire string; keymap lacks a character"
                );
                return self.paste_text(&text, shortcut, request);
            }
            return Err(TextInputFailure::MissingCharacters);
        }
        request.start_typing()?;
        for ch in text.chars() {
            self.pump_events()
                .map_err(TextInputFailure::BackendUnavailable)?;
            request.check()?;
            if !self.keyboard_emulating
                || self
                    .table
                    .as_ref()
                    .is_none_or(|(_, current)| !Arc::ptr_eq(current, &table))
                || self.table.as_ref().map(|(identity, _)| *identity) != table_identity
                || self.group.is_some_and(|group| {
                    self.table
                        .as_ref()
                        .is_some_and(|(cached, _)| cached.0 != group || cached.1 != self.locked)
                })
                || self.depressed != 0
                || self.latched != 0
            {
                return Err(TextInputFailure::NativeInput(
                    "keyboard state changed during text input".into(),
                ));
            }
            hardware
                .modifiers_released()
                .map_err(temporary_input_failure)?;
            if let Some(entry) = table.get(&text_keysym(ch)) {
                self.inject_keycode_combo(&entry.mods, entry.evdev, request)
                    .map_err(TextInputFailure::BackendUnavailable)?;
                request.mark_progress();
            }
        }
        Ok(())
    }

    /// Key table of the layout the compositor uses now. Without a keymap
    /// from the compositor, fail rather than guess its options. Without
    /// a reported group, use the active layout's index.
    fn key_table(&mut self) -> Result<Arc<KeyTable>, TextInputFailure> {
        let Some(keymap) = self.keymap.as_deref() else {
            return Err(TextInputFailure::BackendUnavailable(
                "the compositor did not provide a keyboard keymap".into(),
            ));
        };
        let group = self
            .group
            .or_else(|| crate::runtime_state::keyboard_layout().map(|layout| layout.index))
            .ok_or(TextInputFailure::LayoutUnknown)?;
        if let Some((cached, table)) = &self.table
            && *cached == (group, self.locked)
        {
            return Ok(Arc::clone(table));
        }
        let table = Arc::new(
            table_from_keymap_text(keymap, group, self.locked).ok_or_else(|| {
                TextInputFailure::BackendUnavailable("cannot read the compositor keymap".into())
            })?,
        );
        self.table = Some(((group, self.locked), Arc::clone(&table)));
        Ok(table)
    }

    fn paste_text(
        &mut self,
        text: &str,
        shortcut: ClipboardKeys,
        request: &TextRequest,
    ) -> Result<(), TextInputFailure> {
        let clipboard = prepare_clipboard(&self.clipboard, text, request)?;
        request.check()?;
        wait_for_modifiers(request).map_err(temporary_input_failure)?;
        let deadline = Instant::now() + MODIFIER_RELEASE_TIMEOUT;
        loop {
            self.pump_events()
                .map_err(TextInputFailure::BackendUnavailable)?;
            request.check()?;
            if self.depressed == 0 && self.latched == 0 {
                break;
            }
            if Instant::now() >= deadline {
                return Err(TextInputFailure::NativeInput(
                    "release or unlatch keyboard modifiers before paste".into(),
                ));
            }
            thread::sleep(POLL_INTERVAL);
        }
        let group = self
            .group
            .or_else(|| crate::runtime_state::keyboard_layout().map(|layout| layout.index))
            .unwrap_or(0);
        let v_key = self
            .keymap
            .as_deref()
            .and_then(|keymap| paste_v_key_from_keymap_text(keymap, group));
        let (mods, key) = checked_paste_keys(shortcut, v_key)?;
        clipboard.arm().map_err(TextInputFailure::Clipboard)?;
        let result = self
            .inject_keycode_combo(mods, key, request)
            .map_err(TextInputFailure::BackendUnavailable);
        let transfer = clipboard
            .finish(result.is_ok())
            .map_err(TextInputFailure::Clipboard);
        result.and(transfer)
    }

    fn inject_keycode_combo(
        &mut self,
        mods: &[u32],
        key: u32,
        request: &TextRequest,
    ) -> Result<(), String> {
        let mut pressed = Vec::new();
        let mut result = (|| {
            for &m in mods {
                self.inject_key(m, KeyState::Press)?;
                pressed.push(m);
            }
            self.inject_key(key, KeyState::Press)?;
            self.inject_key(key, KeyState::Released)
        })();
        if result.is_err() {
            let _ = self.inject_key(key, KeyState::Released);
        }
        for m in pressed.into_iter().rev() {
            if let Err(error) = self.inject_key(m, KeyState::Released)
                && result.is_ok()
            {
                result = Err(error);
            }
        }
        result?;
        self.synchronize(request)
    }

    fn inject_key(&mut self, key: u32, state: KeyState) -> Result<(), String> {
        let Some((device, keyboard)) = self.keyboard_device.as_ref() else {
            return Err("no EI keyboard device".into());
        };
        keyboard.key(key, state);
        device.frame(self.connection.serial(), monotonic_micros());
        self.context.flush().map_err(|e| format!("EI flush: {e}"))
    }
}

// Physical input can interrupt a job without invalidating the EI connection.
fn temporary_input_failure(error: TextInputFailure) -> TextInputFailure {
    match error {
        TextInputFailure::BackendUnavailable(detail) => TextInputFailure::NativeInput(detail),
        other => other,
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[test]
    fn interrupted_input_preserves_the_session_and_destination_errors() {
        assert_eq!(
            temporary_input_failure(TextInputFailure::BackendUnavailable("held modifier".into())),
            TextInputFailure::NativeInput("held modifier".into())
        );
        assert_eq!(
            temporary_input_failure(TextInputFailure::DestinationChanged),
            TextInputFailure::DestinationChanged
        );
    }
}
