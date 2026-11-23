//! Action strings (`KeyA`, `Ctrl+KeyC`, `macro:id`, `builtin:id`, `cmd:id`, `sys:id`,
//! `app:id`, `kde:component/shortcut`, `xdg:app-id/action`, `text:…`,
//! `pause:ms`) and the
//! catalogs of built-in actions.

use super::model::ResolvedAppConfig;
use crate::mapper::builtin_macros::BUILTIN_MACROS;

/// A bindable action, parsed from its canonical string form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Pass the physical key through (`""` / absent field).
    Native,
    /// Suppress the key (`null`).
    Swallow,
    /// Key or chord, e.g. `Ctrl+KeyC`.
    Keys(String),
    /// User macro (`macro:<id>`).
    Macro(String),
    /// Built-in macro (`builtin:<id>`), independent of user macro IDs.
    BuiltinMacro(String),
    Command(String),
    System(String),
    App(String),
    /// KDE global shortcut, `component/shortcut` as named by kglobalaccel.
    KdeShortcut(String),
    /// Action of an installed application from its freedesktop desktop
    /// entry, `desktop-id/action` (`Actions=` key; Linux only).
    DesktopEntry(String),
    Text(String),
    /// Macro-only delay in milliseconds.
    Pause(String),
    /// A `word:` prefix this build does not know, most likely an action
    /// type from a newer version. Kept verbatim so saving preserves it.
    Unknown(String),
}

/// Every action prefix, in the order [`Action::parse`] tries them. Stored
/// in user files, so a prefix is never removed (`names.v1.txt`).
pub const PREFIXES: [&str; 9] = [
    "text:", "macro:", "builtin:", "cmd:", "sys:", "app:", "kde:", "xdg:", "pause:",
];

impl Action {
    /// The variant `prefix` (one of [`PREFIXES`]) builds from `payload`.
    fn with_prefix(prefix: &str, payload: &str) -> Option<Self> {
        let payload = payload.to_owned();
        Some(match prefix {
            "text:" => Self::Text(payload),
            "macro:" => Self::Macro(payload),
            "builtin:" => Self::BuiltinMacro(payload),
            "cmd:" => Self::Command(payload),
            "sys:" => Self::System(payload),
            "app:" => Self::App(payload),
            "kde:" => Self::KdeShortcut(payload),
            "xdg:" => Self::DesktopEntry(payload),
            "pause:" => Self::Pause(payload),
            _ => return None,
        })
    }

    pub fn parse(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::Swallow;
        };
        // Values are taken verbatim: blanks around a prefix or id make the
        // action invalid rather than being dropped, so every action has one
        // spelling. The text payload keeps all of its whitespace.
        for prefix in PREFIXES {
            if let Some(action) = value
                .strip_prefix(prefix)
                .and_then(|payload| Self::with_prefix(prefix, payload))
            {
                return action;
            }
        }
        if value.is_empty() {
            Self::Native
        } else if unknown_prefix(value) {
            Self::Unknown(value.into())
        } else {
            Self::Keys(super::key_catalog::normalize_chord(value).unwrap_or_else(|| value.into()))
        }
    }

    /// Whether this action requires the in-development setting.
    pub fn development_only(&self) -> bool {
        use super::app_action::AppAction;
        match self {
            Self::Macro(_) | Self::Command(_) | Self::KdeShortcut(_) | Self::DesktopEntry(_) => {
                true
            }
            Self::System(id) => !LEFT_HAND_CONTROL_SYSTEM_ACTIONS.contains(&id.as_str()),
            Self::Keys(keys) => keys.split('+').any(|key| {
                matches!(
                    key,
                    "MouseBack" | "MouseForward" | "Mouse6" | "Mouse7" | "Mouse8"
                )
            }),
            Self::App(id) => matches!(
                AppAction::parse(id),
                Some(AppAction::ShowMenu { .. } | AppAction::GameMode(_))
            ),
            _ => false,
        }
    }

    /// Canonical string; `None` for [`Action::Swallow`].
    pub fn format(&self) -> Option<String> {
        Some(match self {
            Self::Native => String::new(),
            Self::Swallow => return None,
            Self::Keys(keys) => keys.clone(),
            Self::Macro(id) => format!("macro:{id}"),
            Self::BuiltinMacro(id) => format!("builtin:{id}"),
            Self::Command(id) => format!("cmd:{id}"),
            Self::System(id) => format!("sys:{id}"),
            Self::App(id) => format!("app:{id}"),
            Self::KdeShortcut(id) => format!("kde:{id}"),
            Self::DesktopEntry(id) => format!("xdg:{id}"),
            Self::Text(text) => format!("text:{text}"),
            Self::Pause(ms) => format!("pause:{ms}"),
            Self::Unknown(value) => value.clone(),
        })
    }
}

/// `word:…` with a lowercase prefix, the shape of every action type. Key
/// names never contain `:`.
fn unknown_prefix(value: &str) -> bool {
    value.split_once(':').is_some_and(|(prefix, _)| {
        prefix.starts_with(|c: char| c.is_ascii_lowercase())
            && prefix.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

/// How an action should be presented: a translatable message id with an
/// optional number, or a user-provided label that is shown verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionName {
    /// Built-in system action; the id is the `sys:` id without its number.
    System { id: &'static str, n: u32 },
    /// Built-in app action (`showQuickMenu`, `activateKeymap`, …).
    App { id: &'static str, n: u32 },
    /// Built-in macro; its id is translated by the shell.
    BuiltinMacro { id: &'static str },
    /// Name chosen by the user.
    Verbatim(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub action: Action,
    pub name: ActionName,
}

pub const NUMBERED_SYSTEM_ACTIONS: [&str; 4] = [
    "switchToDesktop",
    "switchToKeyboardLayout",
    "moveWindowToDesktop",
    "activateTaskManagerEntry",
];

// System actions available for assignment in the standard mode.
const LEFT_HAND_CONTROL_SYSTEM_ACTIONS: &[&str] = &[
    "activateTaskManagerEntry1",
    "activateTaskManagerEntry10",
    "activateTaskManagerEntry2",
    "activateTaskManagerEntry3",
    "activateTaskManagerEntry4",
    "activateTaskManagerEntry5",
    "activateTaskManagerEntry6",
    "activateTaskManagerEntry7",
    "activateTaskManagerEntry8",
    "activateTaskManagerEntry9",
    "closeWindow",
    "copy",
    "cut",
    "deleteWordBackward",
    "deleteWordForward",
    "keepWindowAbove",
    "maximizeWindow",
    "maximizeWindowHorizontally",
    "maximizeWindowVertically",
    "minimizeWindow",
    "moveToDocumentEnd",
    "moveToDocumentStart",
    "moveWindowToNextDesktop",
    "moveWordLeft",
    "moveWordRight",
    "paste",
    "redo",
    "selectAll",
    "selectWordLeft",
    "selectWordRight",
    "showAppLauncher",
    "showClipboardHistory",
    "showOverview",
    "switchToDesktop1",
    "switchToDesktop2",
    "switchToDesktop3",
    "switchToDesktop4",
    "switchToKeyboardLayout1",
    "switchToKeyboardLayout10",
    "switchToKeyboardLayout2",
    "switchToKeyboardLayout3",
    "switchToKeyboardLayout4",
    "switchToKeyboardLayout5",
    "switchToKeyboardLayout6",
    "switchToKeyboardLayout7",
    "switchToKeyboardLayout8",
    "switchToKeyboardLayout9",
    "switchWindow",
    "tileWindowBottom",
    "tileWindowLeft",
    "tileWindowRight",
    "tileWindowTop",
    "toggleComment",
    "turnOffScreen",
    "undo",
];

#[cfg(test)]
#[test]
fn standard_system_actions_match_left_hand_control() {
    let bundled = super::bundled::BUNDLED
        .iter()
        .find(|keymap| keymap.name == "LHC1_by_Ivan_K")
        .unwrap();
    let assigned: std::collections::BTreeSet<_> = bundled
        .text()
        .split('"')
        .filter_map(|token| token.strip_prefix("sys:"))
        .collect();
    assert_eq!(
        assigned,
        LEFT_HAND_CONTROL_SYSTEM_ACTIONS.iter().copied().collect()
    );
}

/// Standard application shortcuts: editing commands whose keys follow the
/// OS, the keyboard layout and the focused window, and fixed text and
/// window-switching chords (`mapper::shortcuts`). They act on the focused
/// application like a key chord.
pub const SHORTCUT_SYSTEM_ACTIONS: [&str; 31] = [
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
    "toggleComment",
    "moveWordLeft",
    "moveWordRight",
    "selectWordLeft",
    "selectWordRight",
    "deleteWordBackward",
    "deleteWordForward",
    "moveToLineStart",
    "moveToLineEnd",
    "selectToLineStart",
    "selectToLineEnd",
    "moveToDocumentStart",
    "moveToDocumentEnd",
    "switchWindow",
];

/// Desktop actions without a number. On Windows `moveWindowToNextDesktop`
/// and `moveWindowToPreviousDesktop` do not resolve, since like
/// `moveWindowToDesktopN` there is no public API to move another app's
/// window between virtual desktops; on GNOME `showDesktop`, top and bottom
/// tiling and clipboard history have no counterpart. See
/// [`unsupported_on_macos`] for macOS.
pub const PLAIN_SYSTEM_ACTIONS: [&str; 22] = [
    "closeWindow",
    "minimizeWindow",
    "maximizeWindow",
    "maximizeWindowVertically",
    "maximizeWindowHorizontally",
    "tileWindowLeft",
    "tileWindowRight",
    "tileWindowTop",
    "tileWindowBottom",
    "keepWindowAbove",
    "moveWindowToNextDesktop",
    "moveWindowToPreviousDesktop",
    "moveWindowToNextScreen",
    "moveWindowToPreviousScreen",
    "switchToNextDesktop",
    "switchToPreviousDesktop",
    "showDesktop",
    "showOverview",
    "showAppLauncher",
    "showClipboardHistory",
    "turnOffScreen",
    "lockSession",
];

/// Whether `sys:<id>` is a standard application shortcut
/// ([`SHORTCUT_SYSTEM_ACTIONS`]).
pub fn is_shortcut_system_action(id: &str) -> bool {
    SHORTCUT_SYSTEM_ACTIONS.contains(&id)
}

/// `sys:` ids (numbered ones without their number) that macOS has no
/// public API for. Running one there is ignored with a notice.
const MACOS_UNSUPPORTED_SYSTEM_ACTIONS: [&str; 20] = [
    "switchToDesktop",
    "activateTaskManagerEntry",
    "tileWindowLeft",
    "tileWindowRight",
    "tileWindowTop",
    "tileWindowBottom",
    "showClipboardHistory",
    "moveWindowToDesktop",
    "moveWindowToNextDesktop",
    "moveWindowToPreviousDesktop",
    "moveWindowToNextScreen",
    "moveWindowToPreviousScreen",
    "switchToNextDesktop",
    "switchToPreviousDesktop",
    "lockSession",
    "maximizeWindowVertically",
    "maximizeWindowHorizontally",
    "keepWindowAbove",
    "showDesktop",
    "showAppLauncher",
];

/// Numbered actions (`switchToDesktopN`, `showQuickMenuN`,
/// `activateKeymapN`, …) accept any `N` in `1..=MAX_SLOT`; one rule for all
/// families. A slot that resolves to nothing at run time does nothing. The
/// `*_PICKER_COUNT` constants only size the action picker.
///
/// Menu pages and keymaps are addressed by position on purpose, not by id:
/// the user binds keys to slots once and changes what a key opens by
/// reordering pages or library keymaps, which the UI shows. Reordering
/// therefore does not rewrite these actions. Do not switch them to ids.
///
/// `MAX_SLOT` is part of the file format: it may be raised, never lowered.
pub const MAX_SLOT: u32 = 999;
/// Numbered system actions offered in the action picker.
pub const SYSTEM_PICKER_COUNT: u32 = 10;
/// Menu pages offered in the action picker (more if the keymap has more).
pub const MENU_PAGE_PICKER_COUNT: u32 = 10;
/// Keymap slots offered in the action picker (more if the library has more).
pub const KEYMAP_PICKER_COUNT: u32 = 5;
/// How deep macros may reference other macros.
pub const MAX_MACRO_DEPTH: usize = 10;
/// Most steps a macro may expand to, nested macros included.
pub const MAX_MACRO_STEPS: usize = 1000;

/// Numbered `app:` action families.
pub const NUMBERED_APP_ACTIONS: [&str; 3] = ["showQuickMenu", "showEmojiMenu", "activateKeymap"];

/// The number of a numbered id: decimal digits without a sign or leading
/// zero (`3`, not `03` or `+3`), so every number has one spelling, in
/// `1..=MAX_SLOT`.
pub fn slot_number(digits: &str) -> Option<u32> {
    if digits.is_empty()
        || digits.len() > 3
        || digits.starts_with('0')
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    digits.parse().ok()
}

/// Split `switchToDesktop3` into its catalog name and number.
fn numbered(id: &str, names: &[&'static str]) -> Option<(&'static str, u32)> {
    names.iter().find_map(|name| {
        let n = slot_number(id.strip_prefix(name)?)?;
        (n <= MAX_SLOT).then_some((*name, n))
    })
}

pub fn system_action_name(id: &str) -> Option<ActionName> {
    numbered(id, &NUMBERED_SYSTEM_ACTIONS)
        .or_else(|| {
            SHORTCUT_SYSTEM_ACTIONS
                .iter()
                .chain(&PLAIN_SYSTEM_ACTIONS)
                .find(|known| **known == id)
                .map(|known| (*known, 0))
        })
        .map(|(name, n)| ActionName::System { id: name, n })
}

/// Whether the `sys:` action `id` cannot run on macOS.
pub fn unsupported_on_macos(id: &str) -> bool {
    matches!(system_action_name(id), Some(ActionName::System { id, .. })
        if MACOS_UNSUPPORTED_SYSTEM_ACTIONS.contains(&id))
}

/// Split a `kde:` id into its kglobalaccel component and shortcut name.
/// The component is the last segment of its D-Bus object path, so it may
/// only hold the characters such a segment allows.
pub fn kde_shortcut_parts(id: &str) -> Option<(&str, &str)> {
    let (component, shortcut) = id.split_once('/')?;
    let valid_component = !component.is_empty()
        && component
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    (valid_component && !shortcut.is_empty() && shortcut.trim() == shortcut)
        .then_some((component, shortcut))
}

/// Split an `xdg:` id into its desktop-file id (without `.desktop`) and
/// action id. Desktop ids may hold `.`, `-` and `_`; action ids are the
/// names listed in the entry's `Actions=` key.
pub fn desktop_entry_parts(id: &str) -> Option<(&str, &str)> {
    let (app, action) = id.split_once('/')?;
    let valid = |part: &str, extra: &[char]| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
    };
    (valid(app, &['.', '-', '_']) && valid(action, &['-', '_'])).then_some((app, action))
}

/// Game-mode control actions: flip the effective state, or set the mode.
pub const GAME_MODE_APP_ACTIONS: [&str; 4] = [
    "toggleGameMode",
    "setGameModeAuto",
    "setGameModeOn",
    "setGameModeOff",
];

/// App actions without a number: the menus on their last used page and
/// the settings window.
pub const WINDOW_APP_ACTIONS: [&str; 3] =
    ["showQuickMenuRecent", "showEmojiMenuRecent", "showSettings"];

/// Actions of the built-in popup hotkeys Ctrl+Alt+F11 and Ctrl+Alt+F12.
pub const SHOW_EMOJI_MENU_RECENT: &str = "app:showEmojiMenuRecent";
pub const SHOW_QUICK_MENU_RECENT: &str = "app:showQuickMenuRecent";

pub fn app_action_name(id: &str) -> Option<ActionName> {
    numbered(id, &NUMBERED_APP_ACTIONS)
        .map(|(id, n)| ActionName::App { id, n })
        .or_else(|| {
            WINDOW_APP_ACTIONS
                .iter()
                .chain(&GAME_MODE_APP_ACTIONS)
                .find(|known| **known == id)
                .map(|known| ActionName::App { id: known, n: 0 })
        })
}

/// Page of an `app:` menu action, e.g. `showEmojiMenu2` → (Emoji, 2).
pub fn menu_page(app_id: &str) -> Option<(bool, u32)> {
    match app_action_name(app_id)? {
        ActionName::App {
            id: id @ ("showQuickMenu" | "showEmojiMenu"),
            n,
        } => Some((id == "showEmojiMenu", n)),
        _ => None,
    }
}

pub fn system_actions() -> Vec<CatalogEntry> {
    SHORTCUT_SYSTEM_ACTIONS
        .into_iter()
        .chain(PLAIN_SYSTEM_ACTIONS)
        .map(|id| CatalogEntry {
            action: Action::System(id.into()),
            name: ActionName::System { id, n: 0 },
        })
        .chain(NUMBERED_SYSTEM_ACTIONS.into_iter().flat_map(|name| {
            (1..=SYSTEM_PICKER_COUNT).map(move |n| CatalogEntry {
                action: Action::System(format!("{name}{n}")),
                name: ActionName::System { id: name, n },
            })
        }))
        .collect()
}

/// App actions with the default picker ranges.
pub fn app_actions() -> Vec<CatalogEntry> {
    app_actions_up_to(0, 0, 0)
}

/// App actions whose numbered ranges grow past the picker defaults to
/// cover `quick_pages`, `emoji_pages` and `keymaps` existing slots.
fn app_actions_up_to(quick_pages: usize, emoji_pages: usize, keymaps: usize) -> Vec<CatalogEntry> {
    let plain = |id: &'static str| CatalogEntry {
        action: Action::App(id.into()),
        name: ActionName::App { id, n: 0 },
    };
    let numbered = |id: &'static str, n: u32| CatalogEntry {
        action: Action::App(format!("{id}{n}")),
        name: ActionName::App { id, n },
    };
    let range = |default: u32, count: usize| {
        1..=u32::try_from(count)
            .unwrap_or(MAX_SLOT)
            .clamp(default, MAX_SLOT)
    };
    let quick = range(MENU_PAGE_PICKER_COUNT, quick_pages);
    let emoji = range(MENU_PAGE_PICKER_COUNT, emoji_pages);
    let pages = 1..=(*quick.end()).max(*emoji.end());
    WINDOW_APP_ACTIONS
        .into_iter()
        .map(plain)
        .chain(pages.flat_map(move |n| {
            [("showQuickMenu", &quick), ("showEmojiMenu", &emoji)]
                .into_iter()
                .filter(|(_, range)| range.contains(&n))
                .map(|(id, _)| numbered(id, n))
                .collect::<Vec<_>>()
        }))
        .chain(range(KEYMAP_PICKER_COUNT, keymaps).map(|n| numbered("activateKeymap", n)))
        .chain(GAME_MODE_APP_ACTIONS.map(plain))
        .collect()
}

/// Every action that can be picked for `config`: user macros and
/// commands, built-in macros, system and app actions.
pub fn catalog(config: &ResolvedAppConfig) -> Vec<CatalogEntry> {
    let mut out: Vec<CatalogEntry> = config
        .macros
        .iter()
        .map(|item| CatalogEntry {
            action: Action::Macro(item.id.clone()),
            name: ActionName::Verbatim(item.name.clone()),
        })
        .collect();
    out.extend(BUILTIN_MACROS.iter().map(|builtin| CatalogEntry {
        action: Action::BuiltinMacro(builtin.id.into()),
        name: ActionName::BuiltinMacro { id: builtin.id },
    }));
    out.extend(config.commands.iter().map(|command| CatalogEntry {
        action: Action::Command(command.id.clone()),
        name: ActionName::Verbatim(if command.name.is_empty() {
            command.posix_script.clone()
        } else {
            command.name.clone()
        }),
    }));
    out.extend(system_actions());
    out.extend(app_actions_up_to(
        config.quick_action_pages.len(),
        config.emoji_pages.len(),
        config.settings.keymap_order.len(),
    ));
    out
}

/// Why an action cannot be used, following `validateActionValue()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionIssue {
    InvalidSyntax,
    PauseOutsideMacro,
    UnknownMacro,
    UnknownCommand,
    UnknownSystemAction,
    UnknownAppAction,
    /// An action type this build does not know (see [`Action::Unknown`]).
    UnknownActionType,
}

/// Check an action outside a macro. Key names are checked by the mapper
/// when it starts; here only the chord shape is validated.
pub fn validate(action: &Action, config: &ResolvedAppConfig) -> Option<ActionIssue> {
    match action {
        Action::Native | Action::Swallow | Action::Text(_) => None,
        Action::Keys(keys) => (!valid_held_key(keys)).then_some(ActionIssue::InvalidSyntax),
        Action::Pause(_) => Some(ActionIssue::PauseOutsideMacro),
        Action::Unknown(_) => Some(ActionIssue::UnknownActionType),
        Action::Macro(id) => {
            let known = config.macros.iter().any(|item| &item.id == id);
            (!known).then_some(ActionIssue::UnknownMacro)
        }
        Action::BuiltinMacro(id) => {
            (!BUILTIN_MACROS.iter().any(|item| item.id == id)).then_some(ActionIssue::UnknownMacro)
        }
        Action::Command(id) => (!config.commands.iter().any(|command| &command.id == id))
            .then_some(ActionIssue::UnknownCommand),
        Action::System(id) => system_action_name(id)
            .is_none()
            .then_some(ActionIssue::UnknownSystemAction),
        Action::App(id) => app_action_name(id)
            .is_none()
            .then_some(ActionIssue::UnknownAppAction),
        Action::KdeShortcut(id) => kde_shortcut_parts(id)
            .is_none()
            .then_some(ActionIssue::InvalidSyntax),
        Action::DesktopEntry(id) => desktop_entry_parts(id)
            .is_none()
            .then_some(ActionIssue::InvalidSyntax),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionIssue {
    CommandsDisabled,
    Unavailable,
}

pub fn execution_issue(action: &Action, config: &ResolvedAppConfig) -> Option<ExecutionIssue> {
    fn check(action: &Action, config: &ResolvedAppConfig, depth: usize) -> Option<ExecutionIssue> {
        if depth > MAX_MACRO_DEPTH {
            return Some(ExecutionIssue::Unavailable);
        }
        match action {
            Action::Command(id) => {
                if !crate::mapper::runtime::SUPPORTED {
                    Some(ExecutionIssue::Unavailable)
                } else if !config.settings.commands_enabled() {
                    Some(ExecutionIssue::CommandsDisabled)
                } else if config
                    .commands
                    .iter()
                    .find(|command| &command.id == id)
                    .is_some_and(|command| command.script().trim().is_empty())
                {
                    // Only a script for another OS: the mapper skips it here.
                    Some(ExecutionIssue::Unavailable)
                } else {
                    None
                }
            }
            Action::System(id) => {
                #[cfg(any(target_os = "linux", target_os = "windows"))]
                if crate::mapper::system::resolve(id).is_some() {
                    return None;
                }
                #[cfg(not(any(target_os = "linux", target_os = "windows")))]
                let _ = id;
                Some(ExecutionIssue::Unavailable)
            }
            Action::KdeShortcut(id) => {
                #[cfg(target_os = "linux")]
                if crate::mapper::system::resolve_kde_shortcut(id).is_some() {
                    return None;
                }
                #[cfg(not(target_os = "linux"))]
                let _ = id;
                Some(ExecutionIssue::Unavailable)
            }
            Action::DesktopEntry(id) => {
                #[cfg(target_os = "linux")]
                if crate::mapper::system::resolve_desktop_entry(id).is_some() {
                    return None;
                }
                #[cfg(not(target_os = "linux"))]
                let _ = id;
                Some(ExecutionIssue::Unavailable)
            }
            Action::Macro(_) | Action::BuiltinMacro(_) => {
                let steps: Vec<&str> = match action {
                    Action::Macro(id) => config
                        .macros
                        .iter()
                        .find(|item| item.id == *id)
                        .map(|item| item.steps.iter().map(|step| step.action.as_str()).collect())
                        .unwrap_or_default(),
                    Action::BuiltinMacro(id) => BUILTIN_MACROS
                        .iter()
                        .find(|item| item.id == id)
                        .map(|item| item.steps.to_vec())
                        .unwrap_or_default(),
                    _ => unreachable!(),
                };
                steps
                    .into_iter()
                    .find_map(|value| check(&Action::parse(Some(value)), config, depth + 1))
            }
            _ => None,
        }
    }
    check(action, config, 0)
}

pub fn valid_held_key(value: &str) -> bool {
    let normalized = super::key_catalog::normalize_chord(value);
    let value = normalized.as_deref().unwrap_or(value);
    #[cfg(target_os = "linux")]
    {
        crate::mapper::action::parse_action(value).is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut tokens = value.split('+').map(str::trim).collect::<Vec<_>>();
        let Some(key) = tokens.pop() else {
            return false;
        };
        super::key_catalog::valid_key(key)
            && tokens
                .iter()
                .all(|modifier| super::key_catalog::modifier_key(modifier).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{AppSettings, KeymapPreset};

    #[test]
    fn key_names_normalize_without_changing_payloads() {
        assert_eq!(
            Action::parse(Some("shift+slash")).format().as_deref(),
            Some("Shift+Slash")
        );
        for raw in ["text:MiXeD ", "macro:MiXeD", "cmd:MiXeD"] {
            assert_eq!(Action::parse(Some(raw)).format().as_deref(), Some(raw));
        }
        assert!(valid_held_key("shiftright+slash"));
    }

    #[test]
    fn parse_and_format_round_trip() {
        for raw in [
            "KeyA",
            "Ctrl+KeyC",
            "macro:custom",
            "builtin:copyLineText",
            "cmd:music",
            "sys:switchToDesktop2",
            "app:showQuickMenu1",
            "kde:kwin/Window Quick Tile Top",
            "xdg:org.mozilla.firefox/new-private-window",
            "text:  keep spaces ",
            "text:   ",
            "text:first\nsecond\n",
            "pause:100",
            "future:thing",
            "",
        ] {
            assert_eq!(Action::parse(Some(raw)).format().as_deref(), Some(raw));
        }
        assert_eq!(Action::parse(None), Action::Swallow);
        assert_eq!(Action::Swallow.format(), None);
        // Values are verbatim: blanks around a prefix or id are not dropped,
        // so the action is invalid; the text payload keeps its whitespace.
        assert_eq!(
            Action::parse(Some("text: hi ")),
            Action::Text(" hi ".into())
        );
        assert!(matches!(Action::parse(Some(" text:hi")), Action::Keys(_)));
        assert_eq!(Action::parse(Some("macro: a")), Action::Macro(" a".into()));
        assert_eq!(system_action_name("switchToDesktop03"), None);
        assert_eq!(system_action_name("switchToDesktop+3"), None);
        assert!(system_action_name("switchToDesktop3").is_some());
        assert_eq!(app_action_name("showQuickMenu01"), None);
    }

    #[test]
    fn desktop_entry_ids_need_an_app_and_an_action() {
        assert_eq!(
            desktop_entry_parts("org.mozilla.firefox/new-private-window"),
            Some(("org.mozilla.firefox", "new-private-window"))
        );
        for bad in ["firefox", "/x", "firefox/", "a b/c", "a/b c", "a/b/c"] {
            assert_eq!(desktop_entry_parts(bad), None, "{bad}");
        }
    }

    #[test]
    fn unknown_action_types_are_kept_and_reported() {
        let config =
            ResolvedAppConfig::from_parts(AppSettings::default(), KeymapPreset::default(), None);
        assert_eq!(
            Action::parse(Some("newKind:some value")),
            Action::Unknown("newKind:some value".into())
        );
        assert_eq!(
            validate(&Action::parse(Some("newKind:x")), &config),
            Some(ActionIssue::UnknownActionType)
        );
        // Not the shape of an action type: still a (bad) key chord.
        for raw in ["Ctrl+a:b", "Key:A", ":x"] {
            assert_eq!(Action::parse(Some(raw)), Action::Keys(raw.into()));
        }
    }

    #[test]
    fn catalogs_preserve_builtin_action_ids() {
        assert_eq!(system_actions().len(), 93);
        assert_eq!(app_actions().len(), 32);
        assert_eq!(
            system_action_name("switchToDesktop10"),
            Some(ActionName::System {
                id: "switchToDesktop",
                n: 10
            })
        );
        assert_eq!(
            system_action_name("switchToDesktop11"),
            Some(ActionName::System {
                id: "switchToDesktop",
                n: 11
            })
        );
        assert!(system_action_name("switchToDesktop999").is_some());
        assert_eq!(system_action_name("switchToDesktop1000"), None);
        assert_eq!(system_action_name("lockScreen"), None);
        assert_eq!(
            system_action_name("turnOffScreen"),
            Some(ActionName::System {
                id: "turnOffScreen",
                n: 0
            })
        );
        assert!(unsupported_on_macos("switchToDesktop3"));
        assert!(unsupported_on_macos("keepWindowAbove"));
        assert!(unsupported_on_macos("maximizeWindowVertically"));
        assert!(unsupported_on_macos("moveWindowToNextDesktop"));
        assert!(!unsupported_on_macos("switchToKeyboardLayout2"));
        assert!(!unsupported_on_macos("closeWindow"));
        assert!(unsupported_on_macos("lockSession"));
        assert_eq!(
            kde_shortcut_parts("org_kde_powerdevil/Turn Off Screen"),
            Some(("org_kde_powerdevil", "Turn Off Screen"))
        );
        assert_eq!(kde_shortcut_parts("kwin"), None);
        assert_eq!(kde_shortcut_parts("kwin/ "), None);
        assert_eq!(kde_shortcut_parts("org.kde/Overview"), None);
        assert_eq!(menu_page("showEmojiMenu3"), Some((true, 3)));
        assert_eq!(menu_page("showQuickMenu42"), Some((false, 42)));
        assert_eq!(menu_page("showQuickMenu0"), None);
        assert_eq!(menu_page("toggleGameMode"), None);
        assert_eq!(menu_page("activateKeymap2"), None);
        assert_eq!(menu_page("showEmojiMenuRecent"), None);
        assert_eq!(
            app_action_name("setGameModeOff"),
            Some(ActionName::App {
                id: "setGameModeOff",
                n: 0
            })
        );
        assert_eq!(
            app_action_name("activateKeymap5"),
            Some(ActionName::App {
                id: "activateKeymap",
                n: 5
            })
        );
        assert!(app_action_name("activateKeymap6").is_some());
        assert!(app_action_name("showQuickMenu999").is_some());
        assert_eq!(app_action_name("activateKeymap1000"), None);
        assert_eq!(app_action_name("showEmojiMenu1000"), None);
        assert!(app_action_name("showQuickMenuRecent").is_some());
        assert!(app_action_name("showSettings").is_some());
    }

    #[test]
    fn builtin_macro_labels_are_typed_and_user_names_stay_verbatim() {
        let mut config =
            ResolvedAppConfig::from_parts(AppSettings::default(), KeymapPreset::initial(), None);
        config.macros.push(super::super::model::Macro {
            id: "copyLine".into(),
            name: "Copy line".into(),
            steps: vec![],
            step_pause_ms: None,
            modifier_delay_ms: None,
        });
        let entries = catalog(&config);
        for builtin in BUILTIN_MACROS {
            assert!(entries.iter().any(|entry| entry.action
                == Action::BuiltinMacro(builtin.id.into())
                && entry.name == ActionName::BuiltinMacro { id: builtin.id }));
        }
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == Action::Macro("copyLine".into())
                    && entry.name == ActionName::Verbatim("Copy line".into()))
        );
    }

    #[test]
    fn execution_issues_follow_what_this_os_runs() {
        let mut config =
            ResolvedAppConfig::from_parts(AppSettings::default(), KeymapPreset::default(), None);
        let other_os_only = if cfg!(target_os = "windows") {
            ("true", "")
        } else {
            ("", "dir")
        };
        for (id, (linux, windows)) in [("both", ("true", "dir")), ("foreign", other_os_only)] {
            config.commands.push(crate::config::model::Command {
                id: id.into(),
                name: String::new(),
                posix_script: linux.into(),
                powershell_script: windows.into(),
                working_directory: None,
            });
        }
        let issue = |config: &ResolvedAppConfig, raw: &str| {
            execution_issue(&Action::parse(Some(raw)), config)
        };

        config.settings.set_commands_enabled(false);
        let disabled = if crate::mapper::runtime::SUPPORTED {
            Some(ExecutionIssue::CommandsDisabled)
        } else {
            Some(ExecutionIssue::Unavailable)
        };
        assert_eq!(issue(&config, "cmd:both"), disabled);

        config.settings.set_commands_enabled(true);
        let runnable = (!crate::mapper::runtime::SUPPORTED).then_some(ExecutionIssue::Unavailable);
        assert_eq!(issue(&config, "cmd:both"), runnable);
        assert_eq!(
            issue(&config, "cmd:foreign"),
            Some(ExecutionIssue::Unavailable)
        );
        // Windows runs system actions through its own backend.
        if cfg!(target_os = "windows") {
            assert_eq!(issue(&config, "sys:closeWindow"), None);
        }
        if !crate::mapper::runtime::SUPPORTED {
            assert_eq!(
                issue(&config, "sys:closeWindow"),
                Some(ExecutionIssue::Unavailable)
            );
        }
        assert_eq!(
            issue(&config, "sys:switchToDesktop1000"),
            Some(ExecutionIssue::Unavailable)
        );
    }

    #[test]
    fn validation_checks_references() {
        let config =
            ResolvedAppConfig::from_parts(AppSettings::default(), KeymapPreset::initial(), None);
        let check = |raw: &str| validate(&Action::parse(Some(raw)), &config);
        assert_eq!(check("Ctrl+KeyC"), None);
        assert_eq!(check("Ctrl+"), Some(ActionIssue::InvalidSyntax));
        assert_eq!(check("NotAKey"), Some(ActionIssue::InvalidSyntax));
        assert_eq!(check("KeyA+KeyB"), Some(ActionIssue::InvalidSyntax));
        assert_eq!(check("builtin:copyLineText"), None);
        assert_eq!(check("builtin:copyLine"), Some(ActionIssue::UnknownMacro));
        assert_eq!(check("macro:copyLine"), Some(ActionIssue::UnknownMacro));
        assert_eq!(check("builtin:nope"), Some(ActionIssue::UnknownMacro));
        assert_eq!(check("macro:nope"), Some(ActionIssue::UnknownMacro));
        assert_eq!(check("cmd:nope"), Some(ActionIssue::UnknownCommand));
        assert_eq!(check("sys:moveWindowToDesktop2"), None);
        assert_eq!(check("sys:lockSession"), None);
        assert_eq!(
            check("sys:lockScreen"),
            Some(ActionIssue::UnknownSystemAction)
        );
        assert_eq!(check("xdg:org.kde.konsole/NewWindow"), None);
        assert_eq!(check("xdg:konsole"), Some(ActionIssue::InvalidSyntax));
        assert_eq!(check("kde:ksmserver/Lock Session"), None);
        assert_eq!(check("kde:Lock Session"), Some(ActionIssue::InvalidSyntax));
        assert_eq!(check("app:showEmojiMenu42"), None);
        assert_eq!(
            check("app:showEmojiMenu0"),
            Some(ActionIssue::UnknownAppAction)
        );
        assert_eq!(check("pause:10"), Some(ActionIssue::PauseOutsideMacro));
    }
}
