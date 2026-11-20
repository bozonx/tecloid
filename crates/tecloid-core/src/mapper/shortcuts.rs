//! Standard application shortcuts behind portable `sys:` ids. A keymap names
//! the command (`sys:copy`, `sys:moveWordLeft`) and the keys come from the
//! OS, the focused window and the keyboard layout, so the keymap works on
//! any system and layout:
//!
//! * editing commands ([`EditCommand`]) are resolved each time they run:
//!   their letter is the key that types it in the active layout (Ctrl+C is
//!   the physical `KeyI` on Dvorak), terminals get Ctrl+Shift variants or
//!   nothing where the shortcut is a terminal control key, and on Linux
//!   paste follows the paste settings;
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
    Print,
    NewTab,
    CloseTab,
    ReopenClosedTab,
    NextTab,
    PreviousTab,
    ZoomIn,
    ZoomOut,
    ZoomReset,
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
            "print" => Self::Print,
            "newTab" => Self::NewTab,
            "closeTab" => Self::CloseTab,
            "reopenClosedTab" => Self::ReopenClosedTab,
            "nextTab" => Self::NextTab,
            "previousTab" => Self::PreviousTab,
            "zoomIn" => Self::ZoomIn,
            "zoomOut" => Self::ZoomOut,
            "zoomReset" => Self::ZoomReset,
            _ => return None,
        })
    }

    /// Character of the Ctrl shortcut on Linux, looked up in the active
    /// layout; `None` for shortcuts on non-character keys.
    fn character(self) -> Option<char> {
        Some(match self {
            Self::Copy => 'c',
            Self::Cut => 'x',
            Self::Paste => 'v',
            Self::Undo | Self::Redo => 'z',
            Self::SelectAll => 'a',
            Self::Find => 'f',
            Self::Save => 's',
            Self::Print => 'p',
            Self::NewTab | Self::ReopenClosedTab => 't',
            Self::CloseTab => 'w',
            Self::ZoomIn => '=',
            Self::ZoomOut => '-',
            Self::ZoomReset => '0',
            Self::NextTab | Self::PreviousTab => return None,
        })
    }

    /// Whether the keys differ in terminal windows, so the focused window
    /// has to be looked up.
    #[cfg(target_os = "linux")]
    fn depends_on_terminal(self) -> bool {
        !matches!(self, Self::Print | Self::ZoomIn | Self::ZoomOut | Self::ZoomReset)
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
        "moveToLineStart" => "Home",
        "moveToLineEnd" => "End",
        "moveToDocumentStart" => "Ctrl+Home",
        "moveToDocumentEnd" => "Ctrl+End",
        "switchWindow" => "Alt+Tab",
        _ => return None,
    };
    parse_action(chord)
}

/// The keys `command` sends now, for the focused window and active layout;
/// `None` when the command has no safe shortcut there (undo in a terminal
/// would be Ctrl+Z, which suspends the program).
pub fn edit_keystroke(command: EditCommand) -> Option<Keystroke> {
    #[cfg(target_os = "linux")]
    {
        let (terminal, clipboard) = if command.depends_on_terminal() {
            crate::text_input::portal::focused_clipboard_keys()
        } else {
            (false, crate::config::model::ClipboardKeys::CtrlV)
        };
        linux_keystroke(command, terminal, clipboard, |character| {
            crate::text_input::portal::shortcut_key_for_letter(character)
                .map(K::new)
                .unwrap_or_else(|| qwerty_key(character))
        })
    }
    #[cfg(target_os = "windows")]
    {
        Some(windows_keystroke(command, |character| {
            crate::keyboard_layout::shortcut_key_for_letter(character)
                .unwrap_or_else(|| qwerty_key(character))
        }))
    }
}

/// The physical US QWERTY key of a shortcut character: right when the
/// layout is unknown or does not type it without modifiers.
fn qwerty_key(character: char) -> K {
    match character {
        '=' => K::KEY_EQUAL,
        '-' => K::KEY_MINUS,
        '0' => K::KEY_0,
        letter => parse_action(&format!("Key{}", letter.to_ascii_uppercase()))
            .map_or(K::KEY_RESERVED, |ks| ks.key),
    }
}

fn ctrl(key: K, shift: bool) -> Keystroke {
    let mut mods = vec![K::KEY_LEFTCTRL];
    if shift {
        mods.push(K::KEY_LEFTSHIFT);
    }
    Keystroke { mods, key }
}

/// Ctrl+PageDown / Ctrl+PageUp for the tab commands.
fn tab_key(command: EditCommand) -> Option<K> {
    match command {
        EditCommand::NextTab => Some(K::KEY_PAGEDOWN),
        EditCommand::PreviousTab => Some(K::KEY_PAGEUP),
        _ => None,
    }
}

/// `clipboard` is the paste setting resolved for the focused window; only
/// paste follows it, copy and cut always use the standard shortcut.
#[cfg(target_os = "linux")]
fn linux_keystroke(
    command: EditCommand,
    terminal: bool,
    clipboard: crate::config::model::ClipboardKeys,
    character: impl Fn(char) -> K,
) -> Option<Keystroke> {
    use crate::config::model::ClipboardKeys as C;
    use EditCommand as E;
    let key = command
        .character()
        .map(&character)
        .or_else(|| tab_key(command))?;
    Some(match command {
        // An explicit paste setting is sent exactly as chosen.
        E::Paste => match clipboard {
            C::PasteKey => Keystroke {
                mods: Vec::new(),
                key: K::KEY_PASTE,
            },
            C::ShiftInsert => Keystroke {
                mods: vec![K::KEY_LEFTSHIFT],
                key: K::KEY_INSERT,
            },
            C::CtrlV => ctrl(key, false),
            C::CtrlShiftV => ctrl(key, true),
        },
        // Ctrl+Z suspends the program, Ctrl+S stops terminal output, and
        // Ctrl+Shift+T is the terminal's new tab: no safe shortcut there.
        E::Undo | E::Redo | E::Save | E::ReopenClosedTab if terminal => return None,
        E::Copy
        | E::Cut
        | E::SelectAll
        | E::Find
        | E::NewTab
        | E::CloseTab
        | E::NextTab
        | E::PreviousTab => ctrl(key, terminal),
        E::Redo | E::ReopenClosedTab => ctrl(key, true),
        E::Undo | E::Save | E::Print | E::ZoomIn | E::ZoomOut | E::ZoomReset => {
            ctrl(key, false)
        }
    })
}

#[cfg(any(target_os = "windows", test))]
fn windows_keystroke(command: EditCommand, character: impl Fn(char) -> K) -> Keystroke {
    let key = match command {
        EditCommand::Redo => character('y'),
        command => match command.character() {
            Some(c) => character(c),
            None => tab_key(command).unwrap_or(K::KEY_RESERVED),
        },
    };
    ctrl(key, command == EditCommand::ReopenClosedTab)
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
            "moveToLineStart",
            "moveToLineEnd",
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
            "copy",
            "cut",
            "paste",
            "undo",
            "redo",
            "selectAll",
            "find",
            "save",
            "print",
            "newTab",
            "closeTab",
            "reopenClosedTab",
            "nextTab",
            "previousTab",
            "zoomIn",
            "zoomOut",
            "zoomReset",
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
        let next = windows_keystroke(EditCommand::NextTab, qwerty_key);
        assert_eq!(chord(&next), (vec![K::KEY_LEFTCTRL], K::KEY_PAGEDOWN));
        let reopen = windows_keystroke(EditCommand::ReopenClosedTab, qwerty_key);
        assert_eq!(
            chord(&reopen),
            (vec![K::KEY_LEFTCTRL, K::KEY_LEFTSHIFT], K::KEY_T)
        );
        let zoom = windows_keystroke(EditCommand::ZoomIn, qwerty_key);
        assert_eq!(chord(&zoom), (vec![K::KEY_LEFTCTRL], K::KEY_EQUAL));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_commands_follow_terminal_layout_and_paste_setting() {
        use crate::config::model::ClipboardKeys as C;
        use EditCommand as E;
        let dvorak = |c| match c {
            'c' => K::KEY_I,
            'v' => K::KEY_DOT,
            'f' => K::KEY_Y,
            '=' => K::KEY_RIGHTBRACE,
            _ => qwerty_key(c),
        };
        let run = |command, terminal, clipboard| {
            linux_keystroke(command, terminal, clipboard, dvorak).map(|ks| chord(&ks))
        };
        let ctrl = vec![K::KEY_LEFTCTRL];
        let ctrl_shift = vec![K::KEY_LEFTCTRL, K::KEY_LEFTSHIFT];
        // Copy and cut ignore the paste setting.
        for clipboard in [C::CtrlV, C::ShiftInsert, C::PasteKey, C::CtrlShiftV] {
            assert_eq!(run(E::Copy, false, clipboard), Some((ctrl.clone(), K::KEY_I)));
            assert_eq!(
                run(E::Copy, true, clipboard),
                Some((ctrl_shift.clone(), K::KEY_I))
            );
            assert_eq!(run(E::Cut, false, clipboard), Some((ctrl.clone(), K::KEY_X)));
        }
        // Paste sends the chosen keys, in terminals too.
        assert_eq!(run(E::Paste, true, C::CtrlV), Some((ctrl.clone(), K::KEY_DOT)));
        assert_eq!(
            run(E::Paste, false, C::CtrlShiftV),
            Some((ctrl_shift.clone(), K::KEY_DOT))
        );
        assert_eq!(
            run(E::Paste, true, C::ShiftInsert),
            Some((vec![K::KEY_LEFTSHIFT], K::KEY_INSERT))
        );
        assert_eq!(run(E::Paste, false, C::PasteKey), Some((vec![], K::KEY_PASTE)));
        assert_eq!(run(E::Find, true, C::CtrlV), Some((ctrl_shift.clone(), K::KEY_Y)));
        assert_eq!(run(E::Find, false, C::CtrlV), Some((ctrl.clone(), K::KEY_Y)));
        assert_eq!(run(E::Redo, false, C::CtrlV), Some((ctrl_shift.clone(), K::KEY_Z)));
        assert_eq!(run(E::Undo, false, C::CtrlV), Some((ctrl.clone(), K::KEY_Z)));
        // Terminal control keys are never sent.
        for command in [E::Undo, E::Redo, E::Save, E::ReopenClosedTab] {
            assert_eq!(run(command, true, C::CtrlShiftV), None, "{command:?}");
        }
        assert_eq!(
            run(E::SelectAll, true, C::CtrlShiftV),
            Some((ctrl_shift.clone(), K::KEY_A))
        );
        assert_eq!(
            run(E::NextTab, true, C::CtrlShiftV),
            Some((ctrl_shift.clone(), K::KEY_PAGEDOWN))
        );
        assert_eq!(
            run(E::PreviousTab, false, C::CtrlV),
            Some((ctrl.clone(), K::KEY_PAGEUP))
        );
        assert_eq!(
            run(E::ReopenClosedTab, false, C::CtrlV),
            Some((ctrl_shift, K::KEY_T))
        );
        assert_eq!(run(E::ZoomIn, false, C::CtrlV), Some((ctrl.clone(), K::KEY_RIGHTBRACE)));
        assert_eq!(run(E::ZoomReset, false, C::CtrlV), Some((ctrl, K::KEY_0)));
    }
}
