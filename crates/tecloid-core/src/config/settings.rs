//! Versioned application settings. Editor working copies stay in memory.

use super::keymap_file::WhenDoc;
use super::model::{
    AccessibilityChoice, AppSettings, Appearance, AutoRule, GameModeControl,
    GameModeProcessMatcher, GameModeSettings, KeymapMode, LocalePreference, PasteShortcut,
    TapDecision, TextMode,
};
use super::schema::{self, CONFIG_SCHEMA_VERSION};
use crate::storage::CONFIG_FILE;
use serde::{Deserialize, Serialize};

/// Parse `config.json`; an empty file gives the defaults.
pub fn parse(text: &str) -> Result<AppSettings, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.trim().is_empty() {
        return Ok(AppSettings::default());
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("{CONFIG_FILE}: {error}"))?;
    let settings = match schema::read_version(&value, CONFIG_FILE, CONFIG_SCHEMA_VERSION)? {
        1 => from_doc(
            serde_json::from_str::<ConfigDoc>(text)
                .map_err(|error| schema::read_error(CONFIG_FILE, &error))?,
        )?,
        version => {
            return Err(format!(
                "{CONFIG_FILE}: no reader for schema version {version}"
            ));
        }
    };
    validate(&settings)?;
    Ok(settings)
}

/// JSON Schema of `config.json`, published under `schemas/`.
#[cfg(test)]
pub(crate) fn json_schema() -> schemars::Schema {
    schemars::schema_for!(ConfigDoc)
}

/// Every setting is written, defaults included, so a later change of a
/// default never changes the behaviour of an existing installation.
pub fn serialize(settings: &AppSettings) -> String {
    serde_json::to_string_pretty(&to_doc(settings)).unwrap_or_default()
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigDoc {
    #[cfg_attr(test, schemars(extend("const" = CONFIG_SCHEMA_VERSION)))]
    version: u64,
    #[serde(
        rename = "$schema",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "String"))]
    schema: Option<String>,
    #[serde(default)]
    #[cfg_attr(test, schemars(default = "schema_settings_default"))]
    settings: SettingsDoc,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct SettingsDoc {
    /// Shows interface features that are still in development.
    experimental: bool,
    launch_at_login: bool,
    /// Start the mapper when the app launches.
    start_mapper_on_launch: bool,
    interface: InterfaceDoc,
    devices: DevicesDoc,
    timing: TimingDoc,
    keymaps: KeymapsDoc,
    commands_enabled: bool,
    #[cfg_attr(test, schemars(default = "schema_game_mode_default"))]
    game_mode: GameModeDoc,
    linux: LinuxDoc,
}

impl Default for SettingsDoc {
    fn default() -> Self {
        to_doc(&AppSettings::default()).settings
    }
}

/// Look of the settings window and on-screen notices.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct InterfaceDoc {
    appearance: Appearance,
    high_contrast: AccessibilityChoice,
    reduce_motion: AccessibilityChoice,
    locale: LocalePreference,
    /// On-screen notice when the active keymap changes.
    keymap_osd: bool,
    /// While a layer is held, an overlay with its keys.
    layer_hint: bool,
}

impl Default for InterfaceDoc {
    fn default() -> Self {
        to_doc(&AppSettings::default()).settings.interface
    }
}

/// Devices the mapper grabs, as stable paths such as `/dev/input/by-id/…`
/// on Linux. Lists leave room for several devices; this version grabs at
/// most one of each kind.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct DevicesDoc {
    keyboards: Vec<String>,
    mice: Vec<String>,
}

impl DevicesDoc {
    /// The single device of `list`, checked.
    fn single(list: Vec<String>, what: &str) -> Result<Option<String>, String> {
        if list.len() > 1 {
            return Err(format!(
                "settings.devices.{what}: this version supports at most one device"
            ));
        }
        match list.into_iter().next() {
            Some(path) if path.trim().is_empty() => Err(format!(
                "settings.devices.{what}: device path must not be empty"
            )),
            path => Ok(path),
        }
    }
}

/// Which library keymap the mapper uses.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct KeymapsDoc {
    mode: KeymapMode,
    /// Keymap used in manual mode; missing: none chosen yet.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "String"))]
    manual_keymap: Option<String>,
    /// Library order: the slots of `activateKeymapN` and the order of the
    /// list. Behaviour, not remembered state, so it lives in settings.
    order: Vec<String>,
    auto: AutoDoc,
}

impl Default for KeymapsDoc {
    fn default() -> Self {
        to_doc(&AppSettings::default()).settings.keymaps
    }
}

/// Automatic mode: the first matching rule wins, else `fallback`.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct AutoDoc {
    rules: Vec<AutoRuleDoc>,
    /// Keymap used when no rule matches; `null` turns the mapper off.
    fallback: Option<String>,
}

impl Default for AutoDoc {
    fn default() -> Self {
        to_doc(&AppSettings::default()).settings.keymaps.auto
    }
}

/// Defaults for rules and macros; a rule or macro may override each.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct TimingDoc {
    #[cfg_attr(test, schemars(range(min = super::timing::TAP_HOLD.min, max = super::timing::TAP_HOLD.max)))]
    hold_ms: u64,
    #[cfg_attr(test, schemars(range(min = super::timing::LONG_HOLD.min, max = super::timing::LONG_HOLD.max)))]
    long_hold_ms: u64,
    #[cfg_attr(test, schemars(range(min = super::timing::TAP_HOLD.min, max = super::timing::TAP_HOLD.max)))]
    double_tap_ms: u64,
    /// Window in which all keys of a multi-key rule must be pressed.
    #[cfg_attr(test, schemars(range(min = super::timing::TRIGGER_CHORD.min, max = super::timing::TRIGGER_CHORD.max)))]
    chord_ms: u64,
    tap_decision: TapDecision,
    #[cfg_attr(test, schemars(range(min = super::timing::MACRO_DELAY.min, max = super::timing::MACRO_DELAY.max)))]
    macro_step_pause_ms: u64,
    #[cfg_attr(test, schemars(range(min = super::timing::MACRO_DELAY.min, max = super::timing::MACRO_DELAY.max)))]
    macro_modifier_delay_ms: u64,
    /// A macro run still going after this time is aborted.
    #[cfg_attr(test, schemars(range(min = super::timing::MACRO_TIMEOUT.min, max = super::timing::MACRO_TIMEOUT.max)))]
    macro_timeout_ms: u64,
}

impl Default for TimingDoc {
    fn default() -> Self {
        to_doc(&AppSettings::default()).settings.timing
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AutoRuleDoc {
    /// Required; an explicit `null` turns the mapper off, so a forgotten
    /// field is an error rather than a silent "off".
    #[serde(deserialize_with = "nullable")]
    keymap: Option<String>,
    #[serde(default = "yes")]
    enabled: bool,
    /// Empty conditions always match; no conditions are not written.
    #[serde(default, skip_serializing_if = "WhenDoc::is_empty")]
    when: WhenDoc,
}

/// `linux.terminalPasteShortcut`: a paste chord, or `"sameAsPaste"` (`None`)
/// to use `linux.pasteShortcut`.
#[derive(PartialEq)]
struct TerminalPasteDoc(Option<PasteShortcut>);

const SAME_AS_PASTE: &str = "sameAsPaste";

impl Serialize for TerminalPasteDoc {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Some(shortcut) => shortcut.serialize(serializer),
            None => serializer.serialize_str(SAME_AS_PASTE),
        }
    }
}

impl<'de> Deserialize<'de> for TerminalPasteDoc {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value == SAME_AS_PASTE {
            return Ok(Self(None));
        }
        PasteShortcut::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&value))
            .map(|shortcut| Self(Some(shortcut)))
    }
}

#[cfg(test)]
impl schemars::JsonSchema for TerminalPasteDoc {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TerminalPasteShortcut".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut values: Vec<&str> = PasteShortcut::ALL.map(PasteShortcut::as_str).into();
        values.push(SAME_AS_PASTE);
        schemars::json_schema!({ "type": "string", "enum": values })
    }
}

/// A required field that may be `null`.
fn nullable<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct GameModeDoc {
    control: GameModeControl,
    #[cfg_attr(test, schemars(default = "schema_detectors_default"))]
    detectors: DetectorsDoc,
    /// Games a launcher or the system reports count only while their
    /// window is active.
    foreground_only: bool,
    processes: Vec<ProcessDoc>,
}

impl Default for GameModeDoc {
    fn default() -> Self {
        to_doc(&AppSettings::default()).settings.game_mode
    }
}

/// Game detection sources; each platform uses only the ones it supports.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct DetectorsDoc {
    #[cfg_attr(test, schemars(default = "yes"))]
    gamemoded: bool,
    fullscreen: bool,
    steam: bool,
    #[cfg_attr(test, schemars(default = "yes"))]
    lutris: bool,
    #[cfg_attr(test, schemars(default = "yes"))]
    heroic: bool,
    #[cfg_attr(test, schemars(default = "yes"))]
    umu: bool,
    #[cfg_attr(test, schemars(default = "yes"))]
    gamescope: bool,
    app_category: bool,
    game_bar: bool,
}

impl Default for DetectorsDoc {
    fn default() -> Self {
        to_doc(&AppSettings::default()).settings.game_mode.detectors
    }
}

// Published schema annotations use Linux defaults on every build host.
// Runtime defaults still follow the target platform.
#[cfg(test)]
fn schema_detectors_default() -> DetectorsDoc {
    DetectorsDoc {
        gamemoded: true,
        lutris: true,
        heroic: true,
        umu: true,
        gamescope: true,
        ..DetectorsDoc::default()
    }
}

#[cfg(test)]
fn schema_game_mode_default() -> GameModeDoc {
    GameModeDoc {
        detectors: schema_detectors_default(),
        ..GameModeDoc::default()
    }
}

#[cfg(test)]
fn schema_settings_default() -> SettingsDoc {
    SettingsDoc {
        game_mode: schema_game_mode_default(),
        ..SettingsDoc::default()
    }
}

/// A process that marks game mode, or with `exclude` one that never does.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProcessDoc {
    name: String,
    #[serde(default = "yes")]
    foreground_only: bool,
    #[serde(default)]
    exclude: bool,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct LinuxDoc {
    /// How `text:` actions reach the focused application.
    text_mode: TextMode,
    /// On-screen notice when Tecloid sends a copy shortcut.
    copy_osd: bool,
    /// Shortcut the clipboard text modes send to paste, as a key chord.
    paste_shortcut: PasteShortcut,
    /// Paste shortcut for terminal windows, or `sameAsPaste`.
    terminal_paste_shortcut: TerminalPasteDoc,
    /// Missing: looked up in `PATH`. An empty string is an error, so
    /// "no value" has one spelling.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "program_path"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    ydotool_path: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "program_path"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    xdotool_path: Option<String>,
}

/// A program path: absent or a non-blank string; `null` and `""` are errors.
fn program_path<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let path = String::deserialize(deserializer)?;
    if path.trim().is_empty() {
        return Err(serde::de::Error::custom(
            "program path must not be empty; omit the field to search PATH",
        ));
    }
    Ok(Some(path))
}

impl Default for LinuxDoc {
    fn default() -> Self {
        to_doc(&AppSettings::default()).settings.linux
    }
}

fn yes() -> bool {
    true
}

/// A keymap reference: `null` or `user:<name>` with a valid library name
/// in its stored spelling. Other prefixes are not keymap ids.
fn keymap_ref(value: Option<String>, what: &str) -> Result<Option<String>, String> {
    let Some(id) = value else {
        return Ok(None);
    };
    let valid = super::model::user_keymap_name(&id)
        .is_some_and(|name| crate::storage::validate_keymap_name(name).as_deref() == Ok(name));
    if valid {
        Ok(Some(id))
    } else {
        Err(format!(
            "{what}: {id:?} is not a keymap id (expected \"user:<keymap name>\")"
        ))
    }
}

/// `settings.keymaps.order`: keymap ids, each listed once. Library names
/// are unique ignoring case (one file on a case-insensitive file system),
/// so `user:Main` and `user:main` are the same entry.
fn keymap_order(order: Vec<String>) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::with_capacity(order.len());
    for (index, id) in order.into_iter().enumerate() {
        let what = format!("settings.keymaps.order[{index}]");
        let id = keymap_ref(Some(id), &what)?.unwrap_or_default();
        if out
            .iter()
            .any(|listed| listed.to_lowercase() == id.to_lowercase())
        {
            return Err(format!("{what}: {id:?} is listed twice"));
        }
        out.push(id);
    }
    Ok(out)
}

/// A game-mode process pattern: an app or process name, not blank and
/// not a window-title pattern (a process has no title).
fn process_name(name: &str, index: usize) -> Result<String, String> {
    let what = format!("settings.gameMode.processes[{index}].name");
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("{what}: must not be empty"));
    }
    if super::app_match::title_text(name).is_some() {
        return Err(format!("{what}: a process has no window title"));
    }
    Ok(name.into())
}

pub fn validate(settings: &AppSettings) -> Result<(), String> {
    super::ids::unique(
        settings.auto_rules.iter().map(|item| item.id.as_str()),
        "settings.keymaps.auto.rules",
    )?;
    super::ids::unique(
        settings
            .game_mode
            .process_matchers
            .iter()
            .map(|item| item.id.as_str()),
        "settings.gameMode.processes",
    )?;
    super::timing::validate_settings(settings)
}

/// Auto rules and game-mode processes are not referenced from anywhere, so
/// the file stores none of their ids; the editor's ids are assigned on load.
fn from_doc(doc: ConfigDoc) -> Result<AppSettings, String> {
    let ConfigDoc { settings, .. } = doc;
    let interface = settings.interface;
    let timing = settings.timing;
    let detectors = settings.game_mode.detectors;
    let keyboard_device = DevicesDoc::single(settings.devices.keyboards, "keyboards")?;
    let mouse_device = DevicesDoc::single(settings.devices.mice, "mice")?;
    let keymaps = settings.keymaps;
    let mut auto_rules = Vec::new();
    for (index, rule) in keymaps.auto.rules.into_iter().enumerate() {
        auto_rules.push(AutoRule {
            id: format!("rule-{}", index + 1),
            keymap_id: keymap_ref(
                rule.keymap,
                &format!("settings.keymaps.auto.rules[{index}]"),
            )?,
            enabled: rule.enabled,
            conditions: rule.when.into_set()?.normalized(),
        });
    }
    Ok(AppSettings {
        schema: doc.schema,
        in_development: settings.experimental,
        launch_on_startup: settings.launch_at_login,
        start_mapper_on_launch: settings.start_mapper_on_launch,
        appearance: interface.appearance,
        high_contrast: interface.high_contrast,
        reduce_motion: interface.reduce_motion,
        keymap_osd: interface.keymap_osd,
        copy_osd: settings.linux.copy_osd,
        layer_hint: interface.layer_hint,
        locale: interface.locale,
        default_hold_timeout_ms: timing.hold_ms,
        default_long_hold_timeout_ms: timing.long_hold_ms,
        tap_decision: timing.tap_decision,
        default_double_tap_timeout_ms: timing.double_tap_ms,
        trigger_chord_timeout_ms: timing.chord_ms,
        default_macro_step_pause_ms: timing.macro_step_pause_ms,
        default_macro_modifier_delay_ms: timing.macro_modifier_delay_ms,
        macro_timeout_ms: timing.macro_timeout_ms,
        input_device_path: Some(keyboard_device.unwrap_or_default()),
        input_mouse_device_path: mouse_device,
        editing_keymap_id: None,
        keymap_mode: keymaps.mode,
        manual_keymap_id: keymap_ref(keymaps.manual_keymap, "settings.keymaps.manualKeymap")?,
        keymap_order: keymap_order(keymaps.order)?,
        auto_rules,
        auto_default_keymap_id: keymap_ref(
            keymaps.auto.fallback,
            "settings.keymaps.auto.fallback",
        )?,
        commands_enabled: settings.commands_enabled,
        game_mode: GameModeSettings {
            use_gamemoded: detectors.gamemoded,
            use_fullscreen: detectors.fullscreen,
            use_steam: detectors.steam,
            use_lutris: detectors.lutris,
            use_heroic: detectors.heroic,
            use_umu: detectors.umu,
            use_gamescope: detectors.gamescope,
            use_app_category: detectors.app_category,
            use_game_bar: detectors.game_bar,
            foreground_only: settings.game_mode.foreground_only,
            process_matchers: settings
                .game_mode
                .processes
                .into_iter()
                .enumerate()
                .map(|(index, process)| {
                    Ok(GameModeProcessMatcher {
                        id: format!("process-{}", index + 1),
                        name: process_name(&process.name, index)?,
                        only_active_window: process.foreground_only,
                        is_blacklist: process.exclude,
                    })
                })
                .collect::<Result<_, String>>()?,
            control: settings.game_mode.control,
        },
        linux_wayland_text_mode: settings.linux.text_mode,
        linux_paste_shortcut: settings.linux.paste_shortcut,
        linux_terminal_paste_shortcut: settings.linux.terminal_paste_shortcut.0,
        linux_ydotool_path: settings.linux.ydotool_path.unwrap_or_default(),
        linux_xdotool_path: settings.linux.xdotool_path.unwrap_or_default(),
    })
}

fn to_doc(settings: &AppSettings) -> ConfigDoc {
    let game_mode = &settings.game_mode;
    let filled = |value: &Option<String>| value.clone().filter(|value| !value.is_empty());
    ConfigDoc {
        version: CONFIG_SCHEMA_VERSION,
        schema: settings.schema.clone(),
        settings: SettingsDoc {
            experimental: settings.in_development,
            launch_at_login: settings.launch_on_startup,
            start_mapper_on_launch: settings.start_mapper_on_launch,
            interface: InterfaceDoc {
                appearance: settings.appearance,
                high_contrast: settings.high_contrast,
                reduce_motion: settings.reduce_motion,
                locale: settings.locale,
                keymap_osd: settings.keymap_osd,
                layer_hint: settings.layer_hint,
            },
            devices: DevicesDoc {
                keyboards: filled(&settings.input_device_path).into_iter().collect(),
                mice: filled(&settings.input_mouse_device_path)
                    .into_iter()
                    .collect(),
            },
            timing: TimingDoc {
                hold_ms: settings.default_hold_timeout_ms,
                long_hold_ms: settings.default_long_hold_timeout_ms,
                double_tap_ms: settings.default_double_tap_timeout_ms,
                chord_ms: settings.trigger_chord_timeout_ms,
                tap_decision: settings.tap_decision,
                macro_step_pause_ms: settings.default_macro_step_pause_ms,
                macro_modifier_delay_ms: settings.default_macro_modifier_delay_ms,
                macro_timeout_ms: settings.macro_timeout_ms,
            },
            keymaps: KeymapsDoc {
                mode: settings.keymap_mode,
                manual_keymap: filled(&settings.manual_keymap_id),
                order: settings
                    .keymap_order
                    .iter()
                    .enumerate()
                    .filter(|(index, id)| !settings.keymap_order[..*index].contains(id))
                    .map(|(_, id)| id.clone())
                    .collect(),
                auto: AutoDoc {
                    rules: settings
                        .auto_rules
                        .iter()
                        .map(|rule| AutoRuleDoc {
                            keymap: filled(&rule.keymap_id),
                            enabled: rule.enabled,
                            when: WhenDoc::from_set(&rule.conditions),
                        })
                        .collect(),
                    fallback: filled(&settings.auto_default_keymap_id),
                },
            },
            commands_enabled: settings.commands_enabled,
            game_mode: GameModeDoc {
                control: game_mode.control,
                detectors: DetectorsDoc {
                    gamemoded: game_mode.use_gamemoded,
                    fullscreen: game_mode.use_fullscreen,
                    steam: game_mode.use_steam,
                    lutris: game_mode.use_lutris,
                    heroic: game_mode.use_heroic,
                    umu: game_mode.use_umu,
                    gamescope: game_mode.use_gamescope,
                    app_category: game_mode.use_app_category,
                    game_bar: game_mode.use_game_bar,
                },
                foreground_only: game_mode.foreground_only,
                // Entries the reader rejects (a blank name the editor holds
                // transiently, a title pattern) are left out, so the file
                // loads again.
                processes: game_mode
                    .process_matchers
                    .iter()
                    .enumerate()
                    .filter(|(index, process)| process_name(&process.name, *index).is_ok())
                    .map(|(_, process)| ProcessDoc {
                        name: process.name.clone(),
                        foreground_only: process.only_active_window,
                        exclude: process.is_blacklist,
                    })
                    .collect(),
            },
            linux: LinuxDoc {
                text_mode: settings.linux_wayland_text_mode,
                copy_osd: settings.copy_osd,
                paste_shortcut: settings.linux_paste_shortcut,
                terminal_paste_shortcut: TerminalPasteDoc(settings.linux_terminal_paste_shortcut),
                ydotool_path: Some(settings.linux_ydotool_path.clone())
                    .filter(|path| !path.trim().is_empty()),
                xdotool_path: Some(settings.linux_xdotool_path.clone())
                    .filter(|path| !path.trim().is_empty()),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn config(settings: Value) -> String {
        json!({"version": 1, "settings": settings}).to_string()
    }

    #[test]
    fn all_interface_locales_persist_without_changing_schema_version() {
        for code in [
            "auto", "en", "ru", "de", "fr", "es", "it", "pt", "pl", "tr", "uk", "cs", "hu", "ro",
            "el", "hr", "ka", "kk", "zh-Hans", "zh-Hant", "ja", "ko",
        ] {
            let settings = parse(&config(json!({"interface": {"locale": code}}))).unwrap();
            assert_eq!(settings.locale.as_str(), code);
            let saved = serialize(&settings);
            let raw: Value = serde_json::from_str(&saved).unwrap();
            assert_eq!(raw["version"], CONFIG_SCHEMA_VERSION);
            assert_eq!(raw["settings"]["interface"]["locale"], json!(code));
            assert_eq!(parse(&saved).unwrap().locale, settings.locale);
        }
    }

    #[test]
    fn editor_schema_is_preserved_and_strictly_typed() {
        let text = r#"{"version":1,"$schema":"https://example.com/config.schema.json"}"#;
        let mut settings = parse(text).unwrap();
        settings.high_contrast = AccessibilityChoice::On;
        let saved = serialize(&settings);
        assert_eq!(parse(&saved).unwrap(), settings);
        let raw: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(raw["$schema"], "https://example.com/config.schema.json");
        for value in ["null", "1", "{}"] {
            assert!(parse(&format!(r#"{{"version":1,"$schema":{value}}}"#)).is_err());
        }
    }

    #[test]
    fn experimental_flag_is_optional() {
        let mut settings = parse(&config(json!({}))).unwrap();
        assert!(!settings.in_development);
        settings.in_development = true;
        assert!(parse(&serialize(&settings)).unwrap().in_development);
    }

    /// Defaults are written, so a later change of a default does not change
    /// what an existing installation does.
    #[test]
    fn defaults_are_written() {
        let raw: Value = serde_json::from_str(&serialize(&AppSettings::default())).unwrap();
        let settings = &raw["settings"];
        assert_eq!(settings["experimental"], false);
        assert_eq!(settings["interface"]["appearance"], "system");
        assert_eq!(settings["interface"]["locale"], "auto");
        assert_eq!(settings["devices"], json!({"keyboards": [], "mice": []}));
        assert_eq!(settings["timing"]["holdMs"], 200);
        assert_eq!(settings["timing"]["tapDecision"], "permissiveHold");
        assert_eq!(settings["keymaps"]["auto"]["fallback"], Value::Null);
        assert_eq!(settings["gameMode"]["detectors"]["fullscreen"], false);
        assert_eq!(settings["linux"]["pasteShortcut"], "Ctrl+KeyV");
        assert_eq!(settings["linux"]["terminalPasteShortcut"], "Shift+Insert");
        assert_eq!(settings["linux"]["copyOsd"], false);
        assert!(settings["linux"].get("ydotoolPath").is_none());
        assert!(settings["keymaps"].get("manualKeymap").is_none());
        assert_eq!(
            parse(&serialize(&AppSettings::default())).unwrap(),
            AppSettings::default()
        );
    }

    #[test]
    fn auto_rules_and_processes_store_no_ids() {
        assert!(
            parse(&config(
                json!({"keymaps":{"auto":{"rules":[{"id":"x","keymap":null}]}}})
            ))
            .is_err()
        );
        assert!(
            parse(&config(
                json!({"gameMode":{"processes":[{"id":"x","name":"a"}]}})
            ))
            .is_err()
        );
        let settings = parse(&config(json!({
            "keymaps":{"auto":{"rules":[{"keymap":null}, {"keymap":"user:A"}]}},
            "gameMode":{"processes":[{"name":"a"},{"name":"b"}]}
        })))
        .unwrap();
        assert_eq!(settings.auto_rules[1].id, "rule-2");
        assert_eq!(settings.game_mode.process_matchers[1].id, "process-2");
        let saved = serialize(&settings);
        assert!(!saved.contains("\"id\""));
        assert_eq!(parse(&saved).unwrap(), settings);
    }

    #[test]
    fn settings_timing_boundaries_are_enforced() {
        use super::super::timing::*;
        for (field, range) in [
            ("holdMs", TAP_HOLD),
            ("doubleTapMs", TAP_HOLD),
            ("longHoldMs", LONG_HOLD),
            ("chordMs", TRIGGER_CHORD),
            ("macroStepPauseMs", MACRO_DELAY),
            ("macroModifierDelayMs", MACRO_DELAY),
            ("macroTimeoutMs", MACRO_TIMEOUT),
        ] {
            for value in [0, range.min, range.max, range.max + 1, u64::MAX] {
                let text = config(serde_json::json!({"timing":{field:value}}));
                assert_eq!(
                    parse(&text).is_ok(),
                    range.contains(value),
                    "{field}: {value}"
                );
            }
        }
    }

    #[test]
    fn missing_settings_use_defaults() {
        assert_eq!(parse("").unwrap(), AppSettings::default());
        assert_eq!(parse(r#"{"version": 1}"#).unwrap(), AppSettings::default());
        assert_eq!(parse(&config(json!({}))).unwrap(), AppSettings::default());
    }

    #[test]
    fn accessibility_choices_are_strict_and_round_trip() {
        for (value, choice) in [
            ("system", AccessibilityChoice::System),
            ("on", AccessibilityChoice::On),
            ("off", AccessibilityChoice::Off),
        ] {
            let settings = parse(&config(
                json!({"interface": {"highContrast": value, "reduceMotion": value}}),
            ))
            .unwrap();
            assert_eq!(settings.high_contrast, choice);
            assert_eq!(settings.reduce_motion, choice);
            assert_eq!(parse(&serialize(&settings)).unwrap(), settings);
        }
        for value in [json!(true), json!(false)] {
            assert!(parse(&config(json!({"interface": {"highContrast": value}}))).is_err());
            assert!(parse(&config(json!({"interface": {"reduceMotion": value}}))).is_err());
        }
    }

    #[test]
    fn text_modes_use_current_names_only() {
        for (value, mode) in [
            (
                "libeiWithClipboardFallback",
                TextMode::LibeiWithClipboardFallback,
            ),
            ("libeiOnly", TextMode::LibeiOnly),
            ("clipboardPaste", TextMode::Clipboard),
            ("portalKeycode", TextMode::Keycode),
        ] {
            let settings = parse(&config(json!({"linux": {"textMode": value}}))).unwrap();
            assert_eq!(settings.linux_wayland_text_mode, mode);
            assert_eq!(parse(&serialize(&settings)).unwrap(), settings);
        }
        for value in ["libei", "libeiPure", "keycode", "clipboard"] {
            assert!(parse(&config(json!({"linux": {"textMode": value}}))).is_err());
        }
    }

    #[test]
    fn renamed_settings_reject_old_names() {
        assert!(parse(&config(json!({"launchOnStartup": true}))).is_err());
        assert!(parse(&config(json!({"timing": {"comboMs": 50}}))).is_err());
        let settings = parse(&config(
            json!({"launchAtLogin": true, "timing": {"chordMs": 75}}),
        ))
        .unwrap();
        assert!(settings.launch_on_startup);
        assert_eq!(settings.trigger_chord_timeout_ms, 75);
        let raw: Value = serde_json::from_str(&serialize(&settings)).unwrap();
        assert_eq!(raw["settings"]["launchAtLogin"], true);
        assert_eq!(raw["settings"]["timing"]["chordMs"], 75);
    }

    #[test]
    fn version_is_required_and_checked() {
        assert!(
            parse(r#"{"settings": {}}"#)
                .unwrap_err()
                .contains("version")
        );
        assert!(parse(r#"{"version": 2}"#).unwrap_err().contains("newer"));
        assert!(parse(r#"[]"#).is_err());
    }

    #[test]
    fn invalid_or_unknown_values_are_errors() {
        for settings in [
            json!({"interface": {"appearance": "Dark"}}),
            json!({"interface": {"locale": "de-DE"}}),
            json!({"interface": {"locale": "en-US"}}),
            json!({"interface": {"appearance": "eink"}}),
            json!({"appearance": "dark"}),
            json!({"copyOsd": true}),
            json!({"linux": {"textMode": "libei-pure"}}),
            json!({"linux": {"pasteShortcut": "shift-insert"}}),
            json!({"linux": {"pasteShortcut": "Alt+KeyV"}}),
            json!({"inDevelopment": true}),
            json!({"activeKeymapId": "user:A"}),
            json!({"futureOption": 7}),
            json!({"timing": {"holdMs": "fast"}}),
            json!({"timing": {"tapDecision": "sometimes"}}),
            json!({"linux": {"textMode": "magic"}}),
            json!({"gameMode": {"detectors": {"origin": true}}}),
            json!({"gameMode": {"processes": [{"name": "steam", "isBlacklist": true}]}}),
            json!({"keymaps": {"auto": {"rules": [{"keymap": "user:A", "window_patterns": ["kate"]}]}}}),
            json!({"keymaps": {"auto": {"rules": [{"when": {"gameMode": "on"}}]}}}),
            json!({"keymaps": {"auto": {"rules": [{"keymap": ""}]}}}),
            json!({"keymaps": {"auto": {"fallback": " "}}}),
            json!({"keymaps": {"auto": {"rules": [{"id": "a", "keymap": null}]}}}),
            json!({"keymaps": {"active": "user:A"}}),
            json!({"keyboardDevice": "/dev/input/event1"}),
            json!({"devices": {"keyboards": ["/dev/a", "/dev/b"]}}),
            json!({"devices": {"mice": [""]}}),
            json!({"devices": {"tablets": []}}),
            json!({"keymaps": {"manualKeymap": "Main"}}),
            json!({"keymaps": {"manualKeymap": "user:"}}),
            json!({"keymaps": {"manualKeymap": "user: Main"}}),
            json!({"keymaps": {"order": ["user:A", "user:A"]}}),
            json!({"keymaps": {"order": ["user:Main", "user:main"]}}),
            json!({"keymaps": {"order": ["bundled:x"]}}),
            json!({"keymaps": {"auto": {"fallback": "user:a/b"}}}),
            json!({"gameMode": {"processes": [{"name": " "}]}}),
            json!({"gameMode": {"processes": [{"name": "title:Steam"}]}}),
        ] {
            assert!(
                parse(&config(settings.clone())).is_err(),
                "accepted {settings}"
            );
        }
        assert!(parse(r#"{"version": 1, "extra": true}"#).is_err());
        assert!(parse(r#"{"version": 1, "state": {"gameModeSwitch": "maybe"}}"#).is_err());
        assert!(parse(r#"{"version": 1, "state": {"gameMode": "on"}}"#).is_err());
    }

    #[test]
    fn every_setting_round_trips() {
        let settings = AppSettings {
            copy_osd: true,
            launch_on_startup: true,
            start_mapper_on_launch: true,
            appearance: Appearance::EInk,
            locale: LocalePreference::Russian,
            default_hold_timeout_ms: 180,
            tap_decision: TapDecision::HoldOnOtherKeyPress,
            input_device_path: Some("/dev/input/by-id/kbd".into()),
            input_mouse_device_path: Some("/dev/input/by-id/mouse".into()),
            keymap_mode: KeymapMode::Auto,
            manual_keymap_id: Some("user:Other".into()),
            keymap_order: vec!["user:Other".into(), "user:Main".into()],
            auto_rules: vec![AutoRule {
                id: "rule-1".into(),
                keymap_id: None,
                enabled: false,
                conditions: super::super::model::KeymapConditionSet {
                    game_mode: Some(crate::config::model::GameModeCondition::On),
                    keyboard_layouts: vec!["us".into()],
                    window_patterns: vec!["blender".into()],
                    excluded_window_patterns: vec!["title:Preferences".into()],
                },
            }],
            auto_default_keymap_id: Some("user:Main".into()),
            commands_enabled: true,
            game_mode: GameModeSettings {
                use_fullscreen: true,
                process_matchers: vec![GameModeProcessMatcher {
                    id: "process-1".into(),
                    name: "steam".into(),
                    only_active_window: false,
                    is_blacklist: true,
                }],
                control: GameModeControl::On,
                ..AppSettings::default().game_mode
            },
            linux_wayland_text_mode: TextMode::Clipboard,
            linux_paste_shortcut: PasteShortcut::CtrlShiftV,
            linux_terminal_paste_shortcut: None,
            linux_ydotool_path: "/usr/bin/ydotool".into(),
            ..AppSettings::default()
        };
        let text = serialize(&settings);
        assert_eq!(parse(&text).unwrap(), settings);
        let raw: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(raw["version"], CONFIG_SCHEMA_VERSION);
        assert_eq!(raw["settings"]["timing"]["holdMs"], 180);
        assert_eq!(raw["settings"]["linux"]["pasteShortcut"], "Ctrl+Shift+KeyV");
        assert_eq!(raw["settings"]["interface"]["appearance"], "eInk");
        assert_eq!(raw["settings"]["interface"]["locale"], "ru");
        assert_eq!(raw["settings"]["linux"]["copyOsd"], true);
        assert_eq!(raw["settings"]["linux"]["textMode"], "clipboardPaste");
        assert_eq!(
            raw["settings"]["linux"]["terminalPasteShortcut"],
            "sameAsPaste"
        );
        assert_eq!(raw["settings"]["gameMode"]["processes"][0]["exclude"], true);
        assert!(raw.get("state").is_none());
        assert_eq!(raw["settings"]["gameMode"]["control"], "on");
        let keymaps = &raw["settings"]["keymaps"];
        assert_eq!(keymaps["mode"], "auto");
        assert_eq!(keymaps["order"], json!(["user:Other", "user:Main"]));
        assert!(raw["state"].get("keymapOrder").is_none());
        assert_eq!(
            keymaps["auto"]["rules"][0]["when"]["excludeWindows"],
            json!(["title:Preferences"])
        );
        assert_eq!(keymaps["auto"]["rules"][0]["keymap"], Value::Null);
        assert_eq!(keymaps["auto"]["fallback"], "user:Main");
        assert_eq!(
            raw["settings"]["devices"],
            json!({"keyboards": ["/dev/input/by-id/kbd"], "mice": ["/dev/input/by-id/mouse"]})
        );
        assert_eq!(keymaps["manualKeymap"], "user:Other");
    }

    #[test]
    fn partial_sections_keep_other_defaults() {
        let settings = parse(&config(json!({
            "timing": {"holdMs": 300},
            "gameMode": {"detectors": {"lutris": false}, "processes": [{"name": " steam "}]},
        })))
        .unwrap();
        let base = AppSettings::default();
        assert_eq!(settings.default_hold_timeout_ms, 300);
        assert_eq!(
            settings.default_long_hold_timeout_ms,
            base.default_long_hold_timeout_ms
        );
        assert!(!settings.game_mode.use_lutris);
        assert_eq!(settings.game_mode.use_steam, base.game_mode.use_steam);
        let process = &settings.game_mode.process_matchers[0];
        assert_eq!(process.name, "steam");
        assert_eq!(process.id, "process-1");
        assert!(process.only_active_window);
        assert!(!process.is_blacklist);
    }

    #[test]
    fn paste_defaults_do_not_override_explicit_shortcuts() {
        let defaults = parse(&config(json!({}))).unwrap();
        assert_eq!(defaults.linux_paste_shortcut, PasteShortcut::CtrlV);
        assert_eq!(
            defaults.linux_terminal_paste_shortcut,
            Some(PasteShortcut::ShiftInsert)
        );
        let explicit = parse(&config(json!({"linux": {
            "pasteShortcut": "Shift+Insert",
            "terminalPasteShortcut": "Ctrl+Shift+KeyV"
        }})))
        .unwrap();
        assert_eq!(explicit.linux_paste_shortcut, PasteShortcut::ShiftInsert);
        assert_eq!(
            explicit.linux_terminal_paste_shortcut,
            Some(PasteShortcut::CtrlShiftV)
        );
    }

    #[test]
    fn terminal_paste_shortcut_defaults_and_can_follow_the_main_one() {
        let shortcut = |linux: Value| {
            parse(&config(json!({ "linux": linux })))
                .unwrap()
                .linux_terminal_paste_shortcut
        };
        assert_eq!(
            shortcut(json!({"pasteShortcut": "Ctrl+KeyV"})),
            Some(PasteShortcut::ShiftInsert)
        );
        assert_eq!(
            shortcut(json!({"terminalPasteShortcut": "sameAsPaste"})),
            None
        );
        assert_eq!(
            shortcut(json!({"terminalPasteShortcut": "Shift+Insert"})),
            Some(PasteShortcut::ShiftInsert)
        );
        // Any modifier order and spelling names the same chord.
        assert_eq!(
            shortcut(json!({"terminalPasteShortcut": "ShiftLeft+Ctrl+KeyV"})),
            Some(PasteShortcut::CtrlShiftV)
        );
        let null = config(json!({"linux": {"terminalPasteShortcut": null}}));
        assert!(parse(&null).is_err());
    }

    #[test]
    fn program_paths_are_omitted_rather_than_empty() {
        for bad in [json!(""), json!("  "), Value::Null] {
            assert!(parse(&config(json!({"linux": {"ydotoolPath": bad}}))).is_err());
        }
        let settings = parse(&config(json!({"linux": {"xdotoolPath": "/opt/xdotool"}}))).unwrap();
        assert_eq!(settings.linux_xdotool_path, "/opt/xdotool");
        assert_eq!(settings.linux_ydotool_path, "");
        let raw: Value = serde_json::from_str(&serialize(&AppSettings::default())).unwrap();
        assert!(raw["settings"]["linux"].get("xdotoolPath").is_none());
    }

    #[test]
    fn mapper_start_on_launch_is_off_by_default() {
        assert!(!AppSettings::default().start_mapper_on_launch);
        let settings = parse(&config(json!({"startMapperOnLaunch": true}))).unwrap();
        assert!(settings.start_mapper_on_launch);
    }

    #[test]
    fn editor_identity_is_not_persisted() {
        let settings = parse(r#"{"version":1,"settings":{"keymaps":{"manualKeymap":"user:Active"},"gameMode":{"control":"on"}}}"#).unwrap();
        assert!(settings.editing_keymap_id.is_none());
        assert_eq!(settings.manual_keymap_id.as_deref(), Some("user:Active"));
        assert_eq!(settings.game_mode.control, GameModeControl::On);
        let mut editing = settings.clone();
        editing.editing_keymap_id = Some("user:Editor".into());
        let saved = serialize(&editing);
        assert_eq!(parse(&saved).unwrap(), settings);
        let raw: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(raw["version"], 1);
        assert!(raw.get("state").is_none());
        assert_eq!(raw["settings"]["gameMode"]["control"], "on");
        assert!(parse(r#"{"version":1,"state":{}}"#).is_err());
        assert!(parse(r#"{"version":1,"settings":{"gameMode":{"control":"maybe"}}}"#).is_err());
    }

    #[test]
    fn auto_rules_get_ids_by_position() {
        let settings = parse(&config(json!({"keymaps": {"auto": {
            "rules": [
                {"keymap": "user:A", "when": {"windows": ["blender"]}},
                {"keymap": null, "enabled": false},
            ],
            "fallback": "user:B",
        }}})))
        .unwrap();
        assert_eq!(settings.auto_rules.len(), 2);
        assert_eq!(
            settings.auto_rules[0].conditions.window_patterns,
            ["blender"]
        );
        assert_eq!(settings.auto_rules[1].id, "rule-2");
        assert_eq!(settings.auto_rules[1].keymap_id, None);
        assert!(!settings.auto_rules[1].enabled);
        assert!(settings.auto_rules[1].conditions.is_empty());
        assert_eq!(settings.auto_default_keymap_id.as_deref(), Some("user:B"));
    }
}
