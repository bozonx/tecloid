//! Platform + session detection layer.
//!
//! Three OS backends — `linux`, `windows`, `macos`. Linux with KDE is the
//! primary accepted environment; Windows has its own implementations, and
//! modules a platform lacks stay in place as compile-safe stubs so the call
//! sites remain explicit.
//!
//! Inside the Linux backend we further dispatch to per-desktop
//! implementations (KDE, GNOME, Sway, generic X11) for APIs that are
//! DE-specific:
//!
//!   * `keyboard_layout::current()` / `keyboard_layout::start_watcher()`   — which DBus / CLI tool
//!     tells us the active keyboard layout.
//!   * `mapper::system::resolve()`                       — how to implement
//!     "system functions" (virtual-desktop switching, etc.)
//!
//! The low-level key-event interception (evdev + uinput) is DE-agnostic,
//! so it lives directly in `mapper::linux` without further branching.
//!
//! On Windows the mapper uses session-wide low-level hooks (`mapper::windows`).
//! Missing macOS bindings return a clear, actionable error instead of
//! panicking.

pub mod accessibility;
#[cfg(target_os = "linux")]
pub mod dbus;
#[cfg(target_os = "linux")]
pub mod file_chooser;
#[cfg(target_os = "windows")]
#[path = "file_chooser_windows.rs"]
pub mod file_chooser;
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub mod file_chooser {
    /// Native file selection is not implemented outside the Linux portal backend.
    pub fn pick(_title: &str, _directory: bool) -> Result<Option<std::path::PathBuf>, String> {
        Err("File selection is not implemented on this platform".into())
    }
}
#[cfg(target_os = "linux")]
pub mod desktop_entries;
#[cfg(target_os = "linux")]
pub mod gnome;
#[cfg(target_os = "linux")]
pub mod kde_shortcuts;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod locale;
#[cfg(target_os = "linux")]
pub mod osd;
mod osd_queue;
#[cfg(target_os = "linux")]
pub mod x11;
#[cfg(not(target_os = "linux"))]
pub mod osd {
    pub use super::osd_queue::enqueue;

    pub fn show(_text: &str) -> Result<(), String> {
        Err("on-screen display is not implemented on this platform".into())
    }
}

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "windows")]
pub(crate) mod windows;
/// Directory of the evdev input devices.
#[cfg(target_os = "linux")]
pub const INPUT_DIR: &str = "/dev/input";
/// Device used to create the virtual keyboard.
#[cfg(target_os = "linux")]
pub const UINPUT_PATH: &str = "/dev/uinput";
/// Type text as Unicode characters into the focused window.
#[cfg(target_os = "windows")]
pub use crate::mapper::windows::{type_unicode, type_unicode_with_progress};

#[cfg(target_os = "linux")]
use std::fs::{self, OpenOptions};
#[cfg(target_os = "linux")]
use std::io::ErrorKind;
#[cfg(target_os = "linux")]
use std::path::Path;

/// Why the mapper cannot reach the devices it reads from or emits through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAccessIssue {
    /// No `/dev/input/event*` device exists.
    NoInputDevices,
    /// The selected keyboard device does not exist.
    KeyboardMissing,
    /// Input devices exist but cannot be opened for reading.
    InputDenied,
    /// `/dev/uinput` does not exist: the kernel module is not loaded.
    UinputMissing,
    /// `/dev/uinput` cannot be opened for reading and writing.
    UinputDenied,
}

/// Classify a denied event node without opening it. Missing sysfs data remains
/// a possible access problem (for example during device removal).
#[cfg(target_os = "linux")]
pub(crate) fn remappable_input(path: &Path) -> bool {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if path.parent() != Some(Path::new(INPUT_DIR)) {
        return true;
    }
    let Some(name) = path.file_name() else {
        return true;
    };
    let Ok(bitmap) = fs::read_to_string(
        Path::new("/sys/class/input")
            .join(name)
            .join("device/capabilities/key"),
    ) else {
        return true;
    };
    remappable_keys(&bitmap)
}

#[cfg(target_os = "linux")]
fn remappable_keys(bitmap: &str) -> bool {
    let words: Option<Vec<usize>> = bitmap
        .split_whitespace()
        .rev()
        .map(|word| usize::from_str_radix(word, 16).ok())
        .collect();
    let Some(words) = words else {
        return true;
    };
    let has = |bit: usize| {
        words
            .get(bit / usize::BITS as usize)
            .is_some_and(|word| word & (1usize << (bit % usize::BITS as usize)) != 0)
    };
    // KEY_A/S/D/F, SPACE, ENTER; BTN_LEFT/RIGHT, matching discovery.
    [30, 31, 32, 33, 57, 28].into_iter().all(has) || (has(272) && has(273))
}

/// Check the device access the mapper needs before it is started. With a
/// `keyboard` the check opens that device, otherwise every input device:
/// one readable mouse must not hide a denied keyboard.
/// Empty on platforms without key interception.
pub fn input_access_issues(keyboard: Option<&str>) -> Vec<InputAccessIssue> {
    #[cfg(target_os = "linux")]
    {
        input_access_issues_at(
            Path::new(INPUT_DIR),
            Path::new(UINPUT_PATH),
            keyboard.map(Path::new),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = keyboard;
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn input_access_issues_at(
    input_dir: &Path,
    uinput: &Path,
    keyboard: Option<&Path>,
) -> Vec<InputAccessIssue> {
    let mut issues = Vec::new();
    let input = match keyboard {
        Some(path) => OpenOptions::new()
            .read(true)
            .open(path)
            .map(|_| ())
            .map_err(|error| match error.kind() {
                ErrorKind::NotFound => InputAccessIssue::KeyboardMissing,
                _ => InputAccessIssue::InputDenied,
            }),
        None => {
            let events: Vec<_> = fs::read_dir(input_dir)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("event"))
                })
                .collect();
            if events.is_empty() {
                Err(InputAccessIssue::NoInputDevices)
            } else if events.iter().any(|path| {
                OpenOptions::new()
                    .read(true)
                    .open(path)
                    .is_err_and(|error| {
                        error.kind() == ErrorKind::PermissionDenied && remappable_input(path)
                    })
            }) {
                Err(InputAccessIssue::InputDenied)
            } else {
                Ok(())
            }
        }
    };
    issues.extend(input.err());
    if let Err(error) = OpenOptions::new().read(true).write(true).open(uinput) {
        issues.push(match error.kind() {
            ErrorKind::NotFound => InputAccessIssue::UinputMissing,
            _ => InputAccessIssue::UinputDenied,
        });
    }
    issues
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{InputAccessIssue, input_access_issues_at};

    #[test]
    fn denied_nodes_are_classified_without_opening_them() {
        fn bitmap(bits: &[usize]) -> String {
            let mut words = vec![0usize; 274usize.div_ceil(usize::BITS as usize)];
            for bit in bits {
                words[bit / usize::BITS as usize] |= 1usize << (bit % usize::BITS as usize);
            }
            words
                .iter()
                .rev()
                .map(|word| format!("{word:x}"))
                .collect::<Vec<_>>()
                .join(" ")
        }
        assert!(super::remappable_keys(&bitmap(&[30, 31, 32, 33, 57, 28])));
        assert!(super::remappable_keys(&bitmap(&[272, 273])));
        for bits in [&[116][..], &[113, 114, 115], &[], &[272]] {
            assert!(!super::remappable_keys(&bitmap(bits)));
        }
        assert!(super::remappable_keys("invalid"));
    }

    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "tecloid-platform-{prefix}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn reports_missing_devices() {
        let temp = TempDir::new("probe-empty");
        let issues = input_access_issues_at(&temp.path, &temp.path.join("uinput"), None);
        assert_eq!(
            issues,
            [
                InputAccessIssue::NoInputDevices,
                InputAccessIssue::UinputMissing
            ]
        );
    }

    #[test]
    fn readable_devices_have_no_issues() {
        let temp = TempDir::new("probe-ok");
        fs::write(temp.path.join("event0"), "").expect("event");
        fs::write(temp.path.join("uinput"), "").expect("uinput");
        let uinput = temp.path.join("uinput");
        assert!(input_access_issues_at(&temp.path, &uinput, None).is_empty());
        let keyboard = temp.path.join("event0");
        assert!(input_access_issues_at(&temp.path, &uinput, Some(&keyboard)).is_empty());
    }

    #[test]
    fn one_denied_device_is_reported_even_if_another_is_readable() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: geteuid has no preconditions.
        if unsafe { libc::geteuid() } == 0 {
            return; // root opens any file
        }
        let temp = TempDir::new("probe-partial");
        fs::write(temp.path.join("event0"), "").expect("mouse");
        let keyboard = temp.path.join("event1");
        fs::write(&keyboard, "").expect("keyboard");
        fs::set_permissions(&keyboard, fs::Permissions::from_mode(0o000)).expect("chmod");
        fs::write(temp.path.join("uinput"), "").expect("uinput");
        let issues = input_access_issues_at(&temp.path, &temp.path.join("uinput"), None);
        assert_eq!(issues, [InputAccessIssue::InputDenied]);
    }

    #[test]
    fn reports_a_missing_keyboard() {
        let temp = TempDir::new("probe-keyboard");
        fs::write(temp.path.join("uinput"), "").expect("uinput");
        let issues = input_access_issues_at(
            &temp.path,
            &temp.path.join("uinput"),
            Some(&temp.path.join("event7")),
        );
        assert_eq!(issues, [InputAccessIssue::KeyboardMissing]);
    }
}

#[cfg(target_os = "macos")]
pub use macos::{
    NativeTextTarget, capture_text_target, restore_text_target, text_modifiers_released,
};
