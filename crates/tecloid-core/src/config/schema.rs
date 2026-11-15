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
    /// `tecloid:` and `builtin:` ids, key names and trigger modifiers.
    fn stored_names() -> Vec<String> {
        use super::super::{actions, key_catalog};
        let mut names = Vec::new();
        for prefix in [
            "app:", "macro:", "builtin:", "cmd:", "sys:", "tecloid:", "kde:", "xdg:", "text:",
            "pause:",
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
            if let Some(id) = action.strip_prefix("app:") {
                names.push(format!("action tecloid:{id}"));
            }
            names.push(format!("action {action}"));
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
        names.sort();
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

    #[test]
    fn frozen_fields_and_enum_values_remain() {
        let baseline: serde_json::Value =
            serde_json::from_str(include_str!("../../../../schemas/contract.v1.json")).unwrap();
        let schemas = [
            ("config", super::super::settings::json_schema().to_value()),
            (
                "keymap",
                super::super::keymap_file::json_schema().to_value(),
            ),
        ];
        for (path, expected) in baseline.as_object().unwrap() {
            let (name, pointer) = path.split_once('/').unwrap();
            let schema = &schemas.iter().find(|(n, _)| *n == name).unwrap().1;
            let actual = schema.pointer(&format!("/{pointer}")).expect(path);
            for value in expected.as_array().unwrap() {
                assert!(
                    if pointer.ends_with("/properties") {
                        actual.get(value.as_str().unwrap()).is_some()
                    } else {
                        actual.as_array().unwrap().contains(value)
                    },
                    "removed stored value {value} at {path}"
                );
            }
        }
    }

    #[test]
    fn backup_sits_next_to_the_file() {
        assert_eq!(
            backup_path(Path::new("/c/keymaps/Main.json"), 1),
            Path::new("/c/keymaps/Main.json.v1.bak")
        );
    }
}
