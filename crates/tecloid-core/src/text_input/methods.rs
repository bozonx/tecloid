//! Non-interactive discovery. Consent and the focused field are checked at delivery.
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;

use crate::config::model::{AppSettings, TextMode};
use crate::platform::linux::{Desktop, SessionType, command_available, detect};

pub struct Methods {
    pub supported: Vec<TextMode>,
    pub recommended: TextMode,
}

pub fn discover(settings: &AppSettings) -> Methods {
    let session = detect();
    let wayland = session.session_type == SessionType::Wayland;
    let x11 = session.session_type == SessionType::X11;
    let keyboard = super::portal::keyboard_portal_version();
    let clipboard = wayland
        && (super::clipboard::data_control_available()
            || super::portal::clipboard_portal_available());
    let commit = wayland
        && session.desktop == Desktop::Gnome
        && crate::platform::gnome::supports_commit_text();
    let direct_text =
        wayland && session.desktop == Desktop::Kde && keyboard >= 2 && kwin_advertises_eis_text();
    let tool = |path: &str, name| command_available(if path.is_empty() { name } else { path });
    let xtest = x11 && crate::platform::x11::xtest_available();
    let xdotool = xtest && tool(&settings.linux_xdotool_path, "xdotool");
    let graphical = wayland || x11;
    let socket = std::env::var_os("YDOTOOL_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(".ydotool_socket")
        });
    let ydotool = graphical
        && tool(&settings.linux_ydotool_path, "ydotool")
        && UnixDatagram::unbound()
            .and_then(|client| client.connect(socket))
            .is_ok();
    let supported = |mode| match mode {
        TextMode::GnomeCommitText => commit,
        TextMode::LibeiWithClipboardFallback => keyboard >= 2 && clipboard,
        TextMode::LibeiOnly => keyboard >= 2,
        TextMode::LibeiText => direct_text,
        TextMode::Keycode => keyboard >= 1,
        TextMode::Clipboard => keyboard >= 1 && clipboard,
        TextMode::Xdotool => xtest,
        TextMode::Ydotool => graphical,
    };
    let recommended = [
        TextMode::LibeiText,
        TextMode::GnomeCommitText,
        TextMode::Xdotool,
        TextMode::LibeiWithClipboardFallback,
        TextMode::LibeiOnly,
        TextMode::Clipboard,
        TextMode::Keycode,
        TextMode::Ydotool,
    ]
    .into_iter()
    .find(|mode| match mode {
        TextMode::Xdotool => xdotool,
        TextMode::Ydotool => ydotool,
        mode => supported(*mode),
    })
    .unwrap_or(if xtest {
        TextMode::Xdotool
    } else {
        TextMode::LibeiWithClipboardFallback
    });
    Methods {
        supported: TextMode::ALL
            .into_iter()
            .filter(|mode| supported(*mode))
            .collect(),
        recommended,
    }
}

/// KWin added EIS text in 6.8. Inspect the running compositor and its loaded
/// plugin without opening a RemoteDesktop session or prompting for consent.
fn kwin_advertises_eis_text() -> bool {
    use crate::platform::dbus::{KWIN_INTERFACE, KWIN_PATH, KWIN_SERVICE};
    use zbus::blocking::{Proxy, connection::Builder};

    let Ok(conn) = Builder::session().and_then(|builder| {
        builder
            .method_timeout(std::time::Duration::from_secs(1))
            .build()
    }) else {
        return false;
    };
    let Ok(proxy) = Proxy::new(&conn, KWIN_SERVICE, KWIN_PATH, KWIN_INTERFACE) else {
        return false;
    };
    proxy
        .call::<_, _, String>("supportInformation", &())
        .is_ok_and(|info| kwin_has_eis_text(&info))
}

fn kwin_has_eis_text(info: &str) -> bool {
    let version = info.lines().find_map(|line| {
        let version = line.strip_prefix("KWin version: ")?;
        let (major, rest) = version.split_once('.')?;
        let minor = rest.split('.').next()?;
        Some((major.parse::<u32>().ok()?, minor.parse::<u32>().ok()?))
    });
    version.is_some_and(|version| version >= (6, 8))
        && info
            .lines()
            .skip_while(|line| *line != "Loaded Plugins:")
            .skip(1)
            .take_while(|line| !line.is_empty())
            .any(|line| line.trim() == "eis")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eis_text_requires_recent_kwin_with_loaded_eis_plugin() {
        let report = "KWin version: 6.8.2\nLoaded Plugins:\n---------------\neis\n\nAvailable Plugins:\n---------------\neis\n";
        assert!(kwin_has_eis_text(report));
        assert!(!kwin_has_eis_text(&report.replace("6.8.2", "6.7.91")));
        assert!(!kwin_has_eis_text(&report.replacen("\neis\n", "\n", 1)));
    }
}
