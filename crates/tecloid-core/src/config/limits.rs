//! Size limits of `config.json` and keymap files.
//!
//! Every limit is part of the file format: it may be raised, never lowered
//! (`limits.v1.txt`, test `limits_only_widen`). A file over a limit is a
//! load error, and the application refuses to save one.

use serde_json::Value;

/// Most entries in any JSON array or object (layers, rules, macro steps,
/// layer keys, condition lists, …).
pub const MAX_ITEMS: usize = 10_000;
/// Longest JSON string, key or value, in UTF-8 bytes (actions, `text:`
/// payloads, scripts, names).
pub const MAX_TEXT_BYTES: usize = 65_536;
/// Most quick or emoji menu pages: every page has an `app:` slot.
pub const MAX_MENU_PAGES: usize = super::actions::MAX_SLOT as usize;

/// Check every array, object and string of a parsed file against the limits.
pub(crate) fn check(value: &Value, what: &str) -> Result<(), String> {
    fn walk(value: &Value, path: &mut String, what: &str) -> Result<(), String> {
        let too_many = |len: usize, path: &str| {
            Err(format!(
                "{what}: {path}: {len} entries, at most {MAX_ITEMS} are allowed"
            ))
        };
        let too_long = |path: &str| {
            Err(format!(
                "{what}: {path}: text longer than {MAX_TEXT_BYTES} bytes"
            ))
        };
        match value {
            Value::String(text) if text.len() > MAX_TEXT_BYTES => too_long(path),
            Value::Array(items) => {
                if items.len() > MAX_ITEMS {
                    return too_many(items.len(), path);
                }
                for (index, item) in items.iter().enumerate() {
                    let len = path.len();
                    path.push_str(&format!("[{index}]"));
                    walk(item, path, what)?;
                    path.truncate(len);
                }
                Ok(())
            }
            Value::Object(map) => {
                if map.len() > MAX_ITEMS {
                    return too_many(map.len(), path);
                }
                for (key, item) in map {
                    let len = path.len();
                    path.push('.');
                    if key.len() > MAX_TEXT_BYTES {
                        return too_long(path);
                    }
                    path.push_str(key);
                    walk(item, path, what)?;
                    path.truncate(len);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    walk(value, &mut String::from("$"), what)
}

/// Every limit the formats impose, for the frozen `limits.v1.txt`:
/// `max <name> <value>` may only grow, `min <name> <value>` only shrink.
#[cfg(test)]
pub(crate) fn published() -> Vec<String> {
    use super::{actions, ids, timing};
    let mut lines = vec![
        format!("max items {MAX_ITEMS}"),
        format!("max textBytes {MAX_TEXT_BYTES}"),
        format!("max menuPages {MAX_MENU_PAGES}"),
        format!("max slot {}", actions::MAX_SLOT),
        format!("max idLength {}", ids::MAX_LEN),
        format!(
            "max keymapNameChars {}",
            crate::storage::MAX_KEYMAP_NAME_CHARS
        ),
        format!(
            "max keymapNameBytes {}",
            crate::storage::MAX_KEYMAP_NAME_BYTES
        ),
        format!("max macroDepth {}", actions::MAX_MACRO_DEPTH),
        format!("max macroSteps {}", actions::MAX_MACRO_STEPS),
        format!("min pauseMs {}", timing::PAUSE.min),
        format!("max pauseMs {}", timing::PAUSE.max),
    ];
    lines.sort();
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn limits_apply_to_every_array_object_and_string() {
        assert!(check(&json!({"a": [1, "x", {"b": null}]}), "f").is_ok());
        let long = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(
            check(&json!({"a": [long.clone()]}), "f")
                .unwrap_err()
                .contains("$.a[0]")
        );
        assert!(check(&json!({ long: 1 }), "f").is_err());
        let many: Vec<u8> = vec![0; MAX_ITEMS + 1];
        assert!(
            check(&json!({ "a": { "b": many } }), "f")
                .unwrap_err()
                .contains("$.a.b")
        );
        assert!(check(&json!("x".repeat(MAX_TEXT_BYTES)), "f").is_ok());
    }
}
