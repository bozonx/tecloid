//! Built-in macros — globally available macros that resolve
//! `builtin:<id>` at runtime regardless of the active keymap preset.
//!
//! Built-in and user macros have independent action namespaces. Steps use
//! `sys:` shortcuts for clipboard and word movement, so the macros follow
//! the OS, the keyboard layout and terminal windows like those actions.

pub struct BuiltinMacro {
    pub id: &'static str,
    /// English display name.
    pub name: &'static str,
    pub steps: &'static [&'static str],
}

pub const BUILTIN_MACROS: &[BuiltinMacro] = &[
    // nav layer
    BuiltinMacro {
        id: "insertEmptyLineAbove",
        name: "Empty line above",
        steps: &["Home", "Enter", "ArrowUp"],
    },
    BuiltinMacro {
        id: "goToNextLineEnd",
        name: "End of next line",
        steps: &["ArrowDown", "End"],
    },
    BuiltinMacro {
        id: "goToPreviousLineEnd",
        name: "End of previous line",
        steps: &["ArrowUp", "End"],
    },
    BuiltinMacro {
        id: "goTo5LinesUp",
        name: "Move 5 lines up",
        steps: &["ArrowUp", "ArrowUp", "ArrowUp", "ArrowUp", "ArrowUp"],
    },
    BuiltinMacro {
        id: "duplicateLine",
        name: "Duplicate line",
        steps: &["End", "Shift+Home", "sys:copy", "End", "Enter", "sys:paste"],
    },
    BuiltinMacro {
        id: "insertSpaceAfterNextChar",
        name: "Space after next character",
        steps: &["ArrowRight", "Space"],
    },
    BuiltinMacro {
        id: "insertEmptyLineBelow",
        name: "Empty line below",
        steps: &["End", "Enter"],
    },
    BuiltinMacro {
        id: "cutWord",
        name: "Cut word under cursor",
        steps: &["sys:moveWordRight", "sys:selectWordLeft", "sys:cut"],
    },
    BuiltinMacro {
        id: "copyWord",
        name: "Copy word under cursor",
        steps: &[
            "sys:moveWordRight",
            "sys:selectWordLeft",
            "sys:copy",
            "ArrowLeft",
        ],
    },
    BuiltinMacro {
        id: "pasteOnNewLineAbove",
        name: "Paste on new line above",
        steps: &["Home", "Enter", "ArrowUp", "sys:paste"],
    },
    BuiltinMacro {
        id: "replaceWordWithClipboard",
        name: "Replace word with clipboard",
        steps: &["sys:moveWordRight", "sys:selectWordLeft", "sys:paste"],
    },
    BuiltinMacro {
        id: "goToNextLineStart",
        name: "Start of next line",
        steps: &["ArrowDown", "Home"],
    },
    BuiltinMacro {
        id: "goToPreviousLineStart",
        name: "Start of previous line",
        steps: &["ArrowUp", "Home"],
    },
    BuiltinMacro {
        id: "goTo5LinesDown",
        name: "Move 5 lines down",
        steps: &[
            "ArrowDown",
            "ArrowDown",
            "ArrowDown",
            "ArrowDown",
            "ArrowDown",
        ],
    },
    BuiltinMacro {
        id: "pasteOnNewLineBelow",
        name: "Paste on new line below",
        steps: &["End", "Enter", "sys:paste"],
    },
    // select layer
    BuiltinMacro {
        id: "cutToLineStart",
        name: "Cut to line start",
        steps: &["Shift+Home", "sys:cut"],
    },
    BuiltinMacro {
        id: "cutToLineEnd",
        name: "Cut to line end",
        steps: &["Shift+End", "sys:cut"],
    },
    BuiltinMacro {
        id: "cutLineText",
        name: "Cut line text",
        steps: &["Home", "Shift+End", "sys:cut"],
    },
    BuiltinMacro {
        id: "cutLine",
        name: "Cut whole line",
        steps: &["Home", "Shift+End", "sys:cut", "Delete"],
    },
    BuiltinMacro {
        id: "select5LinesUp",
        name: "Select 5 lines up",
        steps: &[
            "Shift+ArrowUp",
            "Shift+ArrowUp",
            "Shift+ArrowUp",
            "Shift+ArrowUp",
            "Shift+ArrowUp",
        ],
    },
    BuiltinMacro {
        id: "copyToLineStart",
        name: "Copy to line start",
        steps: &["Shift+Home", "sys:copy", "Home"],
    },
    BuiltinMacro {
        id: "copyToLineEnd",
        name: "Copy to line end",
        steps: &["Shift+End", "sys:copy", "End"],
    },
    BuiltinMacro {
        id: "copyLineText",
        name: "Copy line text",
        steps: &["Home", "Shift+End", "sys:copy", "Home"],
    },
    BuiltinMacro {
        id: "selectLineText",
        name: "Select line text",
        steps: &["Home", "Shift+End"],
    },
    BuiltinMacro {
        id: "selectWord",
        name: "Select word under cursor",
        steps: &["sys:moveWordRight", "sys:selectWordLeft"],
    },
    BuiltinMacro {
        id: "replaceToLineStartWithClipboard",
        name: "Replace to line start with clipboard",
        steps: &["Shift+Home", "sys:paste"],
    },
    BuiltinMacro {
        id: "replaceToLineEndWithClipboard",
        name: "Replace to line end with clipboard",
        steps: &["Shift+End", "sys:paste"],
    },
    BuiltinMacro {
        id: "replaceLineTextWithClipboard",
        name: "Replace line text with clipboard",
        steps: &["Home", "Shift+End", "sys:paste"],
    },
    BuiltinMacro {
        id: "select5LinesDown",
        name: "Select 5 lines down",
        steps: &[
            "Shift+ArrowDown",
            "Shift+ArrowDown",
            "Shift+ArrowDown",
            "Shift+ArrowDown",
            "Shift+ArrowDown",
        ],
    },
];

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::BUILTIN_MACROS;
    use crate::mapper::action::parse_action;

    #[test]
    fn all_builtin_macros_have_valid_steps() {
        use crate::config::actions::{Action, is_shortcut_system_action};
        for m in BUILTIN_MACROS {
            for step in m.steps {
                let valid = match Action::parse(Some(step)) {
                    // Only shortcuts: desktop actions are in development.
                    Action::System(id) => is_shortcut_system_action(&id),
                    _ => parse_action(step).is_some(),
                };
                assert!(valid, "built-in macro {} has invalid step: {}", m.id, step);
            }
        }
    }

    #[test]
    fn all_builtin_macro_ids_are_non_empty_and_unique() {
        let mut ids = std::collections::HashSet::new();
        for m in BUILTIN_MACROS {
            assert!(!m.id.is_empty(), "built-in macro id cannot be empty");
            assert!(ids.insert(m.id), "duplicate built-in macro id: {}", m.id);
        }
    }
}
