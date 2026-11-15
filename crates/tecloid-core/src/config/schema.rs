//! Schema versions of `config.json` and keymap files.
//!
//! The two formats are versioned independently. Within a version only new
//! optional fields and new enum values may appear; any other change bumps
//! that format's version. To introduce version N + 1 of a format:
//!
//! 1. move the current `*Doc` structs into a `vN` submodule next to the
//!    format's reader and add the new ones;
//! 2. bump the constant below;
//! 3. add a `N => …` arm to the reader's version `match` that parses the
//!    old structs and converts them to the domain model (or to the new
//!    structs);
//! 4. publish `schemas/<format>.v<N+1>.schema.json` next to the old one and
//!    point the URL constant below at it.
//!
//! Within a version, regenerate the published schemas after adding a field
//! (see `published_schemas_match_the_readers`).
//!
//! The application always writes the current version. Before a file with
//! an older version is first overwritten, the storage keeps a copy of it
//! next to the original (see [`backup_path`]).

/// Version of `config.json` this build reads and writes.
pub const CONFIG_SCHEMA_VERSION: u64 = 1;

/// Version of keymap files this build reads and writes.
pub const KEYMAP_SCHEMA_VERSION: u64 = 1;

/// Stable address of the `config.json` JSON Schema for editors (`$schema`).
/// The file lives in the repository's `schemas/` directory; within a format
/// version it only gains optional fields and enum values.
pub const CONFIG_SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/bozonx/tecloid/main/schemas/config.v1.schema.json";

/// Stable address of the keymap file JSON Schema, see [`CONFIG_SCHEMA_URL`].
pub const KEYMAP_SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/bozonx/tecloid/main/schemas/keymap.v1.schema.json";

/// Grammar of collection IDs (layers, macros and commands), see [`super::ids`].
pub const ID_PATTERN: &str = "^[A-Za-z0-9_-]{1,64}$";

/// The `version` of a parsed document, checked before the rest is
/// read, so a file from a newer build is reported as such rather than as a
/// list of unknown fields. Returns a version in `1..=current`.
pub(crate) fn read_version(
    value: &serde_json::Value,
    what: &str,
    current: u64,
) -> Result<u64, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{what} must be a JSON object"))?;
    let Some(version) = object.get("version") else {
        return Err(format!("{what}: missing \"version\" field"));
    };
    match version.as_u64() {
        Some(version) if (1..=current).contains(&version) => Ok(version),
        Some(newer) if newer > current => Err(format!(
            "{what}: schema version {newer} is newer than this application supports ({current})"
        )),
        _ => Err(format!("{what}: unsupported schema version {version}")),
    }
}

/// `version` of a file's text, if it is a JSON object with one.
pub(crate) fn version_of(text: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(text.strip_prefix('\u{feff}').unwrap_or(text))
        .ok()?
        .get("version")?
        .as_u64()
}

/// A strict-read error. A version may gain optional fields and enum
/// values, which an older build reports as unknown, so such errors point at
/// the likely cause.
pub(crate) fn read_error(what: &str, error: &serde_json::Error) -> String {
    let text = error.to_string();
    if text.starts_with("unknown field")
        || text.starts_with("unknown variant")
        || text.starts_with("unsupported paste shortcut")
        // An object form with a field from a newer version (layer keys,
        // menu cells) matches neither the string nor the known object form.
        || text.starts_with("data did not match any variant")
    {
        format!("{what}: {text} (the file may come from a newer version of the application)")
    } else {
        format!("{what}: {text}")
    }
}

/// Where the copy of `path` with schema `version` is kept before the file
/// is upgraded: `config.json` → `config.json.v1.bak`.
pub fn backup_path(path: &std::path::Path, version: u64) -> std::path::PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(format!(".v{version}.bak"));
    path.with_file_name(name)
}

/// An optional text field: absent, or a string that is not blank. `null`
/// and `""` are errors, so "no value" has one spelling: a missing field.
pub(crate) fn text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    filled(deserializer).map(Some)
}

/// A text field with a default: when present, it is not blank.
pub(crate) fn filled<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    if value.trim().is_empty() {
        return Err(serde::de::Error::custom(
            "text must not be empty; omit the field instead",
        ));
    }
    Ok(value)
}

/// Read an optional field whose absence is allowed but explicit null is not.
pub(crate) fn non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn accepts_every_version_up_to_the_current_one() {
        let read = |value, current| read_version(&value, "file", current);
        assert_eq!(read(json!({"version": 1}), 1), Ok(1));
        assert_eq!(read(json!({"version": 1}), 3), Ok(1));
        assert_eq!(read(json!({"version": 3}), 3), Ok(3));
        assert!(
            read(json!({"version": 2}), 1)
                .unwrap_err()
                .contains("newer")
        );
        assert!(
            read(json!({"version": 0}), 1)
                .unwrap_err()
                .contains("unsupported")
        );
        assert!(read(json!({"version": "1"}), 1).is_err());
        assert!(read(json!({}), 1).unwrap_err().contains("missing"));
        assert!(read(json!([]), 1).unwrap_err().contains("JSON object"));
    }

    #[test]
    fn reads_the_version_of_raw_text() {
        assert_eq!(version_of(r#"{"version": 4, "x": 1}"#), Some(4));
        assert_eq!(version_of(r#"{"x": 1}"#), None);
        assert_eq!(version_of("not json"), None);
    }

    /// The published schemas match the readers. After a format change run
    /// `TECLOID_UPDATE_SCHEMAS=1 cargo test -p tecloid-core published_schemas` and
    /// commit the files.
    #[test]
    fn published_schemas_match_the_readers() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        let update = std::env::var_os("TECLOID_UPDATE_SCHEMAS").is_some();
        for (file, url, title, schema) in [
            (
                "config.v1.schema.json",
                CONFIG_SCHEMA_URL,
                "Tecloid config.json",
                super::super::settings::json_schema(),
            ),
            (
                "keymap.v1.schema.json",
                KEYMAP_SCHEMA_URL,
                "Tecloid keymap",
                super::super::keymap_file::json_schema(),
            ),
        ] {
            assert!(url.ends_with(file));
            let mut value = schema.to_value();
            let object = value.as_object_mut().unwrap();
            object.insert("$id".into(), url.into());
            object.insert("title".into(), title.into());
            let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
            let path = dir.join(file);
            if update {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&path, &text).unwrap();
            }
            let published = std::fs::read_to_string(&path).unwrap_or_default();
            assert!(
                published == text,
                "{file} is out of date; regenerate it with TECLOID_UPDATE_SCHEMAS=1"
            );
        }
    }

    /// Every name a user file may store besides field names and enum values
    /// (those are covered by the published schemas): action prefixes, `sys:`,
    /// `app:` and `builtin:` ids, key names and trigger modifiers.
    fn stored_names() -> Vec<String> {
        use super::super::{actions, key_catalog};
        let mut names = Vec::new();
        for prefix in [
            "app:", "macro:", "builtin:", "cmd:", "sys:", "kde:", "xdg:", "text:", "pause:",
        ] {
            let parsed = actions::Action::parse(Some(&format!("{prefix}1")));
            assert!(
                !matches!(
                    parsed,
                    actions::Action::Unknown(_) | actions::Action::Keys(_)
                ),
                "{prefix}"
            );
            names.push(format!("prefix {prefix}"));
        }
        for entry in actions::system_actions()
            .into_iter()
            .chain(actions::app_actions())
        {
            let action = entry.action.format().unwrap_or_default();
            names.push(format!("action {action}"));
        }
        // Numbered families accept every N in 1..=MAX_SLOT; the enumerated
        // lines above are only the picker defaults.
        for family in actions::NUMBERED_SYSTEM_ACTIONS {
            names.push(format!("action sys:{family}<1..={}>", actions::MAX_SLOT));
        }
        for family in actions::NUMBERED_APP_ACTIONS {
            names.push(format!("action app:{family}<1..={}>", actions::MAX_SLOT));
        }
        for family in actions::NUMBERED_SYSTEM_ACTIONS {
            let max = format!("{family}{}", actions::MAX_SLOT);
            assert!(actions::system_action_name(&max).is_some(), "{max}");
        }
        for family in actions::NUMBERED_APP_ACTIONS {
            let max = format!("{family}{}", actions::MAX_SLOT);
            assert!(actions::app_action_name(&max).is_some(), "{max}");
        }
        for builtin in crate::mapper::builtin_macros::BUILTIN_MACROS {
            names.push(format!("action builtin:{}", builtin.id));
        }
        for key in key_catalog::CATEGORIES.iter().flat_map(|keys| keys.iter()) {
            names.push(format!("key {key}"));
        }
        for modifier in key_catalog::TRIGGER_MODIFIERS {
            names.push(format!("trigger {modifier}"));
        }
        names.push("window title:".into());
        names.push(format!(
            "keymap {}",
            super::super::model::USER_KEYMAP_PREFIX
        ));
        names.sort();
        names.dedup();
        names
    }

    /// Names in user files are data: a released name is never renamed or
    /// removed. Adding one updates `names.v1.txt` (run with
    /// `TECLOID_UPDATE_SCHEMAS=1`); a missing line means a name was dropped.
    #[test]
    fn stored_names_are_never_removed() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/names.v1.txt");
        let current = stored_names();
        let published = std::fs::read_to_string(&path).unwrap_or_default();
        let removed: Vec<_> = published
            .lines()
            .filter(|line| !line.is_empty() && !current.iter().any(|name| name == line))
            .collect();
        assert!(
            removed.is_empty(),
            "names stored in user files were removed or renamed: {removed:?}"
        );
        let text = current.join("\n") + "\n";
        if std::env::var_os("TECLOID_UPDATE_SCHEMAS").is_some() {
            std::fs::write(&path, &text).unwrap();
        }
        assert!(
            std::fs::read_to_string(&path).unwrap_or_default() == text,
            "names.v1.txt is out of date; regenerate it with TECLOID_UPDATE_SCHEMAS=1"
        );
    }

    /// Complete files of format version 1. They are never edited: every
    /// later build must load them with the same meaning.
    #[test]
    fn version_1_fixtures_keep_loading() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let config = std::fs::read_to_string(dir.join("config.v1.json")).unwrap();
        let settings = super::super::settings::parse(&config).unwrap();
        assert_eq!(
            super::super::settings::parse(&super::super::settings::serialize(&settings)).unwrap(),
            settings
        );
        let keymap = std::fs::read_to_string(dir.join("keymap.v1.json")).unwrap();
        let keymap = super::super::keymap_file::parse(&keymap).unwrap().unwrap();
        assert_eq!(
            super::super::keymap_file::parse(&super::super::keymap_file::serialize(&keymap))
                .unwrap()
                .unwrap(),
            keymap
        );
        let resolved = super::super::model::ResolvedAppConfig::from_parts(settings, keymap, None);
        let blocking: Vec<_> = super::super::diagnostics::analyze(&resolved)
            .into_iter()
            .filter(|issue| issue.code.is_error())
            .collect();
        assert!(blocking.is_empty(), "{blocking:?}");
    }

    /// Compare `text` with the committed file at `path`; with
    /// `TECLOID_UPDATE_SCHEMAS=1` write it first.
    fn snapshot(path: &Path, text: &str) {
        if std::env::var_os("TECLOID_UPDATE_SCHEMAS").is_some() {
            std::fs::write(path, text).unwrap();
        }
        assert!(
            std::fs::read_to_string(path).unwrap_or_default() == text,
            "{} is out of date; review the change, then regenerate it with TECLOID_UPDATE_SCHEMAS=1",
            path.display()
        );
    }

    /// Steps of every `builtin:` macro. Ids are frozen in `names.v1.txt`;
    /// the steps are what the id does today. Changing them is a deliberate
    /// behaviour change (a fix, another platform's shortcut): regenerate
    /// the file and add a `CHANGELOG.md` entry.
    #[test]
    fn builtin_macro_steps_are_reviewed() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/builtin-macros.v1.txt");
        let mut lines: Vec<String> = crate::mapper::builtin_macros::BUILTIN_MACROS
            .iter()
            .map(|item| format!("{}: {}", item.id, item.steps.join(" | ")))
            .collect();
        lines.sort();
        snapshot(&path, &(lines.join("\n") + "\n"));
    }

    /// Size limits and numeric bounds outside the schemas (see
    /// [`super::super::limits`]): a `max` only grows, a `min` only shrinks.
    #[test]
    fn limits_only_widen() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/limits.v1.txt");
        let current = super::super::limits::published();
        let value = |line: &str| -> Option<(String, u64)> {
            let (key, number) = line.rsplit_once(' ')?;
            Some((key.to_owned(), number.parse().ok()?))
        };
        let current_values: Vec<_> = current.iter().filter_map(|line| value(line)).collect();
        let published = std::fs::read_to_string(&path).unwrap_or_default();
        let narrowed: Vec<_> = published
            .lines()
            .filter_map(value)
            .filter(|(key, old)| {
                let new = current_values
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, new)| *new);
                match new {
                    None => true,
                    Some(new) if key.starts_with("max ") => new < *old,
                    Some(new) => new > *old,
                }
            })
            .collect();
        assert!(
            narrowed.is_empty(),
            "limits of the file formats were removed or narrowed: {narrowed:?}"
        );
        snapshot(&path, &(current.join("\n") + "\n"));
    }

    /// The fixtures saved again: every value in its stored spelling. A diff
    /// here means a version 1 file now reads or writes differently.
    #[test]
    fn version_1_fixtures_save_unchanged() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let config = std::fs::read_to_string(dir.join("config.v1.json")).unwrap();
        let config = super::super::settings::parse(&config).unwrap();
        snapshot(
            &dir.join("config.v1.saved.json"),
            &(super::super::settings::serialize(&config) + "\n"),
        );
        let keymap = std::fs::read_to_string(dir.join("keymap.v1.json")).unwrap();
        let keymap = super::super::keymap_file::parse(&keymap).unwrap().unwrap();
        snapshot(
            &dir.join("keymap.v1.saved.json"),
            &(super::super::keymap_file::serialize(&keymap) + "\n"),
        );
    }

    /// Defaults are written into every new `config.json`, and input
    /// defaults change only as a documented behaviour change. Game-mode
    /// detectors differ per OS, so the snapshot is the Linux one.
    #[cfg(target_os = "linux")]
    #[test]
    fn config_defaults_are_frozen() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let defaults = super::super::settings::parse(r#"{"version": 1}"#).unwrap();
        snapshot(
            &dir.join("config.v1.defaults.json"),
            &(super::super::settings::serialize(&defaults) + "\n"),
        );
    }

    /// What the contract records for a schema node, keyed by its JSON
    /// pointer (`config/$defs/TextMode`) plus a suffix:
    ///
    /// * `…/properties` — field names; enum values under the bare pointer;
    ///   `…/type` — JSON types: these only grow;
    /// * `…/required` — required fields: this list only shrinks;
    /// * `…/minimum`, `…/maximum`, `…/minLength`, `…/maxLength`,
    ///   `…/minItems`, `…/maxItems` — limits: they only widen;
    /// * `…/pattern` — grammar of a string (ids): a change needs review.
    fn stored_fields_and_values() -> serde_json::Map<String, serde_json::Value> {
        use serde_json::Value;
        /// Enum values of a node: `enum` lists and `const`s under `oneOf` /
        /// `anyOf` (documented variants are written as `const`).
        fn values(node: &Value, out: &mut Vec<Value>) {
            if let Some(list) = node.get("enum").and_then(Value::as_array) {
                out.extend(list.iter().cloned());
            }
            if let Some(value) = node.get("const") {
                out.push(value.clone());
            }
            for key in ["oneOf", "anyOf"] {
                for item in node
                    .get(key)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    values(item, out);
                }
            }
        }
        fn walk(node: &Value, path: &str, out: &mut serde_json::Map<String, Value>) {
            let Some(object) = node.as_object() else {
                return;
            };
            if let Some(properties) = object.get("properties").and_then(Value::as_object) {
                let mut names: Vec<_> = properties.keys().cloned().collect();
                names.sort();
                out.insert(format!("{path}/properties"), names.into());
                for (name, property) in properties {
                    walk(property, &format!("{path}/properties/{name}"), out);
                }
            }
            let mut found = Vec::new();
            values(node, &mut found);
            let fixed = path.ends_with("$schema") || path.ends_with("/version");
            if !found.is_empty() && !fixed {
                out.insert(path.to_owned(), found.into());
            }
            if let Some(required) = object.get("required").and_then(Value::as_array) {
                let mut names: Vec<_> = required.clone();
                names.sort_by_key(|name| name.to_string());
                out.insert(format!("{path}/required"), names.into());
            }
            if !fixed {
                match object.get("type") {
                    Some(Value::String(kind)) => {
                        out.insert(format!("{path}/type"), vec![kind.clone()].into());
                    }
                    Some(Value::Array(kinds)) => {
                        out.insert(format!("{path}/type"), kinds.clone().into());
                    }
                    _ => {}
                }
            }
            for limit in [
                "minimum",
                "maximum",
                "minLength",
                "maxLength",
                "minItems",
                "maxItems",
                "pattern",
            ] {
                if let Some(value) = object.get(limit) {
                    out.insert(format!("{path}/{limit}"), vec![value.clone()].into());
                }
            }
            for key in ["items", "propertyNames"] {
                if let Some(child) = object.get(key) {
                    walk(child, &format!("{path}/{key}"), out);
                }
            }
            if let Some(defs) = object.get("$defs").and_then(Value::as_object) {
                for (name, def) in defs {
                    walk(def, &format!("{path}/$defs/{name}"), out);
                }
            }
        }
        let mut out = serde_json::Map::new();
        walk(
            &super::super::settings::json_schema().to_value(),
            "config",
            &mut out,
        );
        walk(
            &super::super::keymap_file::json_schema().to_value(),
            "keymap",
            &mut out,
        );
        out
    }

    /// Field names and enum values in user files are data: a released one
    /// is never renamed or removed. Adding one updates `contract.v1.json`
    /// (run with `TECLOID_UPDATE_SCHEMAS=1`).
    #[test]
    fn frozen_fields_and_enum_values_remain() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/contract.v1.json");
        let current = stored_fields_and_values();
        let published: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        let number = |value: Option<&serde_json::Value>| {
            value
                .and_then(|v| v.as_array())
                .and_then(|v| v.first())
                .and_then(serde_json::Value::as_f64)
        };
        let mut removed = Vec::new();
        for (pointer, expected) in published.as_object().into_iter().flatten() {
            let actual = current.get(pointer);
            let lower = ["/minimum", "/minLength", "/minItems"]
                .iter()
                .any(|suffix| pointer.ends_with(suffix));
            let upper = ["/maximum", "/maxLength", "/maxItems"]
                .iter()
                .any(|suffix| pointer.ends_with(suffix));
            if lower || upper {
                let (Some(old), new) = (number(Some(expected)), number(actual)) else {
                    continue;
                };
                let narrowed = match new {
                    None => false,
                    Some(new) if lower => new > old,
                    Some(new) => new < old,
                };
                if narrowed {
                    removed.push(format!("{pointer}: {old} narrowed to {new:?}"));
                }
                continue;
            }
            let actual = actual.and_then(|v| v.as_array());
            for value in expected.as_array().into_iter().flatten() {
                if !actual.is_some_and(|actual| actual.contains(value)) {
                    removed.push(format!("{pointer}: {value}"));
                }
            }
        }
        // A field that becomes required rejects files written without it.
        for (pointer, required) in &current {
            if !pointer.ends_with("/required") {
                continue;
            }
            // An object that is new in this build may require what it likes.
            let object = pointer.trim_end_matches("/required");
            if published.get(format!("{object}/properties")).is_none() {
                continue;
            }
            let before = published.get(pointer).and_then(|v| v.as_array());
            for name in required.as_array().into_iter().flatten() {
                if !before.is_some_and(|before| before.contains(name)) {
                    removed.push(format!("{pointer}: {name} became required"));
                }
            }
        }
        assert!(
            removed.is_empty(),
            "fields, types, enum values or limits stored in user files changed incompatibly: {removed:?}"
        );
        snapshot(
            &path,
            &(serde_json::to_string_pretty(&serde_json::Value::Object(current)).unwrap() + "\n"),
        );
    }

    #[test]
    fn backup_sits_next_to_the_file() {
        assert_eq!(
            backup_path(Path::new("/c/keymaps/Main.json"), 1),
            Path::new("/c/keymaps/Main.json.v1.bak")
        );
    }
}
