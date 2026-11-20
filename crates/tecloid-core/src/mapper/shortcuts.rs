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

    /// Characters of the Ctrl shortcut, looked up in the active layout in
    /// this order (Ctrl+= or Ctrl++ zooms in: QWERTZ has no unshifted `=`);
    /// the first is the US QWERTY fallback. Empty for non-character keys.
    fn characters(self) -> &'static [char] {
        match self {
            Self::Copy => &['c'],
            Self::Cut => &['x'],
            Self::Paste => &['v'],
            Self::Undo | Self::Redo => &['z'],
            Self::SelectAll => &['a'],
            Self::Find => &['f'],
            Self::Save => &['s'],
            Self::Print => &['p'],
            Self::NewTab | Self::ReopenClosedTab => &['t'],
            Self::CloseTab => &['w'],
            Self::ZoomIn => &['=', '+'],
            Self::ZoomOut => &['-'],
            Self::ZoomReset => &['0'],
            Self::NextTab | Self::PreviousTab => &[],
        }
    }

    /// Whether the keys differ in terminal windows, so the focused window
    /// has to be looked up.
    fn depends_on_terminal(self) -> bool {
        !matches!(self, Self::ZoomIn | Self::ZoomOut | Self::ZoomReset)
    }

    /// How the command is sent to a terminal emulator, where Ctrl+letter
    /// is a control character for the program running in it.
    fn in_terminal(self) -> InTerminal {
        match self {
            // Ctrl+Z suspends the program, Ctrl+S stops output, Ctrl+P is
            // shell history and Ctrl+Shift+T opens a tab.
            Self::Undo | Self::Redo | Self::Save | Self::Print | Self::ReopenClosedTab => {
                InTerminal::Skip
            }
            Self::Copy | Self::Cut | Self::SelectAll | Self::Find | Self::NewTab | Self::CloseTab => {
                InTerminal::Shifted
            }
            // Ctrl+PageDown/PageUp switch tabs there too (with Shift they
            // move the tab); paste follows its own setting on Linux.
            Self::Paste
            | Self::NextTab
            | Self::PreviousTab
            | Self::ZoomIn
            | Self::ZoomOut
            | Self::ZoomReset => InTerminal::Same,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InTerminal {
    /// Ctrl+Shift instead of Ctrl.
    Shifted,
    Same,
    /// No safe shortcut: nothing is sent.
    Skip,
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
            crate::text_input::portal::shortcut_key_for_letter(character).map(K::new)
        })
    }
    #[cfg(target_os = "windows")]
    {
        let terminal =
            command.depends_on_terminal() && crate::active_window::focused_is_terminal();
        windows_keystroke(
            command,
            terminal,
            crate::keyboard_layout::shortcut_key_for_letter,
        )
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

/// Key of `characters` in the active layout (`lookup`), or the QWERTY key
/// of the first one.
fn layout_key(characters: &[char], lookup: impl Fn(char) -> Option<K>) -> Option<K> {
    characters
        .iter()
        .find_map(|c| lookup(*c))
        .or_else(|| characters.first().map(|c| qwerty_key(*c)))
}

/// The main key of `command`'s Ctrl shortcut.
fn command_key(command: EditCommand, lookup: impl Fn(char) -> Option<K>) -> Option<K> {
    layout_key(command.characters(), lookup).or_else(|| tab_key(command))
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

/// Ctrl shortcut of `command` in a terminal window.
fn terminal_ctrl(command: EditCommand, key: K) -> Option<Keystroke> {
    match command.in_terminal() {
        InTerminal::Shifted => Some(ctrl(key, true)),
        InTerminal::Same => Some(ctrl(key, false)),
        InTerminal::Skip => None,
    }
}

/// `clipboard` is the paste setting resolved for the focused window; only
/// paste follows it, copy and cut always use the standard shortcut.
#[cfg(any(target_os = "linux", test))]
fn linux_keystroke(
    command: EditCommand,
    terminal: bool,
    clipboard: crate::config::model::ClipboardKeys,
    lookup: impl Fn(char) -> Option<K>,
) -> Option<Keystroke> {
    use crate::config::model::ClipboardKeys as C;
    use EditCommand as E;
    let key = command_key(command, lookup)?;
    if command == E::Paste {
        // An explicit paste setting is sent exactly as chosen.
        return Some(match clipboard {
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
        });
    }
    if terminal {
        return terminal_ctrl(command, key);
    }
    Some(ctrl(key, matches!(command, E::Redo | E::ReopenClosedTab)))
}

/// Windows Terminal and the console host copy with Ctrl+Insert (Ctrl+C
/// without a selection interrupts the program) and switch tabs with
/// Ctrl+Tab; redo is Ctrl+Y elsewhere.
#[cfg(any(target_os = "windows", test))]
fn windows_keystroke(
    command: EditCommand,
    terminal: bool,
    lookup: impl Fn(char) -> Option<K>,
) -> Option<Keystroke> {
    use EditCommand as E;
    if terminal {
        return match command {
            E::Copy => Some(Keystroke {
                mods: vec![K::KEY_LEFTCTRL],
                key: K::KEY_INSERT,
            }),
            E::NextTab => Some(ctrl(K::KEY_TAB, false)),
            E::PreviousTab => Some(ctrl(K::KEY_TAB, true)),
            command => terminal_ctrl(command, command_key(command, lookup)?),
        };
    }
    let key = if command == E::Redo {
        layout_key(&['y'], lookup)?
    } else {
        command_key(command, lookup)?
    };
    Some(ctrl(key, command == E::ReopenClosedTab))
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

    fn qwerty(c: char) -> Option<K> {
        Some(qwerty_key(c))
    }

    #[test]
    fn windows_commands_follow_layout_and_terminals() {
        use EditCommand as E;
        let dvorak = |c| match c {
            'y' => Some(K::KEY_T),
            _ => qwerty(c),
        };
        let run = |command, terminal| windows_keystroke(command, terminal, qwerty).map(|ks| chord(&ks));
        let ctrl = vec![K::KEY_LEFTCTRL];
        let ctrl_shift = vec![K::KEY_LEFTCTRL, K::KEY_LEFTSHIFT];
        let redo = windows_keystroke(E::Redo, false, dvorak).unwrap();
        assert_eq!(chord(&redo), (ctrl.clone(), K::KEY_T));
        assert_eq!(run(E::Copy, false), Some((ctrl.clone(), K::KEY_C)));
        assert_eq!(run(E::NextTab, false), Some((ctrl.clone(), K::KEY_PAGEDOWN)));
        assert_eq!(run(E::ReopenClosedTab, false), Some((ctrl_shift.clone(), K::KEY_T)));
        assert_eq!(run(E::ZoomIn, false), Some((ctrl.clone(), K::KEY_EQUAL)));
        // Terminals: Ctrl+C would interrupt the program.
        assert_eq!(run(E::Copy, true), Some((ctrl.clone(), K::KEY_INSERT)));
        assert_eq!(run(E::Paste, true), Some((ctrl.clone(), K::KEY_V)));
        assert_eq!(run(E::Find, true), Some((ctrl_shift.clone(), K::KEY_F)));
        assert_eq!(run(E::NextTab, true), Some((ctrl.clone(), K::KEY_TAB)));
        assert_eq!(run(E::PreviousTab, true), Some((ctrl_shift, K::KEY_TAB)));
        for command in [E::Undo, E::Redo, E::Save, E::Print, E::ReopenClosedTab] {
            assert_eq!(run(command, true), None, "{command:?}");
        }
    }

    #[test]
    fn zoom_in_falls_back_to_plus_without_an_unshifted_equal_sign() {
        // QWERTZ: `=` needs Shift, `+` is the key right of `ü`.
        let qwertz = |c| match c {
            '=' => None,
            '+' => Some(K::KEY_RIGHTBRACE),
            _ => qwerty(c),
        };
        let unknown = |_| None;
        assert_eq!(command_key(EditCommand::ZoomIn, qwertz), Some(K::KEY_RIGHTBRACE));
        assert_eq!(command_key(EditCommand::ZoomIn, unknown), Some(K::KEY_EQUAL));
        assert_eq!(command_key(EditCommand::Copy, unknown), Some(K::KEY_C));
    }

    #[test]
    fn linux_commands_follow_terminal_layout_and_paste_setting() {
        use crate::config::model::ClipboardKeys as C;
        use EditCommand as E;
        let dvorak = |c| match c {
            'c' => Some(K::KEY_I),
            'v' => Some(K::KEY_DOT),
            'f' => Some(K::KEY_Y),
            '=' => Some(K::KEY_RIGHTBRACE),
            _ => qwerty(c),
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
        for command in [E::Undo, E::Redo, E::Save, E::Print, E::ReopenClosedTab] {
            assert_eq!(run(command, true, C::CtrlShiftV), None, "{command:?}");
        }
        assert_eq!(
            run(E::SelectAll, true, C::CtrlShiftV),
            Some((ctrl_shift.clone(), K::KEY_A))
        );
        // With Shift, terminals move the tab instead of switching to it.
        assert_eq!(
            run(E::NextTab, true, C::CtrlShiftV),
            Some((ctrl.clone(), K::KEY_PAGEDOWN))
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
