//! Key tables and shortcut keys rebuilt from the layouts the desktop reports.
//!
//! The only part of text injection that depends on `crate::keyboard_layout`.
//! It serves the portal keycode and clipboard modes, which get no keymap from
//! the compositor, and the mapper's Latin shortcut keys. libei modes use the
//! compositor keymap instead (see `libei::keys`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::hardware::caps_lock_state;
use super::xkb::{
    KeyTable, RuleNames, paste_v_key_from_names, shortcut_key_from_names, shortcut_key_from_rules,
    table_from_rules,
};
use crate::events::TextInputFailure;
use crate::platform::gnome::TextKeymap;

/// evdev key that applications read as the Latin letter `character` in a
/// shortcut: in the active layout, or in another configured layout when the
/// active one has no such letter (Qt and GTK borrow Ctrl+C from the Latin
/// layout on Cyrillic). `None` when the layout is unknown.
/// Cached: compiling an XKB keymap reads files, and the mapper asks on
/// every standard shortcut it sends.
pub(crate) fn shortcut_key_for_letter(character: char) -> Option<u16> {
    type Cache = Mutex<HashMap<(Vec<(String, String)>, String, String, u32, char), Option<u16>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let layout = crate::runtime_state::keyboard_layout()?;
    let layouts = crate::keyboard_layout::shortcut_layouts();
    let cache_key = (
        layouts.clone(),
        layout.code.clone(),
        layout.variant.clone(),
        layout.index,
        character,
    );
    let cache = CACHE.get_or_init(Default::default);
    if let Some(key) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&cache_key)
    {
        return *key;
    }
    let group = layout.index as usize;
    let key = if layouts
        .get(group)
        .is_some_and(|(code, variant)| *code == layout.code && *variant == layout.variant)
    {
        let names: Vec<_> = layouts
            .iter()
            .map(|(code, variant)| (code.as_str(), variant.as_str()))
            .collect();
        shortcut_key_from_names(&names, layout.index, "", character)
    } else {
        shortcut_key_from_names(&[(&layout.code, &layout.variant)], 0, "", character)
    };
    let key = key.and_then(|key| u16::try_from(key).ok());
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    // Layout sets change rarely; the bound only guards against growth.
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(cache_key, key);
    key
}

fn rule_names(keymap: &TextKeymap) -> RuleNames<'_> {
    RuleNames {
        layouts: &keymap.layouts,
        variants: &keymap.variants,
        options: &keymap.options,
        model: &keymap.model,
    }
}

fn gnome_wayland() -> bool {
    use crate::platform::linux::{Desktop, SessionType, detect};
    let session = detect();
    session.desktop == Desktop::Gnome && session.session_type == SessionType::Wayland
}

/// Key that types `v` for the layouts the desktop reports (keycode and
/// clipboard modes, which have no compositor keymap). `None` when the
/// layouts or their variants cannot be verified.
pub(super) fn desktop_paste_v_key() -> Option<u32> {
    if gnome_wayland() {
        let keymap = crate::keyboard_layout::gnome_text_keymap()?;
        return shortcut_key_from_rules(rule_names(&keymap), keymap.group, 'v');
    }

    let active = crate::runtime_state::keyboard_layout();
    let listed = crate::keyboard_layout::verified_shortcut_layouts()?;
    let active = active.as_ref()?;
    let group = usize::try_from(active.index).ok()?;
    if listed.get(group)?.0 != active.code {
        return None;
    }
    let names: Vec<_> = listed
        .iter()
        .map(|(code, variant)| (code.as_str(), variant.as_str()))
        .collect();
    paste_v_key_from_names(&names, group as u32, &crate::keyboard_layout::xkb_options())
}

type KeymapCache = Option<(TextKeymap, bool, Arc<KeyTable>)>;
static KEYMAP_CACHE: Mutex<KeymapCache> = Mutex::new(None);

/// Key table of the active layout for the portal keycode mode: the GNOME
/// Shell keymap on GNOME Wayland, otherwise rebuilt from the verified layout
/// name and variant. Fails when it is unknown or XKB cannot compile it:
/// guessing a layout would type wrong characters.
pub(super) fn desktop_key_table() -> Result<Arc<KeyTable>, TextInputFailure> {
    let rules = if gnome_wayland() {
        crate::keyboard_layout::gnome_text_keymap().ok_or(TextInputFailure::LayoutUnknown)?
    } else {
        let layout =
            crate::runtime_state::keyboard_layout().ok_or(TextInputFailure::LayoutUnknown)?;
        let variant = crate::keyboard_layout::verified_text_variant(&layout)
            .ok_or(TextInputFailure::LayoutUnknown)?;
        TextKeymap {
            layouts: layout.code,
            variants: variant.trim().into(),
            options: crate::keyboard_layout::xkb_options(),
            model: String::new(),
            group: 0,
        }
    };
    cached_key_table(rules, caps_lock_state()?)
}

fn cached_key_table(rules: TextKeymap, caps: bool) -> Result<Arc<KeyTable>, TextInputFailure> {
    let mut cache = KEYMAP_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((previous, previous_caps, table)) = cache.as_ref()
        && *previous == rules
        && *previous_caps == caps
    {
        return Ok(Arc::clone(table));
    }
    let table = Arc::new(
        table_from_rules(rule_names(&rules), rules.group, caps)
            .ok_or(TextInputFailure::LayoutUnknown)?,
    );
    *cache = Some((rules, caps, Arc::clone(&table)));
    Ok(table)
}
