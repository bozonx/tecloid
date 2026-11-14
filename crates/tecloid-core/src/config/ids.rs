//! Random ids for new configuration entities.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

/// `prefix` followed by eight base-36 characters.
pub fn generate(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    let mut value = hasher.finish();
    let mut out = String::from(prefix);
    for _ in 0..8 {
        out.push(char::from_digit((value % 36) as u32, 36).unwrap_or('0'));
        value /= 36;
    }
    out
}

/// First free `prefix1`, `prefix2`, …: short, readable ids for entities the
/// user does not name by id (macros `m1`, commands `c1`).
pub fn numbered(prefix: &str, taken: impl Fn(&str) -> bool) -> String {
    (1u64..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken(id))
        .unwrap_or_else(|| generate(prefix))
}

/// A readable id derived from `name` (`Navigation keys` → `navigation-keys`),
/// `fallback` when the name has no ASCII letters or digits, with `2`, `3`, …
/// appended while `taken`.
pub fn from_name(name: &str, fallback: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut base = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            base.push(c.to_ascii_lowercase());
        } else if !base.is_empty() && !base.ends_with('-') {
            base.push('-');
        }
    }
    let mut base: String = base
        .trim_end_matches('-')
        .chars()
        .take(MAX_LEN - 4)
        .collect();
    base.truncate(base.trim_end_matches('-').len());
    if base.is_empty() {
        base = fallback.to_owned();
    }
    if !taken(&base) {
        return base;
    }
    (2u64..)
        .map(|n| format!("{base}{n}"))
        .find(|id| !taken(id))
        .unwrap_or_else(|| generate(fallback))
}

/// Longest id accepted in configuration files.
pub const MAX_LEN: usize = 64;

/// Whether `id` is 1–[`MAX_LEN`] ASCII letters, digits, `_` or `-`. Every id
/// follows this grammar, so ids read the same inside action strings
/// (`macro:<id>`, `cmd:<id>`) and need no quoting anywhere.
pub fn valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_LEN
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// Validate identity within one collection. Different entity kinds may share ids.
pub(crate) fn unique<'a>(ids: impl IntoIterator<Item = &'a str>, path: &str) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for (index, id) in ids.into_iter().enumerate() {
        if !valid(id) {
            return Err(format!(
                "{path}[{index}].id: \"{id}\" must be 1-{MAX_LEN} ASCII letters, digits, '_' or '-'"
            ));
        }
        if !seen.insert(id) {
            return Err(format!("{path}[{index}].id: duplicate id \"{id}\""));
        }
    }
    Ok(())
}

pub(crate) fn validate_keymap(keymap: &super::model::KeymapPreset) -> Result<(), String> {
    unique(keymap.layers.iter().map(|item| item.id.as_str()), "layers")?;
    unique(keymap.rules.iter().map(|item| item.id.as_str()), "rules")?;
    unique(keymap.macros.iter().map(|item| item.id.as_str()), "macros")?;
    unique(
        keymap.commands.iter().map(|item| item.id.as_str()),
        "commands",
    )?;
    unique(
        keymap
            .quick_action_pages
            .iter()
            .map(|item| item.id.as_str()),
        "quickMenuPages",
    )?;
    unique(
        keymap.emoji_pages.iter().map(|item| item.id.as_str()),
        "emojiMenuPages",
    )?;
    for item in &keymap.macros {
        unique(
            item.steps.iter().map(|step| step.id.as_str()),
            &format!("macros[{}].steps", item.id),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids_are_prefixed_and_distinct() {
        let a = super::generate("r_");
        let b = super::generate("r_");
        assert!(a.starts_with("r_") && a.len() == 10);
        assert_ne!(a, b);
    }

    #[test]
    fn readable_ids() {
        let taken = |id: &str| id == "nav" || id == "c1";
        assert_eq!(super::from_name("Nav", "layer", taken), "nav2");
        assert_eq!(
            super::from_name(" Navigation keys! ", "layer", taken),
            "navigation-keys"
        );
        assert_eq!(super::from_name("Символы", "layer", taken), "layer");
        assert!(super::valid(&super::from_name(
            &"x".repeat(200),
            "layer",
            taken
        )));
        assert_eq!(super::numbered("c", taken), "c2");
    }

    #[test]
    fn id_grammar() {
        for id in ["a", "copy-line_2", &"x".repeat(super::MAX_LEN)] {
            assert!(super::valid(id), "{id}");
        }
        for id in [
            "",
            " a",
            "a b",
            "a.b",
            "a:b",
            "ид",
            &"x".repeat(super::MAX_LEN + 1),
        ] {
            assert!(!super::valid(id), "{id}");
        }
        assert!(super::unique(["a b"], "layers").is_err());
    }
}
