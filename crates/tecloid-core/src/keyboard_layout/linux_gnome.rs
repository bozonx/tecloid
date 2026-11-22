//! GNOME input sources, including the actual per-window source, from the Shell bridge.

use std::sync::Mutex;
use std::time::Duration;

use super::KeyboardLayoutInfo;
use crate::platform::gnome;

static TEXT_KEYMAP: Mutex<Option<gnome::TextKeymap>> = Mutex::new(None);

pub(crate) fn text_keymap() -> Option<gnome::TextKeymap> {
    TEXT_KEYMAP
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

fn remember_text_keymap(keymap: Option<gnome::TextKeymap>) {
    *TEXT_KEYMAP.lock().unwrap_or_else(|e| e.into_inner()) = keymap;
}

fn info(source: gnome::InputSource) -> KeyboardLayoutInfo {
    KeyboardLayoutInfo {
        code: source.short,
        variant: source.variant,
        display_label: source.display,
        name: source.long,
        index: source.index,
        backend: "linux-gnome",
    }
}

pub fn current() -> Result<Option<KeyboardLayoutInfo>, String> {
    // Serialize the watcher's observations with explicit source switches.
    static REFRESH: Mutex<()> = Mutex::new(());
    let _refresh = REFRESH.lock().unwrap_or_else(|e| e.into_inner());
    let snapshot = match gnome::snapshot(false) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            clear_layout();
            return Err(error);
        }
    };
    super::remember_shortcut_layouts(
        snapshot
            .layouts
            .iter()
            .map(|source| (source.short.clone(), source.variant.clone()))
            .collect(),
    );
    let current = snapshot.current;
    let layout = snapshot
        .layouts
        .into_iter()
        .find(|source| Some(source.index) == current)
        .map(info);
    if let Some(layout) = &layout {
        remember_text_keymap(snapshot.text_keymap);
        super::publish(layout);
    } else {
        clear_layout();
    }
    Ok(layout)
}

fn clear_layout() {
    remember_text_keymap(None);
    super::clear();
}

pub fn available_layouts() -> Result<Vec<KeyboardLayoutInfo>, String> {
    Ok(gnome::snapshot(false)?
        .layouts
        .into_iter()
        .map(info)
        .collect())
}

pub fn set_layout(index: u32) -> Result<(), String> {
    if index >= crate::config::actions::MAX_SLOT {
        return Err("GNOME input source index is out of range".into());
    }
    gnome::action(&format!("switchToKeyboardLayout{}", index + 1))?;
    let _ = current();
    Ok(())
}

static SIGNAL_WATCHER: super::signal_watcher::SignalWatcher =
    super::signal_watcher::SignalWatcher::new();

pub(super) fn interrupt_signal_watcher() {
    SIGNAL_WATCHER.stop();
}

pub(super) fn clear_text_keymap() {
    remember_text_keymap(None);
}

const RECONNECT_DELAY: Duration = Duration::from_secs(1);

pub fn start_watcher() {
    SIGNAL_WATCHER.reset();
    super::register_watcher(
        std::thread::Builder::new()
            .name("layout-gnome".into())
            .spawn(|| SIGNAL_WATCHER.run("gnome", RECONNECT_DELAY, watch_once)),
    );
}

fn watch_once() -> Result<(), String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    if !SIGNAL_WATCHER.connected(connection.clone()) {
        return Ok(());
    }
    if super::watcher_stop_requested() {
        return Ok(());
    }
    let proxy =
        zbus::blocking::Proxy::new(&connection, gnome::SERVICE, gnome::PATH, gnome::INTERFACE)
            .map_err(|error| error.to_string())?;
    let signals = proxy
        .receive_all_signals()
        .map_err(|error| error.to_string())?;
    let _ = current();
    for _ in signals {
        if super::watcher_stop_requested() {
            return Ok(());
        }
        crate::active_window::wake();
        let _ = current();
    }
    if super::watcher_stop_requested() {
        return Ok(());
    }
    clear_layout();
    Err("keyboard signal stream ended".into())
}
