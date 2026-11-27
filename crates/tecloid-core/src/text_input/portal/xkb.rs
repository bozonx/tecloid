//! Safe wrapper over libxkbcommon: which key and level type each character.
//!
//! Knows nothing about the desktop or the active layout; callers pass XKB
//! rule names or a keymap the compositor sent.

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, c_char};
use std::ptr::NonNull;

enum XkbContext {}
enum XkbKeymapRaw {}
enum XkbState {}

#[repr(C)]
struct XkbRuleNames {
    rules: *const c_char,
    model: *const c_char,
    layout: *const c_char,
    variant: *const c_char,
    options: *const c_char,
}

const XKB_KEYMAP_FORMAT_TEXT_V1: u32 = 1;
const XKB_STATE_MODS_LOCKED: u32 = 1 << 2;
const XKB_STATE_LAYOUT_EFFECTIVE: u32 = 1 << 7;

#[link(name = "xkbcommon")]
unsafe extern "C" {
    fn xkb_utf32_to_keysym(ucs: u32) -> u32;
    fn xkb_context_new(flags: u32) -> *mut XkbContext;
    fn xkb_context_unref(ctx: *mut XkbContext);
    fn xkb_keymap_new_from_names(
        ctx: *mut XkbContext,
        names: *const XkbRuleNames,
        flags: u32,
    ) -> *mut XkbKeymapRaw;
    fn xkb_keymap_new_from_buffer(
        ctx: *mut XkbContext,
        buffer: *const c_char,
        length: usize,
        format: u32,
        flags: u32,
    ) -> *mut XkbKeymapRaw;
    fn xkb_keymap_unref(keymap: *mut XkbKeymapRaw);
    fn xkb_keymap_min_keycode(keymap: *mut XkbKeymapRaw) -> u32;
    fn xkb_keymap_max_keycode(keymap: *mut XkbKeymapRaw) -> u32;
    fn xkb_keymap_num_layouts(keymap: *mut XkbKeymapRaw) -> u32;
    fn xkb_keymap_key_get_name(keymap: *mut XkbKeymapRaw, key: u32) -> *const c_char;
    fn xkb_state_new(keymap: *mut XkbKeymapRaw) -> *mut XkbState;
    fn xkb_state_unref(state: *mut XkbState);
    fn xkb_state_update_mask(
        state: *mut XkbState,
        depressed_mods: u32,
        latched_mods: u32,
        locked_mods: u32,
        depressed_layout: u32,
        latched_layout: u32,
        locked_layout: u32,
    ) -> u32;
    fn xkb_state_update_key(state: *mut XkbState, key: u32, direction: u32) -> u32;
    fn xkb_state_key_get_one_sym(state: *mut XkbState, key: u32) -> u32;
    fn xkb_state_serialize_mods(state: *mut XkbState, components: u32) -> u32;
    fn xkb_state_serialize_layout(state: *mut XkbState, components: u32) -> u32;
}

/// A key and the modifier keys (evdev codes) that type one keysym.
#[derive(Clone)]
pub(super) struct KeycodeEntry {
    pub(super) evdev: u32,
    pub(super) mods: Vec<u32>,
}

/// keysym → the key and level that type it.
pub(super) type KeyTable = HashMap<u32, KeycodeEntry>;

/// XKB rule names of a keymap; comma-separated for several groups.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RuleNames<'a> {
    pub(super) layouts: &'a str,
    pub(super) variants: &'a str,
    pub(super) options: &'a str,
    pub(super) model: &'a str,
}

/// An owned compiled keymap.
struct Keymap(NonNull<XkbKeymapRaw>);

impl Drop for Keymap {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a keymap reference this value owns.
        unsafe { xkb_keymap_unref(self.0.as_ptr()) }
    }
}

/// An owned state of a keymap; must not outlive it.
struct State(NonNull<XkbState>);

impl Drop for State {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a state reference this value owns.
        unsafe { xkb_state_unref(self.0.as_ptr()) }
    }
}

impl State {
    fn raw(&self) -> *mut XkbState {
        self.0.as_ptr()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyClass {
    Main,
    Keypad,
}

impl Keymap {
    fn from_names(names: RuleNames<'_>) -> Option<Self> {
        let layout = CString::new(names.layouts).ok()?;
        let variant = CString::new(names.variants).ok()?;
        let model = CString::new(names.model).ok()?;
        let options = CString::new(names.options).ok()?;
        let names = XkbRuleNames {
            rules: std::ptr::null(),
            model: model.as_ptr(),
            layout: layout.as_ptr(),
            variant: variant.as_ptr(),
            options: options.as_ptr(),
        };
        // SAFETY: `names` points to strings that outlive the call; the
        // context is checked for null and released (the keymap keeps its own
        // reference).
        unsafe {
            let ctx = xkb_context_new(0);
            if ctx.is_null() {
                return None;
            }
            let keymap = xkb_keymap_new_from_names(ctx, &names, 0);
            xkb_context_unref(ctx);
            NonNull::new(keymap).map(Self)
        }
    }

    /// A keymap the compositor sent as XKB text, up to the first NUL.
    fn from_text(text: &[u8]) -> Option<Self> {
        let text = text.split(|byte| *byte == 0).next().unwrap_or_default();
        // SAFETY: `text` outlives the call; the context is checked for null
        // and released.
        unsafe {
            let ctx = xkb_context_new(0);
            if ctx.is_null() {
                return None;
            }
            let keymap = xkb_keymap_new_from_buffer(
                ctx,
                text.as_ptr().cast(),
                text.len(),
                XKB_KEYMAP_FORMAT_TEXT_V1,
                0,
            );
            xkb_context_unref(ctx);
            NonNull::new(keymap).map(Self)
        }
    }

    fn raw(&self) -> *mut XkbKeymapRaw {
        self.0.as_ptr()
    }

    fn state(&self) -> Option<State> {
        // SAFETY: the keymap is live; the state is owned by the result.
        NonNull::new(unsafe { xkb_state_new(self.raw()) }).map(State)
    }

    fn num_layouts(&self) -> u32 {
        // SAFETY: the keymap is live.
        unsafe { xkb_keymap_num_layouts(self.raw()) }
    }

    fn keys(&self) -> std::ops::RangeInclusive<u32> {
        // SAFETY: the keymap is live.
        unsafe { xkb_keymap_min_keycode(self.raw())..=xkb_keymap_max_keycode(self.raw()) }
    }

    /// Block of a key by its XKB name; `None` for keys not used to type text.
    fn key_class(&self, key: u32) -> Option<KeyClass> {
        // X11 keycodes stop at 255; higher ones never reach X11 clients.
        if !(9..=255).contains(&key) {
            return None;
        }
        // SAFETY: the keymap is live; the name it returns is owned by the
        // keymap and only read within this call.
        let name = unsafe {
            let name = xkb_keymap_key_get_name(self.raw(), key);
            if name.is_null() {
                return None;
            }
            CStr::from_ptr(name).to_bytes()
        };
        let row = |prefix: &[u8]| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty() && rest.iter().all(u8::is_ascii_digit))
        };
        if row(b"AE") || row(b"AD") || row(b"AC") || row(b"AB") {
            return Some(KeyClass::Main);
        }
        match name {
            b"TLDE" | b"BKSL" | b"LSGT" | b"SPCE" | b"RTRN" | b"TAB" => Some(KeyClass::Main),
            _ if name.starts_with(b"KP") => Some(KeyClass::Keypad),
            _ => None,
        }
    }

    /// evdev key that types `character` without modifiers, the way Qt and
    /// GTK match a Latin shortcut: in `group` first, then in the other groups
    /// in order (a Cyrillic layout borrows the shortcut from the Latin one).
    fn shortcut_key(&self, group: u32, character: char) -> Option<u32> {
        let groups = self.num_layouts();
        let state = self.state()?;
        let found = std::iter::once(group)
            .chain((0..groups).filter(|g| *g != group))
            .filter(|g| *g < groups)
            .find_map(|g| {
                // SAFETY: the state belongs to this live keymap.
                unsafe { xkb_state_update_mask(state.raw(), 0, 0, 0, 0, 0, g) };
                self.keys().find(|key| {
                    self.key_class(*key) == Some(KeyClass::Main)
                        // SAFETY: as above.
                        && unsafe { xkb_state_key_get_one_sym(state.raw(), *key) }
                            == character as u32
                })
            });
        found.map(|key| key - 8)
    }

    /// Locked modifier mask Caps Lock sets in `group`. Caps Lock may be
    /// configured as Shift Lock; derive its actual action. `None` when the
    /// keymap has no lock key or it locks nothing.
    fn caps_lock_mask(&self, group: u32) -> Option<u32> {
        let state = self.state()?;
        // SAFETY: the state belongs to this live keymap.
        unsafe {
            xkb_state_update_mask(state.raw(), 0, 0, 0, 0, 0, group);
            let lock_key = self.keys().find(|key| {
                matches!(
                    xkb_state_key_get_one_sym(state.raw(), *key),
                    0xffe5 | 0xffe6
                )
            })?;
            xkb_state_update_key(state.raw(), lock_key, 1);
            xkb_state_update_key(state.raw(), lock_key, 0);
            let locked = xkb_state_serialize_mods(state.raw(), XKB_STATE_MODS_LOCKED);
            (locked != 0).then_some(locked)
        }
    }

    /// Index the keysyms `group` types, keeping for each the entry that needs
    /// the fewest modifiers.
    fn key_table(&self, group: u32, locked: u32) -> KeyTable {
        let mut map = KeyTable::new();
        let Some(owner) = self.state() else {
            return map;
        };
        let state = owner.raw();
        // SAFETY (all blocks below): `state` belongs to this live keymap and
        // is released when the owning `State` drops at the end.
        // Discover actual modifier key actions instead of assuming that level N
        // always means Shift/AltGr. This also supports remapped and level-5 keys.
        unsafe { xkb_state_update_mask(state, 0, 0, 0, 0, 0, group) };
        let mut modifiers = Vec::new();
        let mut seen = HashSet::new();
        for key in self.keys() {
            let sym = match unsafe { xkb_state_key_get_one_sym(state, key) } {
                0xffe2 => 0xffe1,
                sym => sym,
            };
            if matches!(sym, 0xffe1 | 0xffe2 | 0xfe03 | 0xfe11 | 0xff7e) && seen.insert(sym) {
                modifiers.push(key);
            }
        }
        // Duplicate left/right Shift adds no characters. Keep enumeration bounded
        // even with hostile or unusual compositor keymaps.
        modifiers.truncate(5);
        let mut combinations: Vec<Vec<u32>> = (0usize..(1 << modifiers.len()))
            .map(|mask| {
                modifiers
                    .iter()
                    .enumerate()
                    .filter_map(|(i, key)| (mask & (1 << i) != 0).then_some(*key))
                    .collect()
            })
            .collect();
        combinations.sort_by_key(Vec::len);
        // Main-block keys win over the keypad even when they need a modifier:
        // keypad keys may send escape sequences in terminals or trigger
        // commands (zoom, view) instead of text. Other keys, such as
        // KEY_DOLLAR/KEY_EURO from `inet(evdev)` or keycodes above 255, are
        // ignored by X11 clients and Chromium/Electron, so a character found
        // only there counts as missing.
        let typing_keys = |class: KeyClass| {
            self.keys()
                .filter(|key| !modifiers.contains(key))
                .filter(|key| self.key_class(*key) == Some(class))
                .collect::<Vec<_>>()
        };
        for keys in [typing_keys(KeyClass::Main), typing_keys(KeyClass::Keypad)] {
            for mods in &combinations {
                for &key in &keys {
                    unsafe {
                        xkb_state_update_mask(state, 0, 0, locked, 0, 0, group);
                        let baseline_locked =
                            xkb_state_serialize_mods(state, XKB_STATE_MODS_LOCKED);
                        let baseline_group =
                            xkb_state_serialize_layout(state, XKB_STATE_LAYOUT_EFFECTIVE);
                        for modifier in mods {
                            xkb_state_update_key(state, *modifier, 1);
                        }
                        let sym = xkb_state_key_get_one_sym(state, key);
                        for modifier in mods.iter().rev() {
                            xkb_state_update_key(state, *modifier, 0);
                        }
                        // A typing chord must not toggle a lock or leave another group active.
                        if sym != 0
                            && xkb_state_serialize_mods(state, XKB_STATE_MODS_LOCKED)
                                == baseline_locked
                            && xkb_state_serialize_layout(state, XKB_STATE_LAYOUT_EFFECTIVE)
                                == baseline_group
                        {
                            map.entry(sym).or_insert_with(|| KeycodeEntry {
                                evdev: key - 8,
                                mods: mods.iter().map(|key| key - 8).collect(),
                            });
                        }
                    }
                }
            }
        }
        map
    }
}

/// Key table of `group` for keymap rule names; with `caps`, as typed while
/// Caps Lock is on. `None` when XKB cannot compile the rules, the group does
/// not exist or nothing is typeable.
pub(super) fn table_from_rules(names: RuleNames<'_>, group: u32, caps: bool) -> Option<KeyTable> {
    let Some(keymap) = Keymap::from_names(names) else {
        log::warn!("[text] XKB cannot compile keymap {names:?}");
        return None;
    };
    if group >= keymap.num_layouts() {
        return None;
    }
    let locked = if caps {
        keymap.caps_lock_mask(group)?
    } else {
        0
    };
    let table = keymap.key_table(group, locked);
    (!table.is_empty()).then_some(table)
}

/// Key table of `group` in a keymap the compositor sent as XKB text.
pub(super) fn table_from_keymap_text(text: &[u8], group: u32, locked: u32) -> Option<KeyTable> {
    let table = Keymap::from_text(text)?.key_table(group, locked);
    (!table.is_empty()).then_some(table)
}

/// evdev key applications read as the Latin `character` in a shortcut, for
/// keymap rule names. On Dvorak `v` is the physical `KEY_DOT`.
pub(super) fn shortcut_key_from_rules(
    names: RuleNames<'_>,
    group: u32,
    character: char,
) -> Option<u32> {
    Keymap::from_names(names)?.shortcut_key(group, character)
}

/// [`shortcut_key_from_rules`] for layouts given as `(code, variant)` pairs,
/// e.g. `[("ru", ""), ("us", "dvorak")]`.
pub(super) fn shortcut_key_from_names(
    layouts: &[(&str, &str)],
    group: u32,
    options: &str,
    character: char,
) -> Option<u32> {
    if layouts.is_empty() {
        return None;
    }
    let codes: Vec<&str> = layouts.iter().map(|l| l.0).collect();
    let variants: Vec<&str> = layouts.iter().map(|l| l.1).collect();
    shortcut_key_from_rules(
        RuleNames {
            layouts: &codes.join(","),
            variants: &variants.join(","),
            options,
            model: "",
        },
        group,
        character,
    )
}

/// Key that types `v` in a keymap the compositor sent as XKB text.
pub(super) fn paste_v_key_from_keymap_text(text: &[u8], group: u32) -> Option<u32> {
    Keymap::from_text(text)?.shortcut_key(group, 'v')
}

/// Key that types `v` for layouts given as `(code, variant)` pairs.
pub(super) fn paste_v_key_from_names(
    layouts: &[(&str, &str)],
    group: u32,
    options: &str,
) -> Option<u32> {
    shortcut_key_from_names(layouts, group, options, 'v')
}

/// Keysym typing `ch`. Text control characters need keyboard keysyms, not
/// Unicode keysyms.
pub(super) fn text_keysym(ch: char) -> u32 {
    match ch {
        '\n' | '\r' => 0xff0d, // Return
        '\t' => 0xff09,        // Tab
        // SAFETY: a pure lookup without pointers.
        _ => unsafe { xkb_utf32_to_keysym(ch as u32) },
    }
}

pub(super) fn missing_characters(table: &KeyTable, text: &str) -> bool {
    text.chars().any(|ch| !table.contains_key(&text_keysym(ch)))
}

#[cfg(test)]
pub(super) fn table_from_names(layout: &str, variant: Option<&str>) -> Option<KeyTable> {
    table_from_names_with_options(layout, variant, false, "")
}

#[cfg(test)]
pub(super) fn table_from_names_with_caps(
    layout: &str,
    variant: Option<&str>,
    caps: bool,
) -> Option<KeyTable> {
    table_from_names_with_options(layout, variant, caps, "")
}

#[cfg(test)]
pub(super) fn table_from_names_with_options(
    layout: &str,
    variant: Option<&str>,
    caps: bool,
    options: &str,
) -> Option<KeyTable> {
    table_from_rules(
        RuleNames {
            layouts: layout,
            variants: variant.unwrap_or_default(),
            options,
            model: "",
        },
        0,
        caps,
    )
}

/// Tables of every group of one keymap, for tests that compare groups.
#[cfg(test)]
pub(super) fn tables_of_groups(layouts: &str, groups: u32) -> Vec<KeyTable> {
    let keymap = Keymap::from_names(RuleNames {
        layouts,
        ..RuleNames::default()
    })
    .expect("keymap");
    (0..groups)
        .map(|group| keymap.key_table(group, 0))
        .collect()
}
