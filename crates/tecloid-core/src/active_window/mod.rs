//! Detection of the currently focused window.
//!
//! A watcher thread caches the latest result in `runtime_state` and emits
//! `CoreEvent::ActiveWindowChanged`; the mapper engine reads the cache to
//! evaluate per-rule app conditions. Where the platform reports focus
//! changes (KWin script, Hyprland and Sway IPC, wlr foreign-toplevel
//! events, the X11 root property, a Windows WinEvent hook) the watcher re-reads on every event and polls
//! only as a slow fallback.
//!
//! Window titles are read only while some condition needs them
//! (`title:` patterns, see `config::app_match`): the app id and the
//! process are enough otherwise, and a title is the most private and the
//! least stable part of a window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(target_os = "linux")]
mod gnome;
#[cfg(target_os = "linux")]
mod kwin;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod terminal;
#[cfg(target_os = "linux")]
mod wlr;

pub use crate::runtime_state::ActiveWindow;

/// Poll interval without focus events.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Safety re-read while focus events arrive, e.g. for X11 titles.
const EVENT_FALLBACK_INTERVAL: Duration = Duration::from_secs(3);

/// The shell (rules, auto mode) needs window titles.
static TITLES_FOR_SHELL: AtomicBool = AtomicBool::new(false);
/// Game-mode process rules need window titles.
static TITLES_FOR_GAME_MODE: AtomicBool = AtomicBool::new(false);
/// Keep transient Tecloid windows out of the rule context.
static IGNORE_OWN_WINDOWS: AtomicBool = AtomicBool::new(false);
static ACTUAL_WINDOW: Mutex<Option<ActiveWindow>> = Mutex::new(None);
static PICKER_WINDOW: Mutex<Option<ActiveWindow>> = Mutex::new(None);
#[cfg(target_os = "linux")]
static TEXT_WINDOW: Mutex<Option<ActiveWindow>> = Mutex::new(None);

#[cfg(target_os = "linux")]
fn is_text_popup(window: &ActiveWindow) -> bool {
    window.app_id == crate::storage::POPUP_APP_ID
}

#[cfg(target_os = "linux")]
fn remember_text_window(current: &Option<ActiveWindow>) {
    if let Ok(mut target) = TEXT_WINDOW.lock() {
        update_text_window(&mut target, current);
    }
}

#[cfg(target_os = "linux")]
fn update_text_window(target: &mut Option<ActiveWindow>, current: &Option<ActiveWindow>) {
    if !current.as_ref().is_some_and(is_text_popup) {
        *target = current.clone();
    }
}

/// An application with an open window, for picking a condition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenWindow {
    pub app_id: String,
    pub process_name: Option<String>,
    pub title: String,
}

pub(crate) fn availability() -> crate::gamemode::DetectorAvailability {
    #[cfg(target_os = "linux")]
    {
        linux::availability()
    }
    // The system API always answers; an empty desktop is no failure.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        crate::gamemode::DetectorAvailability::Available
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        crate::gamemode::DetectorAvailability::Unsupported
    }
}

/// Tool window detection needs but cannot find, e.g. `kdotool`.
#[cfg(target_os = "linux")]
pub(crate) fn missing_tool() -> Option<&'static str> {
    linux::missing_tool()
}

/// Preserve rule context across this application's transient windows.
/// On Linux the settings window has its own app id and can match rules.
pub fn ignore_own_windows(ignore: bool) {
    IGNORE_OWN_WINDOWS.store(ignore, Ordering::SeqCst);
}

/// Whether `process` is this application's executable.
fn is_own_process(process: Option<&str>) -> bool {
    static OWN: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let own = OWN.get_or_init(|| {
        std::env::current_exe()
            .ok()?
            .file_name()?
            .to_str()
            .map(|name| crate::gamemode::process::without_deleted(name).to_lowercase())
    });
    matches!((own, process), (Some(own), Some(process)) if *own == process.to_lowercase())
}

fn is_ignored(window: &ActiveWindow) -> bool {
    ignored_by_policy(window, IGNORE_OWN_WINDOWS.load(Ordering::SeqCst))
}

fn ignored_by_policy(window: &ActiveWindow, ignore_own: bool) -> bool {
    if !ignore_own {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        if window.app_id.eq_ignore_ascii_case(crate::storage::APP_ID) {
            return false;
        }
    }
    is_own_window(&window.app_id, window.process_name.as_deref())
}

fn is_own_window(app_id: &str, process: Option<&str>) -> bool {
    app_id.eq_ignore_ascii_case(crate::storage::APP_ID)
        || app_id.eq_ignore_ascii_case(crate::storage::POPUP_APP_ID)
        || is_own_process(process)
}

fn remember_focus(current: &Option<ActiveWindow>) {
    #[cfg(target_os = "linux")]
    remember_text_window(current);
    if let Ok(mut actual) = ACTUAL_WINDOW.lock() {
        *actual = current.clone();
    }
    if let Ok(mut picker) = PICKER_WINDOW.lock() {
        update_picker_window(&mut picker, current.as_ref());
    }
}

fn update_picker_window(picker: &mut Option<ActiveWindow>, current: Option<&ActiveWindow>) {
    if let Some(window) =
        current.filter(|window| !is_own_window(&window.app_id, window.process_name.as_deref()))
    {
        *picker = Some(window.clone());
    }
}

/// The last focus context outside Tecloid, for the application picker.
pub fn picker_active_window() -> Option<ActiveWindow> {
    PICKER_WINDOW.lock().ok().and_then(|window| window.clone())
}

/// Whether the shell's conditions need window titles. Off by default.
pub fn set_titles_needed(needed: bool) {
    if TITLES_FOR_SHELL.swap(needed, Ordering::SeqCst) != needed {
        wake();
    }
}

pub(crate) fn set_titles_needed_by_game_mode(needed: bool) {
    if TITLES_FOR_GAME_MODE.swap(needed, Ordering::SeqCst) != needed {
        wake();
    }
}

pub(crate) fn titles_needed() -> bool {
    TITLES_FOR_SHELL.load(Ordering::SeqCst) || TITLES_FOR_GAME_MODE.load(Ordering::SeqCst)
}

#[derive(Default)]
struct Control {
    stop: bool,
    refresh: bool,
}

struct Watcher {
    control: Arc<(Mutex<Control>, Condvar)>,
    thread: JoinHandle<()>,
}

static WATCHER: Mutex<Option<Watcher>> = Mutex::new(None);

pub fn cached_active_window() -> Option<ActiveWindow> {
    crate::runtime_state::active_window()
}

/// Re-read the focused window now; event sources call it.
pub(crate) fn wake() {
    if let Ok(watcher) = WATCHER.lock()
        && let Some(watcher) = watcher.as_ref()
        && let Ok(mut control) = watcher.control.0.lock()
    {
        control.refresh = true;
        watcher.control.1.notify_one();
    }
}

pub fn stop_watcher() {
    let watcher = WATCHER.lock().ok().and_then(|mut watcher| watcher.take());
    if let Some(watcher) = watcher {
        if let Ok(mut control) = watcher.control.0.lock() {
            control.stop = true;
            watcher.control.1.notify_one();
        }
        if let Err(error) = watcher.thread.join() {
            log::error!("[active-window] watcher panicked: {error:?}");
        }
    }
}

#[derive(Default)]
struct WindowPublisher {
    last: Option<ActiveWindow>,
}

impl WindowPublisher {
    fn publish(
        &mut self,
        current: Option<ActiveWindow>,
        ignored: bool,
        cache: &crate::runtime_state::RuntimeState,
        emit: impl FnOnce(crate::events::CoreEvent),
    ) -> bool {
        if current == self.last || ignored {
            return false;
        }
        cache.set_active_window(current.clone());
        emit(crate::events::CoreEvent::ActiveWindowChanged(
            current.clone(),
        ));
        self.last = current;
        true
    }
}

pub fn start_watcher() {
    let Ok(mut slot) = WATCHER.lock() else {
        return;
    };
    if slot
        .as_ref()
        .is_some_and(|watcher| !watcher.thread.is_finished())
    {
        return;
    }
    let control = Arc::new((Mutex::new(Control::default()), Condvar::new()));
    let thread_control = control.clone();
    let thread = thread::Builder::new()
        .name("active-window-watcher".into())
        .spawn(move || {
            let events = start_events();
            let mut publisher = WindowPublisher::default();
            loop {
                {
                    let Ok(mut control) = thread_control.0.lock() else {
                        break;
                    };
                    if control.stop {
                        break;
                    }
                    control.refresh = false;
                }
                let current = detect_active_window();
                remember_focus(&current);
                let ignored = current.as_ref().is_some_and(is_ignored);
                if publisher.publish(
                    current,
                    ignored,
                    crate::runtime_state::current(),
                    crate::events::emit,
                ) {
                    // Fullscreen and foreground rules depend on the window.
                    crate::gamemode::wake();
                }
                let interval = if events_active(events) {
                    EVENT_FALLBACK_INTERVAL
                } else {
                    POLL_INTERVAL
                };
                let Ok(control) = thread_control.0.lock() else {
                    break;
                };
                let Ok((control, _)) =
                    thread_control
                        .1
                        .wait_timeout_while(control, interval, |control| {
                            !control.stop && !control.refresh
                        })
                else {
                    break;
                };
                if control.stop {
                    break;
                }
            }
            stop_events();
        });
    match thread {
        Ok(thread) => *slot = Some(Watcher { control, thread }),
        Err(error) => log::error!("[active-window] failed to spawn watcher: {error}"),
    }
}

/// Start the platform's focus-change events; `false` when there are none.
fn start_events() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::start_events()
    }
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::start_focus_events()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

/// Whether the events started by [`start_events`] still arrive.
fn events_active(started: bool) -> bool {
    #[cfg(target_os = "linux")]
    {
        let _ = started;
        linux::events_active()
    }
    #[cfg(not(target_os = "linux"))]
    {
        started
    }
}

fn stop_events() {
    #[cfg(target_os = "linux")]
    linux::stop_events();
    #[cfg(target_os = "windows")]
    crate::platform::windows::stop_focus_events();
}

/// Whether the foreground window is a terminal emulator or console.
#[cfg(target_os = "windows")]
pub(crate) fn focused_is_terminal() -> bool {
    crate::platform::windows::active_window(false)
        .is_some_and(|window| terminal::is_terminal(&window))
}

fn detect_active_window() -> Option<ActiveWindow> {
    detect_active_window_with_titles(titles_needed())
}

fn detect_active_window_with_titles(titles: bool) -> Option<ActiveWindow> {
    #[cfg(target_os = "linux")]
    let window = linux::detect(titles);
    #[cfg(target_os = "windows")]
    let window = crate::platform::windows::active_window(titles);
    #[cfg(target_os = "macos")]
    let window = crate::platform::macos::active_window();
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    let window: Option<ActiveWindow> = None;
    window.map(|mut window| {
        if !titles {
            window.title.clear();
        }
        window
    })
}

/// Detect synchronously and refresh the cache (used before showing popups).
/// An ignored window (see [`ignore_own_windows`]) leaves the rule cache as is.
pub fn detect_active_window_now() -> Option<ActiveWindow> {
    let current = detect_active_window();
    remember_focus(&current);
    if current.as_ref().is_some_and(is_ignored) {
        return cached_active_window();
    }
    crate::runtime_state::set_active_window(current.clone());
    current
}

/// Applications with open windows, one entry per app id, process and title,
/// sorted by app id. Asked on demand when the user picks a condition, so
/// titles are included.
pub fn open_windows() -> Vec<OpenWindow> {
    #[cfg(target_os = "linux")]
    let mut windows = linux::open_windows();
    #[cfg(target_os = "windows")]
    let mut windows = crate::platform::windows::open_windows();
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let mut windows: Vec<OpenWindow> = Vec::new();
    windows.retain(|window| {
        (!window.app_id.is_empty() || window.process_name.is_some())
            && !(IGNORE_OWN_WINDOWS.load(Ordering::SeqCst)
                && is_own_window(&window.app_id, window.process_name.as_deref()))
    });
    windows.sort_by_key(|window| {
        (
            window.app_id.to_lowercase(),
            window
                .process_name
                .clone()
                .unwrap_or_default()
                .to_lowercase(),
            window.title.to_lowercase(),
        )
    });
    windows.dedup_by(|a, b| {
        a.app_id.eq_ignore_ascii_case(&b.app_id)
            && a.process_name.as_deref().map(str::to_lowercase)
                == b.process_name.as_deref().map(str::to_lowercase)
            && a.title == b.title
    });
    windows
}

/// What the focused window is, as far as standard shortcuts care.
#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusKind {
    /// No window could be determined.
    Unknown,
    Window,
    Terminal,
    /// Konsole or Yakuake: copy with Ctrl+Insert.
    KonsoleTerminal,
}

/// A text destination, captured before a popup or an asynchronous backend starts.
#[cfg(target_os = "linux")]
#[derive(Clone)]
pub(crate) enum TextTarget {
    Kwin(String, ActiveWindow),
    Gnome(String, ActiveWindow),
    Cached(ActiveWindow),
    /// No detector exists for this environment; allow best-effort input.
    Unsupported,
    /// A supported detector has no usable destination; never guess a window.
    Unavailable,
}

#[cfg(target_os = "linux")]
impl TextTarget {
    pub(crate) fn capture() -> Self {
        let session = crate::platform::linux::detect();
        let kwin_active = kwin::active();
        if kwin_active
            || matches!(
                (&session.desktop, session.session_type),
                (
                    crate::platform::linux::Desktop::Kde,
                    crate::platform::linux::SessionType::Wayland
                )
            )
        {
            if !kwin_active {
                return Self::Unavailable;
            }
            return kwin::text_target()
                .map_or(Self::Unavailable, |(id, window)| Self::Kwin(id, window));
        }
        if matches!(
            session,
            crate::platform::linux::Session {
                desktop: crate::platform::linux::Desktop::Gnome,
                session_type: crate::platform::linux::SessionType::Wayland,
                ..
            }
        ) {
            return gnome::text_target()
                .map_or(Self::Unavailable, |(id, window)| Self::Gnome(id, window));
        }
        let target = TEXT_WINDOW.lock().ok().and_then(|target| target.clone());
        match target {
            Some(target) => Self::Cached(target),
            None if matches!(
                linux::availability(),
                crate::gamemode::DetectorAvailability::Unsupported
            ) =>
            {
                Self::Unsupported
            }
            None => Self::Unavailable,
        }
    }

    pub(crate) fn is_terminal(&self) -> bool {
        self.window().is_some_and(terminal::is_terminal)
    }

    /// The kind of the focused window for standard shortcuts.
    pub(crate) fn focus_kind(&self) -> FocusKind {
        match self.window() {
            None => FocusKind::Unknown,
            Some(window) if terminal::copies_with_ctrl_insert(window) => FocusKind::KonsoleTerminal,
            Some(window) if terminal::is_terminal(window) => FocusKind::Terminal,
            Some(_) => FocusKind::Window,
        }
    }

    fn window(&self) -> Option<&ActiveWindow> {
        match self {
            Self::Kwin(_, window) | Self::Gnome(_, window) | Self::Cached(window) => Some(window),
            Self::Unsupported | Self::Unavailable => None,
        }
    }

    pub(crate) fn is_current(&self) -> bool {
        match self {
            Self::Kwin(id, _) => kwin::target_is_current(id),
            Self::Gnome(id, _) => gnome::target_is_current(id),
            Self::Cached(target) => ACTUAL_WINDOW
                .lock()
                .ok()
                .and_then(|current| current.clone())
                .is_some_and(|current| same_text_application(&current, target)),
            // Without a detector there is no identity to enforce. Lifetime and
            // modifier checks still apply; use the main paste shortcut.
            Self::Unsupported => true,
            Self::Unavailable => false,
        }
    }
}

#[cfg(target_os = "linux")]
fn same_text_application(current: &ActiveWindow, target: &ActiveWindow) -> bool {
    current.app_id == target.app_id && current.process_name == target.process_name
}

/// A text destination: the foreground window and its process.
#[cfg(target_os = "windows")]
#[derive(Clone)]
pub(crate) struct TextTarget(Option<(isize, u32)>);

#[cfg(target_os = "windows")]
impl TextTarget {
    pub(crate) fn capture() -> Self {
        Self(crate::platform::windows::foreground_window())
    }

    pub(crate) fn is_current(&self) -> bool {
        self.0.is_some() && crate::platform::windows::foreground_window() == self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn text_detection_distinguishes_unsupported_and_unavailable_windows() {
        assert!(TextTarget::Unsupported.is_current());
        assert!(!TextTarget::Unsupported.is_terminal());
        assert!(!TextTarget::Unavailable.is_current());
        assert!(!TextTarget::Unavailable.is_terminal());
        let original = ActiveWindow {
            app_id: "editor".into(),
            title: "document".into(),
            ..Default::default()
        };
        let mut current = original.clone();
        current.title = "*document".into();
        assert!(same_text_application(&current, &original));
        current.app_id = "other".into();
        assert!(!same_text_application(&current, &original));
        let target = TextTarget::Kwin(
            "window-id".into(),
            ActiveWindow {
                app_id: "org.kde.konsole".into(),
                ..Default::default()
            },
        );
        assert!(target.is_terminal());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn isolated_window_publisher_preserves_focus_across_ignored_windows() {
        let cache = crate::runtime_state::RuntimeState::default();
        let window = Some(ActiveWindow {
            app_id: "kate".into(),
            ..Default::default()
        });
        let own = Some(ActiveWindow {
            app_id: "settings".into(),
            ..Default::default()
        });
        let mut publisher = WindowPublisher::default();
        let mut events = Vec::new();
        assert!(publisher.publish(window.clone(), false, &cache, |event| events.push(event)));
        assert!(!publisher.publish(own, true, &cache, |event| events.push(event)));
        assert_eq!(cache.active_window(), window);
        assert!(!publisher.publish(window, false, &cache, |event| events.push(event)));
        assert!(publisher.publish(None, false, &cache, |event| events.push(event)));
        assert!(cache.active_window().is_none());
        assert_eq!(events.len(), 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn settings_switch_rule_context_while_popups_and_picker_keep_their_context() {
        use crate::config::auto_switch::{AutoSwitchContext, choose};
        use crate::config::model::{AppSettings, AutoRule, KeymapConditionSet};

        let qemu = ActiveWindow {
            app_id: "qemu".into(),
            ..Default::default()
        };
        let settings = ActiveWindow {
            app_id: crate::storage::APP_ID.into(),
            process_name: std::env::current_exe()
                .unwrap()
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            ..Default::default()
        };
        let popup = ActiveWindow {
            app_id: crate::storage::POPUP_APP_ID.into(),
            ..Default::default()
        };
        let keymap = "user:Main".to_owned();
        let config = AppSettings {
            auto_rules: vec![AutoRule {
                id: "exclude-qemu".into(),
                enabled: true,
                conditions: KeymapConditionSet {
                    excluded_window_patterns: vec!["qemu".into()],
                    ..Default::default()
                },
                keymap_id: Some(keymap.clone()),
            }],
            ..Default::default()
        };
        let cache = crate::runtime_state::RuntimeState::default();
        let mut publisher = WindowPublisher::default();
        let mut picker = None;
        for (focused, effective, changed) in [
            (&qemu, &qemu, true),
            (&popup, &qemu, false),
            (&settings, &settings, true),
            (&popup, &settings, false),
            (&qemu, &qemu, true),
        ] {
            let current = Some(focused.clone());
            update_picker_window(&mut picker, current.as_ref());
            assert_eq!(picker.as_ref(), Some(&qemu));
            assert_eq!(
                publisher.publish(current, ignored_by_policy(focused, true), &cache, |_| {}),
                changed
            );
            assert_eq!(cache.active_window().as_ref(), Some(effective));
            let choice = choose(
                std::slice::from_ref(&keymap),
                &config,
                &AutoSwitchContext::from_state(&cache),
            );
            assert_eq!(
                choice.keymap_id.as_ref(),
                (effective == &settings).then_some(&keymap)
            );
        }
        update_picker_window(&mut picker, None);
        assert_eq!(picker.as_ref(), Some(&qemu));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn text_destination_is_independent_of_the_rule_cache() {
        let window = |id: &str| {
            Some(ActiveWindow {
                app_id: id.into(),
                ..ActiveWindow::default()
            })
        };
        let settings = window(crate::storage::APP_ID);
        let popup = window(crate::storage::POPUP_APP_ID);
        let mut target = window("org.kde.kate");
        update_text_window(&mut target, &settings);
        assert_eq!(target, settings);
        update_text_window(&mut target, &popup);
        assert_eq!(target, settings);
        update_text_window(&mut target, &None);
        assert!(target.is_none());
    }

    #[test]
    fn own_windows_are_ignored_only_when_asked() {
        let exe = std::env::current_exe().unwrap();
        let own = ActiveWindow {
            app_id: "slint-shell".into(),
            process_name: exe.file_name().unwrap().to_str().map(str::to_uppercase),
            ..ActiveWindow::default()
        };
        let other = ActiveWindow {
            process_name: Some("kate".into()),
            ..ActiveWindow::default()
        };
        assert!(!ignored_by_policy(&own, false));
        assert!(ignored_by_policy(&own, true));
        assert!(!ignored_by_policy(&other, true));
    }
}
