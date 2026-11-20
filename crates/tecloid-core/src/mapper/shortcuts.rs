//! Standard application shortcuts behind portable `sys:` ids. A keymap names
//! the command (`sys:copy`, `sys:moveWordLeft`) and the keys come from the
//! OS, the focused window and the keyboard layout, so the keymap works on
//! any system and layout:
//!
//! * editing commands ([`EditCommand`]) are resolved each time they run:
//!   their letter is the key that types it in the active layout (Ctrl+C is
//!   the physical `KeyI` on Dvorak), terminals get Ctrl+Shift variants, and
//!   on Linux copy, cut and paste follow the paste settings;
//! * text navigation and window switching are fixed chords per OS
//!   ([`fixed_keystroke`]), compiled like key chords so `holdModifiers`
//!   works with them.

use super::action::{Keystroke, parse_action};
use super::key_code::KeyCode as K;

/// Editing command run by its standard application shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    SelectAll,
    Find,
    Save,
    NewTab,
    CloseTab,
}

impl EditCommand {
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "copy" => Self::Copy,
            "cut" => Self::Cut,
            "paste" => Self::Paste,
            "undo" => Self::Undo,
            "redo" => Self::Redo,
            "selectAll" => Self::SelectAll,
            "find" => Self::Find,
            "save" => Self::Save,
            "newTab" => Self::NewTab,
            "closeTab" => Self::CloseTab,
            _ => return None,
        })
    }

    /// Letter of the Ctrl shortcut on Linux.
    fn letter(self) -> char {
        match self {
            Self::Copy => 'c',
            Self::Cut => 'x',
            Self::Paste => 'v',
            Self::Undo | Self::Redo => 'z',
            Self::SelectAll => 'a',
            Self::Find => 'f',
            Self::Save => 's',
            Self::NewTab => 't',
            Self::CloseTab => 'w',
        }
    }
}

/// Chord of a `sys:` id that is the same standard shortcut in every
/// application on this OS.
pub fn fixed_keystroke(id: &str) -> Option<Keystroke> {
    let chord = match id {
        "moveWordLeft" => "Ctrl+ArrowLeft",
        "moveWordRight" => "Ctrl+ArrowRight",
        "selectWordLeft" => "Ctrl+Shift+ArrowLeft",
        "selectWordRight" => "Ctrl+Shift+ArrowRight",
        "deleteWordBackward" => "Ctrl+Backspace",
        "deleteWordForward" => "Ctrl+Delete",
        "moveToDocumentStart" => "Ctrl+Home",
        "moveToDocumentEnd" => "Ctrl+End",
        "switchWindow" => "Alt+Tab",
        _ => return None,
    };
    parse_action(chord)
}

/// The keys `command` sends now, for the focused window and active layout.
pub fn edit_keystroke(command: EditCommand) -> Keystroke {
    #[cfg(target_os = "linux")]
    {
        let (terminal, clipboard) = crate::text_input::portal::focused_clipboard_keys();
        linux_keystroke(command, terminal, clipboard, |letter| {
            crate::text_input::portal::shortcut_key_for_letter(letter)
                .map(K::new)
                .unwrap_or_else(|| qwerty_key(letter))
        })
    }
    #[cfg(target_os = "windows")]
    {
        windows_keystroke(command, |letter| {
            crate::keyboard_layout::shortcut_key_for_letter(letter)
                .unwrap_or_else(|| qwerty_key(letter))
        })
    }
}

/// The physical QWERTY key of a Latin letter: right when the layout is
/// unknown or has no Latin letters of its own.
fn qwerty_key(letter: char) -> K {
    parse_action(&format!("Key{}", letter.to_ascii_uppercase())).map_or(K::KEY_RESERVED, |ks| ks.key)
}

fn ctrl(key: K, shift: bool) -> Keystroke {
    let mut mods = vec![K::KEY_LEFTCTRL];
    if shift {
        mods.push(K::KEY_LEFTSHIFT);
    }
    Keystroke { mods, key }
}

#[cfg(target_os = "linux")]
fn linux_keystroke(
    command: EditCommand,
    terminal: bool,
    clipboard: crate::config::model::ClipboardKeys,
    letter: impl Fn(char) -> K,
) -> Keystroke {
    use crate::config::model::ClipboardKeys as C;
    use EditCommand as E;
    let key = letter(command.letter());
    match command {
        E::Copy | E::Cut | E::Paste => match clipboard {
            C::PasteKey => Keystroke {
                mods: Vec::new(),
                key: match command {
                    E::Copy => K::KEY_COPY,
                    E::Cut => K::KEY_CUT,
                    _ => K::KEY_PASTE,
                },
            },
            C::ShiftInsert => match command {
                E::Copy => Keystroke {
                    mods: vec![K::KEY_LEFTCTRL],
                    key: K::KEY_INSERT,
                },
                E::Cut => Keystroke {
                    mods: vec![K::KEY_LEFTSHIFT],
                    key: K::KEY_DELETE,
                },
                _ => Keystroke {
                    mods: vec![K::KEY_LEFTSHIFT],
                    key: K::KEY_INSERT,
                },
            },
            C::CtrlShiftV => ctrl(key, true),
            // Ctrl+C interrupts the program in a terminal: copy there
            // always takes Shift, whatever the paste setting.
            C::CtrlV => ctrl(key, terminal && command != E::Paste),
        },
        E::Find | E::NewTab | E::CloseTab => ctrl(key, terminal),
        E::Redo => ctrl(key, true),
        E::Undo | E::SelectAll | E::Save => ctrl(key, false),
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_keystroke(command: EditCommand, letter: impl Fn(char) -> K) -> Keystroke {
    let character = match command {
        EditCommand::Redo => 'y',
        command => command.letter(),
    };
    ctrl(letter(character), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(ks: &Keystroke) -> (Vec<K>, K) {
        (ks.mods.clone(), ks.key)
    }

    #[test]
    fn fixed_ids_are_system_actions() {
        for id in [
            "moveWordLeft",
            "moveWordRight",
            "selectWordLeft",
            "selectWordRight",
            "deleteWordBackward",
            "deleteWordForward",
            "moveToDocumentStart",
            "moveToDocumentEnd",
            "switchWindow",
        ] {
            assert!(fixed_keystroke(id).is_some(), "{id}");
            assert!(crate::config::actions::system_action_name(id).is_some(), "{id}");
        }
        assert_eq!(
            chord(&fixed_keystroke("switchWindow").unwrap()),
            (vec![K::KEY_LEFTALT], K::KEY_TAB)
        );
    }

    #[test]
    fn edit_ids_are_system_actions() {
        for id in [
            "copy", "cut", "paste", "undo", "redo", "selectAll", "find", "save", "newTab",
            "closeTab",
        ] {
            assert!(EditCommand::from_id(id).is_some(), "{id}");
            assert!(crate::config::actions::system_action_name(id).is_some(), "{id}");
        }
    }

    #[test]
    fn windows_redo_is_ctrl_y_in_the_layout() {
        let dvorak = |c| match c {
            'y' => K::KEY_T,
            _ => qwerty_key(c),
        };
        let redo = windows_keystroke(EditCommand::Redo, dvorak);
        assert_eq!(chord(&redo), (vec![K::KEY_LEFTCTRL], K::KEY_T));
        let copy = windows_keystroke(EditCommand::Copy, qwerty_key);
        assert_eq!(chord(&copy), (vec![K::KEY_LEFTCTRL], K::KEY_C));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_commands_follow_terminal_layout_and_paste_settings() {
        use crate::config::model::ClipboardKeys as C;
        use EditCommand as E;
        let dvorak = |c| match c {
            'c' => K::KEY_I,
            'v' => K::KEY_DOT,
            'f' => K::KEY_Y,
            _ => qwerty_key(c),
        };
        let run = |command, terminal, clipboard| {
            chord(&linux_keystroke(command, terminal, clipboard, dvorak))
        };
        let ctrl = vec![K::KEY_LEFTCTRL];
        let ctrl_shift = vec![K::KEY_LEFTCTRL, K::KEY_LEFTSHIFT];
        assert_eq!(run(E::Copy, false, C::CtrlV), (ctrl.clone(), K::KEY_I));
        assert_eq!(run(E::Copy, true, C::CtrlV), (ctrl_shift.clone(), K::KEY_I));
        assert_eq!(run(E::Paste, true, C::CtrlV), (ctrl.clone(), K::KEY_DOT));
        assert_eq!(run(E::Paste, true, C::CtrlShiftV), (ctrl_shift.clone(), K::KEY_DOT));
        assert_eq!(
            run(E::Paste, false, C::ShiftInsert),
            (vec![K::KEY_LEFTSHIFT], K::KEY_INSERT)
        );
        assert_eq!(run(E::Copy, false, C::ShiftInsert), (ctrl.clone(), K::KEY_INSERT));
        assert_eq!(
            run(E::Cut, false, C::ShiftInsert),
            (vec![K::KEY_LEFTSHIFT], K::KEY_DELETE)
        );
        assert_eq!(run(E::Cut, true, C::PasteKey), (vec![], K::KEY_CUT));
        assert_eq!(run(E::Find, true, C::CtrlV), (ctrl_shift.clone(), K::KEY_Y));
        assert_eq!(run(E::Find, false, C::CtrlV), (ctrl.clone(), K::KEY_Y));
        assert_eq!(run(E::Redo, false, C::CtrlV), (ctrl_shift, K::KEY_Z));
        assert_eq!(run(E::Undo, true, C::CtrlShiftV), (ctrl, K::KEY_Z));
    }
}
