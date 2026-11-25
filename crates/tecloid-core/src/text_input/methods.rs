//! Non-interactive discovery. Consent and the focused field are checked at delivery.
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;

use crate::config::model::{AppSettings, TextMode};
use crate::platform::linux::{SessionType, command_available, detect};

pub struct Methods {
    pub advertised: Vec<bool>,
    pub recommended: TextMode,
}

pub fn discover(settings: &AppSettings) -> Methods {
    let session = detect().session_type;
    let wayland = session == SessionType::Wayland;
    let x11 = session == SessionType::X11;
    let keyboard = super::portal::keyboard_portal_version();
    let clipboard = wayland
        && (super::clipboard::data_control_available()
            || super::portal::clipboard_portal_available());
    let commit = wayland && crate::platform::gnome::supports_commit_text();
    let tool = |path: &str, name| command_available(if path.is_empty() { name } else { path });
    let xtest = x11
        && tool(&settings.linux_xdotool_path, "xdotool")
        && crate::platform::x11::xtest_available();
    let socket = std::env::var_os("YDOTOOL_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(".ydotool_socket")
        });
    let ydotool = tool(&settings.linux_ydotool_path, "ydotool")
        && UnixDatagram::unbound()
            .and_then(|client| client.connect(socket))
            .is_ok();
    let available = |mode| match mode {
        TextMode::GnomeCommitText => commit,
        TextMode::LibeiWithClipboardFallback => keyboard >= 2 && clipboard,
        TextMode::LibeiOnly => keyboard >= 2,
        TextMode::Keycode => keyboard >= 1,
        TextMode::Clipboard => keyboard >= 1 && clipboard,
        TextMode::Xdotool => xtest,
        TextMode::Ydotool => ydotool,
    };
    let recommended = [
        TextMode::GnomeCommitText,
        TextMode::Xdotool,
        TextMode::LibeiWithClipboardFallback,
        TextMode::LibeiOnly,
        TextMode::Clipboard,
        TextMode::Keycode,
        TextMode::Ydotool,
    ]
    .into_iter()
    .find(|mode| available(*mode))
    .unwrap_or(if x11 {
        TextMode::Xdotool
    } else {
        TextMode::LibeiWithClipboardFallback
    });
    Methods {
        advertised: TextMode::ALL.into_iter().map(available).collect(),
        recommended,
    }
}
