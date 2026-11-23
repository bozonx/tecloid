//! X11 keyboard layout backend (Cinnamon, Xfce, MATE and other X11
//! desktops without their own backend).
//!
//! The active layout is the XKB group of the core keyboard; layout codes
//! and variants come from `_XKB_RULES_NAMES`. Desktops keep both current,
//! so no desktop API is needed. The watcher polls one connection: XKB
//! state events would need a second event loop for a value that changes
//! only on a key press.

#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use super::KeyboardLayoutInfo;
use crate::platform::x11::Display;

const POLL_INTERVAL: Duration = Duration::from_millis(150);
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// Only a real X11 session: under Wayland `DISPLAY` is XWayland, whose
/// group does not follow the compositor's layout.
fn x11_session() -> bool {
    matches!(
        crate::platform::linux::detect().session_type,
        crate::platform::linux::SessionType::X11
    )
}

/// Human-readable layout names from the XKB rules list.
fn description(code: &str) -> String {
    static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            std::fs::read_to_string("/usr/share/X11/xkb/rules/evdev.lst")
                .map(|text| parse_layout_list(&text))
                .unwrap_or_default()
        })
        .get(code)
        .cloned()
        .unwrap_or_else(|| code.to_owned())
}

/// `! layout` section of `evdev.lst`: `  us              English (US)`.
fn parse_layout_list(text: &str) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let mut in_layouts = false;
    for line in text.lines() {
        if let Some(section) = line.strip_prefix('!') {
            in_layouts = section.trim() == "layout";
            continue;
        }
        if !in_layouts {
            continue;
        }
        let line = line.trim();
        if let Some((code, name)) = line.split_once(char::is_whitespace) {
            names.insert(code.to_owned(), name.trim().to_owned());
        }
    }
    names
}

fn infos(layouts: &[(String, String)]) -> Vec<KeyboardLayoutInfo> {
    layouts
        .iter()
        .enumerate()
        .map(|(index, (code, variant))| KeyboardLayoutInfo {
            code: code.clone(),
            display_label: String::new(),
            name: description(code),
            variant: variant.clone(),
            index: index as u32,
            backend: "linux-x11",
        })
        .collect()
}

fn read(display: &Display) -> Result<Option<KeyboardLayoutInfo>, String> {
    let layouts = display.layouts()?;
    super::remember_shortcut_layouts(layouts.clone());
    let group = display.group()?;
    Ok(infos(&layouts).into_iter().nth(group as usize))
}

fn observe(display: &Display) -> Result<Option<KeyboardLayoutInfo>, String> {
    match read(display) {
        Ok(Some(info)) => {
            super::publish(&info);
            Ok(Some(info))
        }
        Ok(None) => {
            super::clear();
            Ok(None)
        }
        Err(error) => {
            super::clear();
            Err(error)
        }
    }
}

pub fn current() -> Result<Option<KeyboardLayoutInfo>, String> {
    if !x11_session() {
        return Ok(None);
    }
    observe(&Display::open()?)
}

pub fn available_layouts() -> Result<Vec<KeyboardLayoutInfo>, String> {
    if !x11_session() {
        return Ok(Vec::new());
    }
    Ok(infos(&Display::open()?.layouts()?))
}

pub fn set_layout(index: u32) -> Result<(), String> {
    if !x11_session() {
        return Err("Switching layout needs an X11 session on this desktop".into());
    }
    let display = Display::open()?;
    display.lock_group(index)?;
    let _ = observe(&display);
    Ok(())
}

pub fn start_watcher() {
    if !x11_session() {
        log::debug!("[layout/x11] not an X11 session; no layout backend");
        return;
    }
    super::register_watcher(
        std::thread::Builder::new()
            .name("layout-x11".into())
            .spawn(watch),
    );
}

fn watch() {
    while !super::watcher_stop_requested() {
        let display = match Display::open() {
            Ok(display) => display,
            Err(error) => {
                log::debug!("[layout/x11] {error}");
                std::thread::sleep(RECONNECT_DELAY);
                continue;
            }
        };
        while !super::watcher_stop_requested() {
            if let Err(error) = observe(&display) {
                log::debug!("[layout/x11] {error}; reconnecting");
                std::thread::sleep(RECONNECT_DELAY);
                break;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_layout_list;

    #[test]
    fn layout_list_section_only() {
        let names = parse_layout_list(
            "! model\n  pc105           Generic 105-key PC\n\n! layout\n  us              English (US)\n  ru              Russian\n\n! variant\n  dvorak          us: English (Dvorak)\n",
        );
        assert_eq!(names.get("us").map(String::as_str), Some("English (US)"));
        assert_eq!(names.get("ru").map(String::as_str), Some("Russian"));
        assert!(!names.contains_key("pc105") && !names.contains_key("dvorak"));
    }
}
