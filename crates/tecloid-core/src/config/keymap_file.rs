//! Saved keymap JSON files in the user library.
//!
//! The format is versioned (`"version": 1`) and read strictly: unknown
//! fields, wrong types and unknown enum values are errors, so a file is
//! never silently rewritten with parts of it dropped. Semantic checks
//! (unknown macros, invalid key names in actions) are left to the
//! diagnostics, which report them without refusing the file.

use super::model::GameModeCondition as OnOff;
use super::model::{
    Command, EmojiPage, HoldBehavior, KeyRule, KeymapConditionSet, KeymapPreset, LEFT_HAND_HOTKEYS,
    Layer, LayerKeymap, Macro, MacroStep, QuickAction, QuickActionPage, TapDecision,
};
use super::schema::{self, KEYMAP_SCHEMA_VERSION};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};

/// JSON Schema of the keymap file, published under `schemas/`.
#[cfg(test)]
pub(crate) fn json_schema() -> schemars::Schema {
    schemars::schema_for!(KeymapDoc)
}

/// Parse a keymap file. Returns `Ok(None)` for an empty document.
pub fn parse(text: &str) -> Result<Option<KeymapPreset>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.trim().is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("parse keymap: {error}"))?;
    let keymap = match schema::read_version(&value, "keymap", KEYMAP_SCHEMA_VERSION)? {
        1 => from_doc(
            serde_json::from_str::<KeymapDoc>(text)
                .map_err(|error| schema::read_error("parse keymap", &error))?,
        )?,
        version => return Err(format!("keymap: no reader for schema version {version}")),
    };
    super::timing::validate_keymap(&keymap)?;
    Ok(Some(keymap))
}

pub fn serialize(preset: &KeymapPreset) -> String {
    serde_json::to_string_pretty(&to_doc(preset)).unwrap_or_default()
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeymapDoc {
    #[cfg_attr(test, schemars(extend("const" = KEYMAP_SCHEMA_VERSION)))]
    version: u64,
    #[serde(
        rename = "$schema",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "String"))]
    schema: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "MetaDoc"))]
    meta: Option<MetaDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    layers: Vec<LayerDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<RuleDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    macros: Vec<MacroDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    commands: Vec<CommandDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    quick_menu_pages: Vec<PageDoc<QuickCellDoc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    emoji_menu_pages: Vec<PageDoc<String>>,
}

/// Portable information about the keymap from its author. Every field is
/// optional; `null` is not allowed.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MetaDoc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    author: Option<String>,
    /// The author's version of the keymap, unrelated to the format `version`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    keymap_version: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    description: Option<String>,
}

impl MetaDoc {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.author.is_none()
            && self.keymap_version.is_none()
            && self.description.is_none()
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LayerDoc {
    #[cfg_attr(test, schemars(regex(pattern = super::schema::ID_PATTERN)))]
    id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    description: Option<String>,
    /// Saved display order of additional keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extra_key_order: Vec<String>,
    /// Key code → action; `null` swallows the key, a missing key is
    /// transparent. Written in keyboard order.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        serialize_with = "keyboard_order"
    )]
    keys: BTreeMap<String, LayerKeyDoc>,
}

/// A layer key: a bare action (`null` swallows) or an action with options.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum LayerKeyDoc {
    Action(Option<String>),
    Full(LayerKeyFullDoc),
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LayerKeyFullDoc {
    action: String,
    /// The chord's modifiers stay pressed until the key holding the layer
    /// is released.
    #[serde(default, skip_serializing_if = "is_false")]
    hold_modifiers: bool,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuleDoc {
    key: RuleKeyDoc,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    enabled: bool,
    #[serde(default, skip_serializing_if = "WhenDoc::is_empty")]
    when: WhenDoc,
    /// Missing: native key, `null`: swallowed, string: action.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    tap: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "TimedActionDoc"))]
    double_tap: Option<TimedActionDoc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "HoldDoc"))]
    hold: Option<HoldDoc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "LongHoldDoc"))]
    long_hold: Option<LongHoldDoc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "TapDecision"))]
    tap_decision: Option<TapDecision>,
}

/// Trigger of a rule: a physical key or side-neutral modifier, or a chord of
/// keys as an array. `+` is not accepted inside a key: in action strings it
/// joins modifiers to a key, which is a different meaning.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum RuleKeyDoc {
    Key(#[cfg_attr(test, schemars(regex(pattern = r"^[^+]+$")))] String),
    Chord(
        #[cfg_attr(test, schemars(length(min = 2), inner(regex(pattern = r"^[^+]+$"))))]
        Vec<String>,
    ),
}

/// A trigger key in its catalog spelling (`capslock` → `CapsLock`); an
/// unknown name is kept for the diagnostics to report.
fn canonical_trigger(key: String) -> String {
    super::key_catalog::canonical_key(&key).map_or(key, str::to_owned)
}

/// A physical key code in its catalog spelling (`keyh` → `KeyH`); an
/// unknown name is kept for the diagnostics to report.
fn canonical_key_code(code: String) -> String {
    match super::key_catalog::canonical_key(&code) {
        Some(key) if super::key_catalog::valid_key(key) => key.to_owned(),
        _ => code,
    }
}

impl RuleKeyDoc {
    /// The model's `+`-joined key.
    fn into_key(self) -> Result<String, String> {
        match self {
            Self::Key(key) => {
                non_empty(&key, "rule key")?;
                if key.contains('+') {
                    let parts: Vec<_> = key.split('+').collect();
                    return Err(format!(
                        "rule key {key:?}: write a chord as an array of keys, e.g. {parts:?}"
                    ));
                }
                Ok(canonical_trigger(key))
            }
            Self::Chord(keys) => {
                if keys.len() < 2 {
                    return Err(format!(
                        "rule key {keys:?}: a chord needs at least two keys; write a single key as a string"
                    ));
                }
                for key in &keys {
                    non_empty(key, "rule chord key")?;
                    if key.contains('+') {
                        return Err(format!("rule chord key {key:?} must be one key code"));
                    }
                }
                let keys: Vec<String> = keys.into_iter().map(canonical_trigger).collect();
                if let Some(key) = keys
                    .iter()
                    .enumerate()
                    .find_map(|(index, key)| keys[..index].contains(key).then_some(key))
                {
                    return Err(format!("rule chord {keys:?}: key {key} is listed twice"));
                }
                Ok(keys.join("+"))
            }
        }
    }

    fn from_key(key: &str) -> Self {
        if key.contains('+') {
            Self::Chord(key.split('+').map(str::to_owned).collect())
        } else {
            Self::Key(key.to_owned())
        }
    }
}

/// Conditions of a keymap rule or an auto-mode rule; every present
/// condition must hold. Both places share this one object.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WhenDoc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "OnOff"))]
    game_mode: Option<OnOff>,
    /// Short ids of the system keyboard layout, e.g. `us`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    keyboard_layouts: Vec<String>,
    /// Window patterns, see [`super::app_match`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    windows: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exclude_windows: Vec<String>,
}

impl WhenDoc {
    pub(crate) fn is_empty(&self) -> bool {
        self.game_mode.is_none()
            && self.keyboard_layouts.is_empty()
            && self.windows.is_empty()
            && self.exclude_windows.is_empty()
    }

    fn check(&self) -> Result<(), String> {
        non_empty_items(&self.keyboard_layouts, "keyboard layout")?;
        non_empty_items(&self.windows, "window pattern")?;
        non_empty_items(&self.exclude_windows, "window pattern")
    }

    pub(crate) fn into_set(self) -> Result<KeymapConditionSet, String> {
        self.check()?;
        Ok(KeymapConditionSet {
            game_mode: self.game_mode,
            keyboard_layouts: self.keyboard_layouts,
            window_patterns: self.windows,
            excluded_window_patterns: self.exclude_windows,
        })
    }

    pub(crate) fn from_set(set: &KeymapConditionSet) -> Self {
        let list = |items: &[String]| -> Vec<String> {
            items
                .iter()
                .filter(|item| !item.trim().is_empty())
                .cloned()
                .collect()
        };
        Self {
            game_mode: set.game_mode,
            keyboard_layouts: list(&set.keyboard_layouts),
            windows: list(&set.window_patterns),
            exclude_windows: list(&set.excluded_window_patterns),
        }
    }
}

/// `doubleTap`: an action and its own tap window.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TimedActionDoc {
    action: String,
    #[cfg_attr(test, schemars(range(min = super::timing::TAP_HOLD.min, max = super::timing::TAP_HOLD.max)))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "u64"))]
    timeout_ms: Option<u64>,
}

/// `longHold`: an action and its own long-hold threshold.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LongHoldDoc {
    action: String,
    #[cfg_attr(test, schemars(range(min = super::timing::LONG_HOLD.min, max = super::timing::LONG_HOLD.max)))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "u64"))]
    timeout_ms: Option<u64>,
}

/// What holding the key does: activate a layer and/or hold other keys.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HoldDoc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    layer: Option<String>,
    /// Key or chord held down while the key is held. Missing: the key
    /// itself (or nothing while a layer is active), `null`: nothing.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    action: Option<Option<String>>,
    #[cfg_attr(test, schemars(range(min = super::timing::TAP_HOLD.min, max = super::timing::TAP_HOLD.max)))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "u64"))]
    timeout_ms: Option<u64>,
    /// Layer holds only: released within this many milliseconds without any
    /// other input, the hold still fires the tap. Missing: twice the hold
    /// timeout; 0: off.
    #[cfg_attr(test, schemars(range(min = super::timing::RETRO_TAP.min, max = super::timing::RETRO_TAP.max)))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "u64"))]
    retro_tap_ms: Option<u64>,
}

impl HoldDoc {
    fn is_empty(&self) -> bool {
        self.layer.is_none()
            && self.action.is_none()
            && self.timeout_ms.is_none()
            && self.retro_tap_ms.is_none()
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MacroDoc {
    #[cfg_attr(test, schemars(regex(pattern = super::schema::ID_PATTERN)))]
    id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    name: Option<String>,
    #[cfg_attr(test, schemars(range(min = super::timing::MACRO_DELAY.min, max = super::timing::MACRO_DELAY.max)))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "u64"))]
    step_pause_ms: Option<u64>,
    #[cfg_attr(test, schemars(range(min = super::timing::MACRO_DELAY.min, max = super::timing::MACRO_DELAY.max)))]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::non_null"
    )]
    #[cfg_attr(test, schemars(with = "u64"))]
    modifier_delay_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    steps: Vec<String>,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandDoc {
    #[cfg_attr(test, schemars(regex(pattern = super::schema::ID_PATTERN)))]
    id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    name: Option<String>,
    /// POSIX `sh` script, run on Linux and macOS.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "schema::filled"
    )]
    #[cfg_attr(test, schemars(length(min = 1)))]
    sh: String,
    /// PowerShell script, run on Windows.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "schema::filled"
    )]
    #[cfg_attr(test, schemars(length(min = 1)))]
    powershell: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    working_directory: Option<String>,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
#[cfg_attr(test, schemars(bound = "T: schemars::JsonSchema", inline))]
struct PageDoc<T> {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "GridCells::is_empty")]
    cells: GridCells<T>,
}

/// Menu cells keyed by grid hotkey (`KeyQ`…`KeyB`), written in grid order.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[cfg_attr(test, schemars(transparent, inline, extend("propertyNames" = { "enum": LEFT_HAND_HOTKEYS })))]
#[derive(Deserialize)]
#[serde(transparent)]
struct GridCells<T>(BTreeMap<String, T>);

impl<T> GridCells<T> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The cells keyed by their canonical hotkey (`keyq` → `KeyQ`); a key
    /// outside the grid or listed twice in two spellings is an error.
    fn canonical(self, menu: &str) -> Result<BTreeMap<String, T>, String> {
        let mut out = BTreeMap::new();
        for (key, cell) in self.0 {
            let Some(hotkey) = LEFT_HAND_HOTKEYS
                .iter()
                .find(|hotkey| hotkey.eq_ignore_ascii_case(&key))
            else {
                return Err(format!("{menu} page cell {key}: not a menu hotkey"));
            };
            if out.insert((*hotkey).to_owned(), cell).is_some() {
                return Err(format!("{menu} page cell {hotkey} is listed twice"));
            }
        }
        Ok(out)
    }
}

impl<T> Default for GridCells<T> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<T> FromIterator<(String, T)> for GridCells<T> {
    fn from_iter<I: IntoIterator<Item = (String, T)>>(items: I) -> Self {
        Self(items.into_iter().collect())
    }
}

impl<T: Serialize> Serialize for GridCells<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(
            LEFT_HAND_HOTKEYS
                .iter()
                .filter_map(|key| self.0.get(*key).map(|cell| (*key, cell))),
        )
    }
}

/// A quick-action cell: a bare action or an action with a caption.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum QuickCellDoc {
    Action(String),
    Full(QuickActionDoc),
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuickActionDoc {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "schema::text"
    )]
    #[cfg_attr(test, schemars(with = "String", length(min = 1)))]
    name: Option<String>,
    action: String,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Distinguish a missing field (`None`) from an explicit `null`
/// (`Some(None)`).
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

/// Layer keys in keyboard order: grid rows from left to right, then the key
/// catalog, then anything else alphabetically.
fn keyboard_order<S: Serializer>(
    keys: &BTreeMap<String, LayerKeyDoc>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut order: Vec<&str> = Vec::new();
    let grid = super::layer_grid::LEFT_HAND
        .iter()
        .zip(super::layer_grid::RIGHT_HAND.iter())
        .flat_map(|(left, right)| left.iter().chain(right.iter()).copied());
    for key in grid.chain(
        super::key_catalog::CATEGORIES
            .iter()
            .flat_map(|keys| keys.iter().copied()),
    ) {
        if keys.contains_key(key) && !order.contains(&key) {
            order.push(key);
        }
    }
    let rest = keys.keys().filter(|key| !order.contains(&key.as_str()));
    let order: Vec<&str> = order
        .iter()
        .copied()
        .chain(rest.map(String::as_str))
        .collect();
    serializer.collect_map(order.into_iter().filter_map(|key| keys.get_key_value(key)))
}

fn non_empty(value: &str, what: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{what} must not be empty"))
    } else {
        Ok(())
    }
}

fn non_empty_items(items: &[String], what: &str) -> Result<(), String> {
    items.iter().try_for_each(|item| non_empty(item, what))
}

/// A key or chord action in its canonical spelling (see
/// [`super::key_catalog::normalize_chord`]); other actions are unchanged.
fn canonical(action: String) -> String {
    match super::actions::Action::parse(Some(&action)) {
        super::actions::Action::Keys(keys) => keys,
        _ => action,
    }
}

/// Three-state action of a parsed field: `Some("")` native, `None`
/// swallowed, otherwise the action.
fn three_state(value: Option<Option<String>>, what: &str) -> Result<Option<String>, String> {
    match value {
        None => Ok(Some(String::new())),
        Some(None) => Ok(None),
        Some(Some(action)) => {
            non_empty(&action, what)?;
            Ok(Some(canonical(action)))
        }
    }
}

/// Rules and menu pages are referenced by nothing (menus by position), so
/// the file stores no ids for them; the editor's ids are assigned here.
fn from_doc(doc: KeymapDoc) -> Result<KeymapPreset, String> {
    super::ids::unique(doc.layers.iter().map(|item| item.id.as_str()), "layers")?;
    super::ids::unique(doc.macros.iter().map(|item| item.id.as_str()), "macros")?;
    super::ids::unique(doc.commands.iter().map(|item| item.id.as_str()), "commands")?;
    // Step ids live only in memory, for the editor.
    let mut step_counter = 0u32;

    let mut layers = Vec::new();
    let mut layer_keymaps = BTreeMap::new();
    for layer in doc.layers {
        non_empty(&layer.id, "layer id")?;
        let mut keys = BTreeMap::new();
        let mut hold_modifiers = BTreeSet::new();
        for (code, key) in layer.keys {
            let code = canonical_key_code(code);
            if keys.contains_key(&code) {
                return Err(format!("layer {}: key {code} is listed twice", layer.id));
            }
            let action = match key {
                LayerKeyDoc::Action(action) => action,
                LayerKeyDoc::Full(full) => {
                    if full.hold_modifiers {
                        hold_modifiers.insert(code.clone());
                    }
                    Some(full.action)
                }
            }
            .map(canonical);
            if let Some(action) = &action {
                non_empty(action, &format!("action of {code} in layer {}", layer.id))?;
            }
            keys.insert(code, action);
        }
        layers.push(Layer {
            name: layer.name.unwrap_or_else(|| layer.id.clone()),
            description: layer.description.filter(|d| !d.is_empty()),
            id: layer.id.clone(),
        });
        let mut extra_key_order: Vec<String> = Vec::new();
        for code in layer.extra_key_order.into_iter().map(canonical_key_code) {
            if extra_key_order.contains(&code) {
                return Err(format!(
                    "layer {}: extraKeyOrder lists {code} twice",
                    layer.id
                ));
            }
            extra_key_order.push(code);
        }
        layer_keymaps.insert(
            layer.id,
            LayerKeymap {
                keys,
                extra_key_order,
                hold_modifiers,
            },
        );
    }

    let mut rules = Vec::new();
    for (index, rule) in doc.rules.into_iter().enumerate() {
        let key = rule.key.into_key()?;
        let what = |field: &str| format!("{field} of rule {key}");
        if rule.hold.is_some() && rule.long_hold.is_some() {
            return Err(format!(
                "rule {key}: \"hold\" and \"longHold\" cannot be combined"
            ));
        }
        let when = rule.when.into_set()?.normalized();
        let tap_action = three_state(rule.tap, &what("tap"))?;
        let (double_tap_action, double_tap_timeout_ms) = match rule.double_tap {
            Some(double) => {
                non_empty(&double.action, &what("doubleTap action"))?;
                (canonical(double.action), double.timeout_ms)
            }
            None => (String::new(), None),
        };
        let (long_hold_action, long_hold_timeout_ms) = match rule.long_hold {
            Some(long) => {
                non_empty(&long.action, &what("longHold action"))?;
                (canonical(long.action), long.timeout_ms)
            }
            None => (String::new(), None),
        };
        if rule.hold.as_ref().is_some_and(HoldDoc::is_empty) {
            return Err(format!(
                "rule {key}: \"hold\" must not be empty; omit the field instead"
            ));
        }
        let hold = rule.hold.unwrap_or_default();
        if hold.retro_tap_ms.is_some() && hold.layer.is_none() {
            return Err(format!(
                "rule {key}: \"hold.retroTapMs\" applies only to a hold with a \"layer\""
            ));
        }
        rules.push(KeyRule {
            id: format!("rule-{}", index + 1),
            enabled: (!rule.enabled).then_some(false),
            condition_game_mode: when.game_mode.map(|mode| mode.as_str().to_owned()),
            condition_keyboard_layouts: (!when.keyboard_layouts.is_empty())
                .then_some(when.keyboard_layouts),
            condition_window_patterns: (!when.window_patterns.is_empty())
                .then_some(when.window_patterns),
            condition_excluded_window_patterns: (!when.excluded_window_patterns.is_empty())
                .then_some(when.excluded_window_patterns),
            key: key.clone(),
            layer_id: hold.layer.unwrap_or_default(),
            tap_action,
            hold_behavior: None,
            long_hold_action,
            long_hold_timeout_ms,
            hold_action: three_state(hold.action, &what("hold action"))?,
            double_tap_action,
            hold_timeout_ms: hold.timeout_ms,
            retro_tap_ms: hold.retro_tap_ms,
            tap_decision: rule.tap_decision,
            double_tap_timeout_ms,
        });
    }

    let mut macros = Vec::new();
    for item in doc.macros {
        non_empty(&item.id, "macro id")?;
        let mut steps = Vec::new();
        for action in item.steps {
            non_empty(&action, &format!("step of macro {}", item.id))?;
            step_counter += 1;
            steps.push(MacroStep {
                id: format!("step-{step_counter}"),
                action: canonical(action),
            });
        }
        macros.push(Macro {
            name: item.name.unwrap_or_else(|| item.id.clone()),
            id: item.id,
            steps,
            step_pause_ms: item.step_pause_ms,
            modifier_delay_ms: item.modifier_delay_ms,
        });
    }

    let mut commands = Vec::new();
    for command in doc.commands {
        non_empty(&command.id, "command id")?;
        commands.push(Command {
            name: command.name.unwrap_or_else(|| command.id.clone()),
            id: command.id,
            posix_script: command.sh,
            powershell_script: command.powershell,
            working_directory: command.working_directory.filter(|dir| !dir.is_empty()),
        });
    }

    let mut quick_action_pages = Vec::new();
    for (index, page) in doc.quick_menu_pages.into_iter().enumerate() {
        let mut cells = BTreeMap::new();
        for (key, cell) in page.cells.canonical("quick action")? {
            let item = match cell {
                QuickCellDoc::Action(action) => QuickAction {
                    action,
                    ..QuickAction::default()
                },
                QuickCellDoc::Full(item) => QuickAction {
                    name: item.name.unwrap_or_default(),
                    action: item.action,
                },
            };
            non_empty(&item.action, &format!("quick action {key}"))?;
            let item = QuickAction {
                action: canonical(item.action),
                ..item
            };
            cells.insert(key, item);
        }
        quick_action_pages.push(QuickActionPage {
            id: format!("quick-{}", index + 1),
            name: page.name.unwrap_or_default(),
            cells,
        });
    }
    if quick_action_pages.is_empty() {
        quick_action_pages.push(QuickActionPage::default_page());
    }

    let mut emoji_pages = Vec::new();
    for (index, page) in doc.emoji_menu_pages.into_iter().enumerate() {
        let cells = page.cells.canonical("emoji")?;
        for (key, text) in &cells {
            non_empty(text, &format!("emoji {key}"))?;
        }
        emoji_pages.push(EmojiPage {
            id: format!("emoji-{}", index + 1),
            name: page.name.unwrap_or_default(),
            cells,
        });
    }
    if emoji_pages.is_empty() {
        emoji_pages.push(EmojiPage::default_page());
    }

    let meta = doc.meta.unwrap_or_default();
    let description = meta.description.filter(|d| !d.trim().is_empty());
    let meta = super::model::KeymapMetadata {
        name: meta.name,
        author: meta.author,
        version: meta.keymap_version,
    };
    Ok(KeymapPreset {
        schema: doc.schema,
        meta: (meta != super::model::KeymapMetadata::default()).then_some(meta),
        description,
        layers,
        rules,
        layer_keymaps,
        macros,
        commands,
        quick_action_pages,
        emoji_pages,
    })
}

/// Build the file form. Entries the parser would reject but the editor can
/// hold transiently (a rule without a key, an empty macro step) are left
/// out, so every saved file loads again.
fn to_doc(preset: &KeymapPreset) -> KeymapDoc {
    let text = |value: &str| (!value.trim().is_empty()).then(|| value.to_owned());
    // A name equal to the id is the default and is not written.
    let name = |name: &str, id: &str| (name != id).then(|| text(name)).flatten();
    let list = |items: &Option<Vec<String>>| -> Vec<String> {
        items
            .iter()
            .flatten()
            .filter(|item| !item.trim().is_empty())
            .cloned()
            .collect()
    };
    // `Some("")` is the native default and is omitted.
    let three_state = |action: &Option<String>| match action.as_deref() {
        Some(action) if action.trim().is_empty() => None,
        Some(action) => Some(Some(action.to_owned())),
        None => Some(None),
    };

    let layers = preset
        .layers
        .iter()
        .filter(|layer| !layer.id.is_empty())
        .map(|layer| {
            let keymap = preset.layer_keymaps.get(&layer.id);
            LayerDoc {
                id: layer.id.clone(),
                name: name(&layer.name, &layer.id),
                description: layer.description.as_deref().and_then(text),
                extra_key_order: keymap
                    .map(|keymap| keymap.extra_key_order.clone())
                    .unwrap_or_default(),
                keys: keymap
                    .map(|keymap| {
                        keymap
                            .keys
                            .iter()
                            // An empty entry is transparent, as the mapper
                            // treats it; `null` would swallow the key.
                            .filter(|(_, action)| {
                                action.as_ref().is_none_or(|a| !a.trim().is_empty())
                            })
                            .map(|(code, action)| {
                                // Only a key that carries an action can hold
                                // modifiers, so the file loads again.
                                let key = match action {
                                    Some(action) if keymap.hold_modifiers.contains(code) => {
                                        LayerKeyDoc::Full(LayerKeyFullDoc {
                                            action: action.clone(),
                                            hold_modifiers: true,
                                        })
                                    }
                                    action => LayerKeyDoc::Action(action.clone()),
                                };
                                (code.clone(), key)
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();

    let rules = preset
        .rules
        .iter()
        .filter(|rule| rule.has_trigger())
        .map(|rule| {
            let behavior = rule.hold_behavior();
            let long_hold = (behavior == HoldBehavior::Action
                && !rule.long_hold_action.trim().is_empty())
            .then(|| LongHoldDoc {
                action: rule.long_hold_action.clone(),
                timeout_ms: rule.long_hold_timeout_ms,
            });
            let hold = if behavior == HoldBehavior::Action {
                HoldDoc::default()
            } else {
                HoldDoc {
                    layer: (behavior == HoldBehavior::Layer)
                        .then(|| text(&rule.layer_id))
                        .flatten(),
                    action: three_state(&rule.hold_action),
                    timeout_ms: rule.hold_timeout_ms,
                    // The late tap belongs to a layer hold only.
                    retro_tap_ms: rule
                        .retro_tap_ms
                        .filter(|_| behavior == HoldBehavior::Layer),
                }
            };
            RuleDoc {
                key: RuleKeyDoc::from_key(&rule.key),
                enabled: rule.is_enabled(),
                when: WhenDoc::from_set(
                    &KeymapConditionSet {
                        game_mode: OnOff::parse(rule.condition_game_mode.as_deref()),
                        keyboard_layouts: list(&rule.condition_keyboard_layouts),
                        window_patterns: list(&rule.condition_window_patterns),
                        excluded_window_patterns: list(&rule.condition_excluded_window_patterns),
                    }
                    .normalized(),
                ),
                tap: three_state(&rule.tap_action),
                double_tap: (!rule.double_tap_action.trim().is_empty()).then(|| TimedActionDoc {
                    action: rule.double_tap_action.clone(),
                    timeout_ms: rule.double_tap_timeout_ms,
                }),
                hold: (!hold.is_empty()).then_some(hold),
                long_hold,
                tap_decision: rule.tap_decision,
            }
        })
        .collect();

    let macros = preset
        .macros
        .iter()
        .filter(|item| !item.id.is_empty())
        .map(|item| MacroDoc {
            id: item.id.clone(),
            name: name(&item.name, &item.id),
            step_pause_ms: item.step_pause_ms,
            modifier_delay_ms: item.modifier_delay_ms,
            steps: item
                .steps
                .iter()
                .filter(|step| !step.action.trim().is_empty())
                .map(|step| step.action.clone())
                .collect(),
        })
        .collect();

    let commands = preset
        .commands
        .iter()
        .filter(|command| !command.id.is_empty())
        .map(|command| CommandDoc {
            id: command.id.clone(),
            name: name(&command.name, &command.id),
            sh: text(&command.posix_script).unwrap_or_default(),
            powershell: text(&command.powershell_script).unwrap_or_default(),
            working_directory: command.working_directory.as_deref().and_then(text),
        })
        .collect();

    let quick_action_pages: Vec<PageDoc<QuickCellDoc>> = preset
        .quick_action_pages
        .iter()
        .map(|page| PageDoc {
            name: text(&page.name),
            cells: page
                .cells
                .iter()
                .filter(|(key, item)| {
                    LEFT_HAND_HOTKEYS.contains(&key.as_str()) && !item.action.trim().is_empty()
                })
                .map(|(key, item)| {
                    let cell = if item.name.trim().is_empty() {
                        QuickCellDoc::Action(item.action.clone())
                    } else {
                        QuickCellDoc::Full(QuickActionDoc {
                            name: text(&item.name),
                            action: item.action.clone(),
                        })
                    };
                    (key.clone(), cell)
                })
                .collect(),
        })
        .collect();

    let emoji_pages: Vec<PageDoc<String>> = preset
        .emoji_pages
        .iter()
        .map(|page| PageDoc {
            name: text(&page.name),
            cells: page
                .cells
                .iter()
                .filter(|(key, value)| {
                    LEFT_HAND_HOTKEYS.contains(&key.as_str()) && !value.is_empty()
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        })
        .collect();

    // A single unnamed empty page is what a missing list loads as.
    fn only_default<T>(pages: &[PageDoc<T>]) -> bool {
        matches!(pages, [page] if page.name.is_none() && page.cells.0.is_empty())
    }
    KeymapDoc {
        version: KEYMAP_SCHEMA_VERSION,
        schema: preset.schema.clone(),
        meta: {
            let meta = preset.meta.clone().unwrap_or_default();
            let meta = MetaDoc {
                name: meta.name.as_deref().and_then(text),
                author: meta.author.as_deref().and_then(text),
                keymap_version: meta.version.as_deref().and_then(text),
                description: preset.description.as_deref().and_then(text),
            };
            (!meta.is_empty()).then_some(meta)
        },
        layers,
        rules,
        macros,
        commands,
        quick_menu_pages: if only_default(&quick_action_pages) {
            Vec::new()
        } else {
            quick_action_pages
        },
        emoji_menu_pages: if only_default(&emoji_pages) {
            Vec::new()
        } else {
            emoji_pages
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn keymap(body: Value) -> String {
        let mut body = body;
        body["version"] = json!(1);
        body.to_string()
    }

    fn parsed(body: Value) -> KeymapPreset {
        parse(&keymap(body)).unwrap().unwrap()
    }

    fn round_trip(preset: &KeymapPreset) -> KeymapPreset {
        parse(&serialize(preset)).unwrap().unwrap()
    }

    #[test]
    fn every_value_has_one_spelling() {
        let preset = parsed(json!({
            "layers": [{"id": "a", "extraKeyOrder": ["f13"], "keys": {
                "keyh": "ArrowLeft", "KeyJ": "Ctrl", "KeyK": "Shift+ControlLeft"
            }}],
            "rules": [{"key": "KeyA", "when": {
                "keyboardLayouts": ["US", " us "], "windows": [" Kate ", "Kate"],
                "excludeWindows": ["Title: Foo"]
            }}]
        }));
        let saved: Value = serde_json::from_str(&serialize(&preset)).unwrap();
        assert_eq!(
            saved["layers"][0]["keys"],
            json!({"KeyH": "ArrowLeft", "KeyJ": "ControlLeft", "KeyK": "Ctrl+ShiftLeft"})
        );
        assert_eq!(saved["layers"][0]["extraKeyOrder"], json!(["F13"]));
        assert_eq!(
            saved["rules"][0]["when"],
            json!({"keyboardLayouts": ["us"], "windows": ["Kate"], "excludeWindows": ["title:Foo"]})
        );
        assert_eq!(round_trip(&preset), preset);
    }

    #[test]
    fn ambiguous_or_empty_values_are_errors() {
        for body in [
            json!({"layers": [{"id": "a", "keys": {"KeyH": "KeyA", "keyh": "KeyB"}}]}),
            json!({"layers": [{"id": "a", "extraKeyOrder": ["F13", "f13"]}]}),
            json!({"layers": [{"id": "a", "name": ""}]}),
            json!({"layers": [{"id": "a", "description": " "}]}),
            json!({"meta": {"name": ""}}),
            json!({"commands": [{"id": "c", "sh": ""}]}),
            json!({"commands": [{"id": "c", "workingDirectory": ""}]}),
            json!({"quickMenuPages": [{"name": ""}]}),
            json!({"quickMenuPages": [{"cells": {"KeyQ": {"name": "", "action": "KeyA"}}}]}),
            json!({"rules": [{"key": "KeyA", "hold": {"layer": ""}}]}),
            json!({"rules": [{"key": "KeyA", "hold": {"action": "KeyB", "retroTapMs": 5}}]}),
            json!({"rules": [{"key": "KeyA", "hold": {"retroTapMs": 5}}]}),
        ] {
            assert!(parse(&keymap(body.clone())).is_err(), "accepted {body}");
        }
        // The old prefix is an unknown action type, not an alias.
        assert_eq!(
            super::super::actions::Action::parse(Some("tecloid:showSettings")),
            super::super::actions::Action::Unknown("tecloid:showSettings".into())
        );
    }

    #[test]
    fn writer_leaves_out_what_the_reader_rejects() {
        let mut preset = parsed(json!({
            "layers": [{"id": "a"}],
            "rules": [{"key": "KeyA", "hold": {"layer": "a", "retroTapMs": 5}}],
            "commands": [{"id": "c", "sh": "true"}]
        }));
        preset.layers[0].name = String::new();
        preset.layers[0].description = Some(" ".into());
        preset.rules[0].layer_id = String::new();
        preset.rules[0].hold_action = Some("KeyB".into());
        preset.commands[0].name = String::new();
        preset.commands[0].powershell_script = " ".into();
        preset.commands[0].working_directory = Some(String::new());
        preset.meta = Some(super::super::model::KeymapMetadata {
            name: Some(String::new()),
            ..Default::default()
        });
        let saved = serialize(&preset);
        let reloaded = parse(&saved).unwrap().unwrap();
        assert_eq!(reloaded.layers[0].name, "a");
        assert_eq!(reloaded.rules[0].retro_tap_ms, None);
        assert!(!saved.contains("powershell") && !saved.contains("meta"));
    }

    #[test]
    fn timing_inherits_only_when_absent() {
        for kind in ["hold", "longHold", "doubleTap"] {
            let mut rule = json!({"key": "KeyA", kind: {"action": "KeyB"}});
            assert!(parse(&keymap(json!({"rules": [rule.clone()]}))).is_ok());
            rule[kind]["timeoutMs"] = Value::Null;
            assert!(parse(&keymap(json!({"rules": [rule]}))).is_err());
        }
        for field in ["stepPauseMs", "modifierDelayMs"] {
            assert!(parse(&keymap(json!({"macros": [{"id": "m", field: null}]}))).is_err());
        }
        let preset = parsed(json!({"rules": [{"key": "KeyA", "tap": null}]}));
        assert_eq!(round_trip(&preset), preset);
    }

    #[test]
    fn portable_metadata_and_schema_survive_round_trip() {
        let body = json!({"$schema": "https://example.com/keymap.schema.json",
            "meta": {"name": "Original", "author": "Ivan K", "keymapVersion": "1.0",
                     "description": "Portable"}});
        let preset = parsed(body.clone());
        assert_eq!(round_trip(&preset), preset);
        let saved: Value = serde_json::from_str(&serialize(&preset)).unwrap();
        assert_eq!(saved["meta"], body["meta"]);
        assert_eq!(saved["$schema"], body["$schema"]);
        for invalid in [
            json!({"$schema": null}),
            json!({"$schema": 1}),
            json!({"meta": null}),
            json!({"meta": {"author": null}}),
            json!({"meta": {"future": true}}),
            json!({"meta": {"keymapVersion": 1}}),
            json!({"meta": {"version": "1.0"}}),
            json!({"meta": {"description": null}}),
            json!({"description": "moved into meta"}),
            json!({"quickMenuPages": [{"cells": {"KeyQ": {"action": "KeyA", "icon": "x"}}}]}),
        ] {
            assert!(parse(&keymap(invalid)).is_err());
        }
    }

    #[test]
    fn duplicate_ids_are_rejected_before_conversion() {
        for field in ["layers", "macros", "commands"] {
            let item = serde_json::json!({"id": "same"});
            let text = serde_json::json!({"version": 1, field: [item.clone(), item]}).to_string();
            let error = parse(&text).unwrap_err();
            assert!(error.contains("duplicate id"), "{field}: {error}");
        }
    }

    /// `null` is accepted only where it means something (swallow, no
    /// action); elsewhere a missing field is the one spelling of "none".
    #[test]
    fn null_only_where_it_has_a_meaning() {
        for body in [
            json!({"layers": [{"id": "l", "name": null}]}),
            json!({"layers": [{"id": "l", "description": null}]}),
            json!({"rules": [{"key": "KeyA", "hold": null}]}),
            json!({"rules": [{"key": "KeyA", "doubleTap": null}]}),
            json!({"rules": [{"key": "KeyA", "longHold": null}]}),
            json!({"rules": [{"key": "KeyA", "tapDecision": null}]}),
            json!({"rules": [{"key": "KeyA", "when": {"gameMode": null}}]}),
            json!({"rules": [{"key": "KeyA", "hold": {"layer": null}}]}),
            json!({"macros": [{"id": "m", "name": null}]}),
            json!({"commands": [{"id": "c", "workingDirectory": null}]}),
            json!({"quickMenuPages": [{"name": null}]}),
            json!({"quickMenuPages": [{"cells": {"KeyQ": {"name": null, "action": "KeyA"}}}]}),
        ] {
            assert!(parse(&keymap(body.clone())).is_err(), "accepted {body}");
        }
        let preset = parsed(json!({
            "layers": [{"id": "l", "keys": {"KeyA": null}}],
            "rules": [{"key": "KeyA", "tap": null, "hold": {"action": null}}],
        }));
        assert_eq!(preset.rules[0].tap_action, None);
    }

    #[test]
    fn key_actions_are_stored_canonically() {
        let preset = parsed(json!({
            "layers": [{"id": "l", "keys": {"KeyA": "shift+ctrl+keyc"}}],
            "rules": [{"key": "capslock", "tap": "ControlLeft+KeyV"}],
            "macros": [{"id": "m", "steps": ["alt+Tab", "text:shift+a"]}],
        }));
        assert_eq!(
            preset.layer_keymaps["l"].keys["KeyA"].as_deref(),
            Some("Ctrl+Shift+KeyC")
        );
        assert_eq!(preset.rules[0].key, "CapsLock");
        assert_eq!(preset.rules[0].tap_action.as_deref(), Some("Ctrl+KeyV"));
        assert_eq!(preset.macros[0].steps[0].action, "Alt+Tab");
        assert_eq!(preset.macros[0].steps[1].action, "text:shift+a");
    }

    #[test]
    fn newer_object_forms_point_at_a_newer_version() {
        let error = parse(&keymap(json!({
            "layers": [{"id": "l", "keys": {"KeyA": {"action": "KeyB", "future": 1}}}]
        })))
        .unwrap_err();
        assert!(error.contains("newer version"), "{error}");
    }

    /// Rules and menu pages are not referenced by id, so the file has none.
    #[test]
    fn rules_and_pages_store_no_ids() {
        for body in [
            json!({"rules": [{"id": "r", "key": "KeyA"}]}),
            json!({"quickMenuPages": [{"id": "q"}]}),
            json!({"emojiMenuPages": [{"id": "e"}]}),
        ] {
            assert!(parse(&keymap(body.clone())).is_err(), "accepted {body}");
        }
        let text = r#"{"version":1,"rules":[{"key":"KeyA"},{"key":"KeyB"}],
            "quickMenuPages":[{}, {}],"emojiMenuPages":[{}, {}]}"#;
        let keymap = parse(text).unwrap().unwrap();
        super::super::ids::validate_keymap(&keymap).unwrap();
        assert_eq!(keymap.rules[1].id, "rule-2");
        assert_eq!(keymap.quick_action_pages[1].id, "quick-2");
        assert_eq!(keymap.emoji_pages[1].id, "emoji-2");
        let saved = serialize(&keymap);
        assert!(!saved.contains("\"id\""));
        assert_eq!(parse(&saved).unwrap().unwrap(), keymap);
    }

    #[test]
    fn rule_and_macro_timing_boundaries() {
        for (field, range) in [
            ("hold", super::super::timing::TAP_HOLD),
            ("doubleTap", super::super::timing::TAP_HOLD),
            ("longHold", super::super::timing::LONG_HOLD),
        ] {
            for value in [
                0,
                range.min - 1,
                range.min,
                range.max,
                range.max + 1,
                u64::MAX,
            ] {
                let timed = if field == "hold" {
                    serde_json::json!({"timeoutMs":value})
                } else {
                    serde_json::json!({"action":"KeyB", "timeoutMs":value})
                };
                let text = serde_json::json!({"version":1,"rules":[{"key":"KeyA",field:timed}]})
                    .to_string();
                assert_eq!(
                    parse(&text).is_ok(),
                    range.contains(value),
                    "{field}: {value}"
                );
            }
        }
        for field in ["stepPauseMs", "modifierDelayMs"] {
            for value in [0, 2000, 2001, u64::MAX] {
                let text =
                    serde_json::json!({"version":1,"macros":[{"id":"m",field:value}]}).to_string();
                assert_eq!(parse(&text).is_ok(), value <= 2000);
            }
        }
    }

    #[test]
    fn version_is_required_and_checked() {
        assert!(parse(r#"{"rules": []}"#).unwrap_err().contains("version"));
        assert!(parse(r#"{"schemaVersion": 1}"#).is_err());
        assert!(
            parse(r#"{"version": 2, "rules": []}"#)
                .unwrap_err()
                .contains("newer")
        );
        assert!(parse(r#"{"version": "1"}"#).is_err());
        assert!(parse(r#"{"version": 1}"#).unwrap().is_some());
        assert_eq!(
            serde_json::from_str::<Value>(&serialize(&KeymapPreset::default())).unwrap()["version"],
            1
        );
    }

    #[test]
    fn unknown_fields_and_values_are_rejected() {
        for body in [
            json!({"extra": true}),
            json!({"rules": [{"key": "KeyA", "holdMs": 100}]}),
            json!({"rules": [{"key": "KeyA", "hold": {"layer": "x", "isolate": []}}]}),
            json!({"rules": [{"key": "KeyA", "when": {"keymaps": ["us"]}}]}),
            json!({"rules": [{"key": "KeyA", "when": {"gameMode": "maybe"}}]}),
            json!({"rules": [{"key": "KeyA", "tapDecision": "sometimes"}]}),
            json!({"rules": [{"key": "KeyA", "tap": 5}]}),
            json!({"rules": [{"key": "KeyA", "tap": ""}]}),
            json!({"rules": [{"key": ""}]}),
            json!({"rules": [{"key": "KeyA", "hold": {"layer": "x"}, "longHold": {"action": "KeyB"}}]}),
            json!({"layers": [{"id": "nav", "keys": {"KeyA": ""}}]}),
            json!({"rules": [{"key": "KeyA", "hold": {"layer": "x", "action": "AltLeft", "keysOnlyFor": ["Tab"]}}]}),
            json!({"rules": [{"key": "KeyA", "hold": {"layer": "x", "keys": "AltLeft"}}]}),
            json!({"layers": [{"id": "nav", "keys": {"Tab": "Alt+Tab"}, "holdModifiers": ["Tab"]}]}),
            json!({"layers": [{"id": "nav", "keys": {"Tab": {"holdModifiers": true}}}]}),
            json!({"layers": [{"id": "nav", "keys": {"Tab": {"action": null, "holdModifiers": true}}}]}),
            json!({"layers": [{"id": "nav", "keys": {"Tab": {"action": " "}}}]}),
            json!({"layers": [{"id": "nav", "keys": {"Tab": {"action": "Alt+Tab", "repeat": true}}}]}),
            json!({"commands": [{"id": "c", "unix": "ls"}]}),
            json!({"commands": [{"id": "c", "windows": "dir"}]}),
            json!({"macros": [{"id": "m", "steps": [{"action": "KeyA"}]}]}),
            json!({"macros": [{"id": "m", "steps": [" "]}]}),
            json!({"commands": [{"id": "c", "linux": "ls"}]}),
            json!({"quickMenuPages": [{"cells": {"KeyP": "KeyA"}}]}),
            json!({"quickMenuPages": [{"cells": {"KeyQ": {"name": "No action"}}}]}),
            json!({"emojiMenuPages": [{"cells": {"KeyQ": 5}}]}),
        ] {
            assert!(parse(&keymap(body.clone())).is_err(), "accepted {body}");
        }
        let error = parse(&keymap(json!({"extra": true}))).unwrap_err();
        assert!(error.contains("newer version"), "{error}");
    }

    #[test]
    fn rule_triggers_are_kept_as_written() {
        let preset = parsed(json!({"rules": [
            {"key": ["Ctrl", "KeyA"]}, {"key": ["ShiftLeft", "ControlRight"]}, {"key": "NoSuchKey"},
        ]}));
        let keys: Vec<_> = preset.rules.iter().map(|rule| rule.key.as_str()).collect();
        assert_eq!(keys, ["Ctrl+KeyA", "ShiftLeft+ControlRight", "NoSuchKey"]);
        let written = serde_json::to_value(to_doc(&preset)).unwrap();
        assert_eq!(
            written["rules"][1]["key"],
            json!(["ShiftLeft", "ControlRight"])
        );
        assert_eq!(written["rules"][2]["key"], json!("NoSuchKey"));
    }

    #[test]
    fn rule_chords_are_arrays() {
        for key in [
            json!("ShiftLeft+ControlRight"),
            json!(["KeyA"]),
            json!([]),
            json!(["KeyA", ""]),
            json!(["KeyA", "KeyB+KeyC"]),
        ] {
            let body = json!({"rules": [{"key": key}]});
            assert!(parse(&keymap(body.clone())).is_err(), "accepted {body}");
        }
    }

    #[test]
    fn rule_fields_map_to_the_model() {
        let preset = parsed(json!({"rules": [
            {"key": "CapsLock", "tap": "Escape",
             "hold": {"layer": "nav", "action": "AltLeft", "timeoutMs": 180},
             "doubleTap": {"action": "Enter", "timeoutMs": 150},
             "tapDecision": "holdOnOtherKeyPress"},
            {"key": "Space", "enabled": false, "tap": null, "hold": {"action": null},
             "when": {"gameMode": "on", "keyboardLayouts": ["us"], "windows": ["code"], "excludeWindows": ["title:x"]}},
            {"key": ["ShiftLeft", "ControlRight"], "longHold": {"action": "text:hello", "timeoutMs": 1500}},
        ]}));
        let caps = &preset.rules[0];
        assert_eq!(caps.hold_behavior(), HoldBehavior::Layer);
        assert_eq!(caps.layer_id, "nav");
        assert_eq!(caps.tap_action.as_deref(), Some("Escape"));
        assert_eq!(caps.hold_action.as_deref(), Some("AltLeft"));
        assert_eq!(caps.hold_timeout_ms, Some(180));
        assert_eq!(caps.double_tap_action, "Enter");
        assert_eq!(caps.double_tap_timeout_ms, Some(150));
        assert_eq!(caps.tap_decision, Some(TapDecision::HoldOnOtherKeyPress));
        let space = &preset.rules[1];
        assert!(!space.is_enabled());
        assert_eq!(space.tap_action, None);
        assert_eq!(space.hold_action, None);
        assert_eq!(space.hold_behavior(), HoldBehavior::None);
        assert_eq!(space.condition_game_mode.as_deref(), Some("on"));
        assert_eq!(
            space.condition_keyboard_layouts,
            Some(vec!["us".to_string()])
        );
        assert_eq!(
            space.condition_window_patterns,
            Some(vec!["code".to_string()])
        );
        assert_eq!(
            space.condition_excluded_window_patterns,
            Some(vec!["title:x".to_string()])
        );
        let chord = &preset.rules[2];
        assert_eq!(chord.hold_behavior(), HoldBehavior::Action);
        assert_eq!(chord.long_hold_action, "text:hello");
        assert_eq!(chord.long_hold_timeout_ms, Some(1500));
        assert_eq!(chord.hold_action.as_deref(), Some(""));
        assert_eq!(round_trip(&preset), preset);
        // The mapper reads the model JSON.
        let mapper: crate::mapper::config::Rule =
            serde_json::from_value(serde_json::to_value(&preset.rules[0]).unwrap()).unwrap();
        assert_eq!(mapper.tap_decision, Some(TapDecision::HoldOnOtherKeyPress));
    }

    #[test]
    fn empty_layer_entry_is_saved_as_transparent_and_names_equal_to_ids_are_omitted() {
        let mut preset = parsed(
            json!({"layers": [{"id": "nav", "keys": {"KeyH": "ArrowLeft"}}],
            "macros": [{"id": "m", "steps": ["KeyA"]}]}),
        );
        let keymap = preset.layer_keymaps.get_mut("nav").unwrap();
        keymap.keys.insert("KeyJ".into(), Some(String::new()));
        keymap.keys.insert("KeyK".into(), None);
        let saved: Value = serde_json::from_str(&serialize(&preset)).unwrap();
        assert_eq!(
            saved["layers"][0],
            json!({"id": "nav", "keys": {"KeyH": "ArrowLeft", "KeyK": null}})
        );
        assert_eq!(saved["macros"][0], json!({"id": "m", "steps": ["KeyA"]}));
    }

    #[test]
    fn only_the_selected_hold_behaviour_is_saved() {
        let mut rule = KeyRule::new("r1".into(), "KeyA");
        rule.layer_id = "nav".into();
        rule.long_hold_action = "KeyB".into();
        rule.hold_behavior = Some(HoldBehavior::Layer);
        let preset = KeymapPreset {
            rules: vec![rule],
            ..KeymapPreset::default()
        };
        let saved: Value = serde_json::from_str(&serialize(&preset)).unwrap();
        assert_eq!(saved["rules"][0]["hold"]["layer"], "nav");
        assert!(saved["rules"][0].get("longHold").is_none());
        let again = round_trip(&preset);
        assert_eq!(again.rules[0].hold_behavior(), HoldBehavior::Layer);
        assert_eq!(again.rules[0].long_hold_action, "");
    }

    #[test]
    fn layers_macros_commands_and_menus_round_trip() {
        let preset = parsed(json!({
            "meta": {"description": "Sample"},
            "layers": [
                {"id": "nav", "name": "Navigation", "description": "Arrows",
                 "extraKeyOrder": ["F15", "F13"],
                 "keys": {"KeyH": "ArrowLeft", "KeyQ": null,
                          "Tab": {"action": "Alt+Tab", "holdModifiers": true}}},
                {"id": "sym"}
            ],
            "macros": [{"id": "copyLine", "name": "Copy line", "stepPauseMs": 10,
                        "steps": ["Home", "Shift+End", "Ctrl+KeyC"]}],
            "commands": [{"id": "music", "sh": "playerctl play-pause", "powershell": "Start-Process x",
                          "workingDirectory": "/tmp"}],
            "quickMenuPages": [{"name": "First", "cells": {
                "KeyW": "Ctrl+KeyC",
                "KeyQ": {"name": "Lock", "action": "kde:ksmserver/Lock Session"}
            }}, {}],
            "emojiMenuPages": [{"name": "Faces", "cells": {"KeyQ": "😀"}}]
        }));
        assert_eq!(preset.description.as_deref(), Some("Sample"));
        assert_eq!(preset.layers[1].name, "sym");
        let nav = &preset.layer_keymaps["nav"];
        assert_eq!(nav.keys["KeyH"].as_deref(), Some("ArrowLeft"));
        assert_eq!(nav.keys["KeyQ"], None);
        assert_eq!(nav.extra_key_order, ["F15", "F13"]);
        assert_eq!(nav.hold_modifiers, BTreeSet::from(["Tab".to_owned()]));
        assert_eq!(preset.macros[0].steps.len(), 3);
        assert_eq!(preset.commands[0].posix_script, "playerctl play-pause");
        assert_eq!(preset.commands[0].powershell_script, "Start-Process x");
        let quick = &preset.quick_action_pages[0];
        assert_eq!(quick.cells["KeyQ"].name, "Lock");
        assert_eq!(quick.cells["KeyW"].name, "");
        let again = round_trip(&preset);
        assert_eq!(again.layers, preset.layers);
        assert_eq!(again.layer_keymaps, preset.layer_keymaps);
        assert_eq!(again.commands, preset.commands);
        assert_eq!(again.quick_action_pages, preset.quick_action_pages);
        assert_eq!(again.emoji_pages, preset.emoji_pages);
        let text = serialize(&preset);
        assert!(
            text.find("\"KeyQ\": {").unwrap() < text.find("\"KeyW\": \"Ctrl+KeyC\"").unwrap(),
            "cells are written in grid order"
        );
        assert!(
            text.find("\"Tab\": {").unwrap() < text.find("\"KeyQ\": null").unwrap()
                && text.find("\"KeyQ\": null").unwrap() < text.find("\"KeyH\"").unwrap(),
            "layer keys are written in keyboard order"
        );
    }

    #[test]
    fn transient_editor_entries_are_not_saved() {
        let mut rule = KeyRule::new("r1".into(), "");
        rule.tap_action = Some("KeyA".into());
        let mut preset = KeymapPreset::initial();
        preset.rules.push(rule);
        preset.macros.push(Macro {
            id: "m".into(),
            name: "m".into(),
            steps: vec![MacroStep {
                id: "s".into(),
                action: " ".into(),
            }],
            step_pause_ms: None,
            modifier_delay_ms: None,
        });
        // A key that lost its action is not saved with holdModifiers.
        preset.layers.push(Layer {
            id: "nav".into(),
            name: "nav".into(),
            description: None,
        });
        let keymap = preset.get_or_create_layer_keymap("nav");
        keymap.keys.insert("KeyQ".into(), None);
        keymap.hold_modifiers.insert("KeyQ".into());
        keymap.hold_modifiers.insert("Tab".into());
        let again = round_trip(&preset);
        assert!(again.rules.is_empty());
        assert!(again.macros[0].steps.is_empty());
        assert!(again.layer_keymaps["nav"].hold_modifiers.is_empty());
    }

    #[test]
    fn missing_menu_pages_get_one_empty_page_and_are_not_written_back() {
        let preset = parsed(json!({}));
        assert_eq!(preset.emoji_pages, vec![EmojiPage::default_page()]);
        assert_eq!(
            preset.quick_action_pages,
            vec![QuickActionPage::default_page()]
        );
        assert_eq!(serialize(&preset), "{\n  \"version\": 1\n}");
        assert_eq!(parsed(json!({"emojiMenuPages": []})), preset);
    }

    #[test]
    fn parsing_is_deterministic_without_ids() {
        let text = keymap(
            json!({"rules": [{"key": "CapsLock"}], "quickMenuPages": [{"cells": {"KeyQ": "KeyA"}}]}),
        );
        assert_eq!(parse(&text), parse(&text));
    }

    #[test]
    fn empty_and_invalid_documents() {
        assert_eq!(parse("").unwrap(), None);
        assert!(parse(r#""just text""#).is_err());
        assert!(parse("{\"layers\": [").is_err());
        assert!(parse("layers: []\nrules: []\n").is_err());
    }

    /// Each value has one spelling: no repeated chord keys, no empty
    /// `hold`, menu hotkeys in their catalog case, ids unique ignoring case,
    /// and empty lists are not written.
    #[test]
    fn single_spelling_rules() {
        let load = |body: &str| parse(&format!(r#"{{"version":1,{body}}}"#));
        for bad in [
            r#""rules":[{"key":["KeyA","keya"],"tap":"KeyB"}]"#,
            r#""rules":[{"key":"KeyA","hold":{}}]"#,
            r#""quickMenuPages":[{"cells":{"KeyQ":"KeyA","keyq":"KeyB"}}]"#,
            r#""emojiMenuPages":[{"cells":{"KeyP":"x"}}]"#,
            r#""layers":[{"id":"Nav"},{"id":"nav"}]"#,
            r#""macros":[{"id":"M1"},{"id":"m1"}]"#,
        ] {
            assert!(load(bad).is_err(), "accepted {bad}");
        }
        let keymap = load(
            r#""quickMenuPages":[{},{"cells":{"keyq":"KeyA"}}],"emojiMenuPages":[{"cells":{"keyw":"x"}}],"macros":[{"id":"m","steps":[]}]"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(keymap.quick_action_pages[1].cells["KeyQ"].action, "KeyA");
        assert_eq!(keymap.emoji_pages[0].cells["KeyW"], "x");
        let saved: serde_json::Value = serde_json::from_str(&serialize(&keymap)).unwrap();
        assert_eq!(saved["quickMenuPages"][0], serde_json::json!({}));
        assert_eq!(saved["macros"][0], serde_json::json!({"id": "m"}));
    }
}
