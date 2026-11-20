//! `sys:` actions on Windows. Window actions post documented window
//! messages to the foreground window, layouts use `WM_INPUTLANGCHANGEREQUEST`,
//! show desktop uses the Shell's `ToggleDesktop`, lock uses
//! `LockWorkStation`, and the remaining shell and window actions press their
//! fixed system shortcuts (Win+Tab, Ctrl+Esc, Ctrl+Win+Arrow, Win+Arrow
//! tiling, Win+N taskbar buttons, Win+V clipboard history, …). Windows
//! has no public API to switch to virtual desktop N or to move a window to
//! another desktop, so those ids do not resolve. Windows of elevated
//! applications ignore all of this (UIPI) unless this app runs elevated
//! too. Every call returns without waiting for the target.

use super::super::key_code::KeyCode;
use super::super::system::SystemAction;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetForegroundWindow, GetWindowLongW, GetWindowRect, HWND_BROADCAST, HWND_TOPMOST,
    IsZoomed, PostMessageW, SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE, SC_MONITORPOWER, SC_RESTORE,
    SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    WM_SYSCOMMAND, WS_EX_TOPMOST,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsAction {
    /// One-based position in the user's input language list.
    SwitchLayout(u32),
    WindowClose,
    WindowMinimize,
    WindowMaximize,
    WindowMaximizeVertical,
    WindowMaximizeHorizontal,
    WindowKeepAbove,
    WindowToNextScreen,
    WindowToPreviousScreen,
    NextDesktop,
    PreviousDesktop,
    ShowDesktop,
    Overview,
    AppLauncher,
    ScreenOff,
    Lock,
}

pub fn resolve(name: &str) -> Option<SystemAction> {
    let slot = |prefix: &str| {
        name.strip_prefix(prefix)
            .and_then(crate::config::actions::slot_number)
    };
    if let Some(n) = slot("switchToKeyboardLayout") {
        return Some(SystemAction::Windows(WindowsAction::SwitchLayout(n)));
    }
    // Fixed Windows shortcuts: Win+N opens the Nth taskbar button.
    let keys = match name {
        "tileWindowLeft" => Some("Meta+ArrowLeft".to_owned()),
        "tileWindowRight" => Some("Meta+ArrowRight".to_owned()),
        "tileWindowTop" => Some("Meta+ArrowUp".to_owned()),
        "tileWindowBottom" => Some("Meta+ArrowDown".to_owned()),
        "showClipboardHistory" => Some("Meta+KeyV".to_owned()),
        _ => slot("activateTaskManagerEntry")
            .filter(|n| *n <= 10)
            .map(|n| format!("Meta+Digit{}", n % 10)),
    };
    if let Some(keys) = keys {
        return crate::mapper::action::parse_action(&keys).map(SystemAction::Keys);
    }
    Some(SystemAction::Windows(match name {
        "closeWindow" => WindowsAction::WindowClose,
        "minimizeWindow" => WindowsAction::WindowMinimize,
        "maximizeWindow" => WindowsAction::WindowMaximize,
        "maximizeWindowVertically" => WindowsAction::WindowMaximizeVertical,
        "maximizeWindowHorizontally" => WindowsAction::WindowMaximizeHorizontal,
        "keepWindowAbove" => WindowsAction::WindowKeepAbove,
        "moveWindowToNextScreen" => WindowsAction::WindowToNextScreen,
        "moveWindowToPreviousScreen" => WindowsAction::WindowToPreviousScreen,
        "switchToNextDesktop" => WindowsAction::NextDesktop,
        "switchToPreviousDesktop" => WindowsAction::PreviousDesktop,
        "showDesktop" => WindowsAction::ShowDesktop,
        "showOverview" => WindowsAction::Overview,
        "showAppLauncher" => WindowsAction::AppLauncher,
        "turnOffScreen" => WindowsAction::ScreenOff,
        "lockSession" => WindowsAction::Lock,
        _ => return None,
    }))
}

/// The foreground window when it belongs to an application, not to the
/// desktop or taskbar: closing the desktop would open the shutdown dialog.
fn app_window() -> Result<HWND, String> {
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() || crate::platform::windows::is_shell_window(window) {
        return Err("No application window is focused".into());
    }
    Ok(window)
}

fn system_command(command: u32) -> Result<(), String> {
    let window = app_window()?;
    unsafe {
        PostMessageW(
            Some(window),
            WM_SYSCOMMAND,
            WPARAM(command as usize),
            LPARAM(0),
        )
    }
    .map_err(|error| format!("Window command failed: {error}"))
}

fn toggle_keep_above() -> Result<(), String> {
    let window = app_window()?;
    let topmost = unsafe { GetWindowLongW(window, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0;
    // HWND_NOTOPMOST is (HWND)-2.
    let after = if topmost {
        HWND(-2isize as _)
    } else {
        HWND_TOPMOST
    };
    unsafe {
        SetWindowPos(
            window,
            Some(after),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
        )
    }
    .map_err(|error| format!("Keep above failed: {error}"))
}

/// Slack for the invisible resize borders that `GetWindowRect` includes
/// (about 8 px at 100 % scale, more at higher scales).
const EDGE_SLACK: i32 = 24;

/// The window's rectangle and its monitor's work area.
fn window_and_work_area(window: HWND) -> Result<(RECT, RECT), String> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(window, &mut rect) }
        .map_err(|error| format!("Window geometry failed: {error}"))?;
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err("Monitor geometry failed".into());
    }
    Ok((rect, info.rcWork))
}

/// Whether the window spans its monitor's work area from top to bottom.
fn spans_work_area_vertically(window: HWND) -> Result<bool, String> {
    let (rect, work) = window_and_work_area(window)?;
    Ok(
        (rect.top - work.top).abs() <= EDGE_SLACK
            && (rect.bottom - work.bottom).abs() <= EDGE_SLACK,
    )
}

/// Left and right edges of windows stretched by
/// [`toggle_maximize_horizontal`], keyed by window handle, to restore them.
static HORIZONTAL_RESTORE: std::sync::Mutex<Vec<(isize, i32, i32)>> =
    std::sync::Mutex::new(Vec::new());

/// Windows has no shortcut for horizontal maximization, so the window is
/// stretched to the work area width and keeps its height; a second run
/// gives back the remembered edges, like the KDE toggle. A maximized window
/// is left alone.
fn toggle_maximize_horizontal() -> Result<(), String> {
    let window = app_window()?;
    if unsafe { IsZoomed(window) }.as_bool() {
        return Ok(());
    }
    let (rect, work) = window_and_work_area(window)?;
    let spans = (rect.left - work.left).abs() <= EDGE_SLACK
        && (rect.right - work.right).abs() <= EDGE_SLACK;
    let mut saved = HORIZONTAL_RESTORE.lock().unwrap_or_else(|e| e.into_inner());
    let id = window.0 as isize;
    let previous = saved
        .iter()
        .position(|entry| entry.0 == id)
        .map(|i| saved.remove(i));
    let (left, right) = match previous {
        Some((_, left, right)) if spans => (left, right),
        _ if spans => return Ok(()),
        _ => {
            // Keep the list small: handles of closed windows are reused.
            if saved.len() >= 32 {
                saved.remove(0);
            }
            saved.push((id, rect.left, rect.right));
            // Extend by the invisible borders so the frame meets the edges.
            let border = (rect.right - rect.left).min(EDGE_SLACK) / 3;
            (work.left - border, work.right + border)
        }
    };
    unsafe {
        SetWindowPos(
            window,
            None,
            left,
            rect.top,
            right - left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
        )
    }
    .map_err(|error| format!("Maximize horizontally failed: {error}"))
}

/// Win+Shift+Up stretches the window to the work area height; Win+Shift+Down
/// gives it back the height Windows remembered, which makes the action a
/// toggle like on KDE.
fn toggle_maximize_vertical() -> Result<(), String> {
    let window = app_window()?;
    let key = if spans_work_area_vertically(window)? {
        KeyCode::KEY_DOWN
    } else {
        KeyCode::KEY_UP
    };
    super::input::shortcut(&[KeyCode::KEY_LEFTMETA, KeyCode::KEY_LEFTSHIFT], key)
}

/// Minimize or restore every window through the Shell, the documented
/// counterpart of Win+D.
fn toggle_desktop() -> Result<(), String> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::{IShellDispatch4, Shell};
    let error = |error: windows::core::Error| format!("Show desktop failed: {}", error.message());
    // An apartment this thread already has is fine too.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let result =
        unsafe { CoCreateInstance::<_, IShellDispatch4>(&Shell, None, CLSCTX_INPROC_SERVER) }
            .and_then(|shell| unsafe { shell.ToggleDesktop() })
            .map_err(error);
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

/// Delay before the display goes off, so releasing the bound key does not
/// wake it again at once.
const SCREEN_OFF_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

fn turn_off_screen() -> Result<(), String> {
    std::thread::Builder::new()
        .name("tecloid-screen-off".into())
        .spawn(|| {
            std::thread::sleep(SCREEN_OFF_DELAY);
            // 2 = the display is being shut off.
            let posted = unsafe {
                PostMessageW(
                    Some(HWND_BROADCAST),
                    WM_SYSCOMMAND,
                    WPARAM(SC_MONITORPOWER as usize),
                    LPARAM(2),
                )
            };
            if let Err(error) = posted {
                report_failure(format!("Screen off failed: {error}"));
            }
        })
        .map(|_| ())
        .map_err(|error| format!("Screen off failed: {error}"))
}

fn report_failure(error: String) {
    crate::events::emit(crate::events::CoreEvent::MapperActionFailed(error));
}

pub(super) fn run(action: WindowsAction) -> Result<(), String> {
    match action {
        WindowsAction::SwitchLayout(n) => crate::keyboard_layout::set(n - 1),
        WindowsAction::WindowClose => system_command(SC_CLOSE),
        WindowsAction::WindowMinimize => system_command(SC_MINIMIZE),
        WindowsAction::WindowMaximize => {
            let window = app_window()?;
            if unsafe { IsZoomed(window) }.as_bool() {
                system_command(SC_RESTORE)
            } else {
                system_command(SC_MAXIMIZE)
            }
        }
        WindowsAction::WindowMaximizeVertical => toggle_maximize_vertical(),
        WindowsAction::WindowMaximizeHorizontal => toggle_maximize_horizontal(),
        WindowsAction::WindowKeepAbove => toggle_keep_above(),
        WindowsAction::WindowToNextScreen => {
            app_window()?;
            super::input::shortcut(
                &[KeyCode::KEY_LEFTMETA, KeyCode::KEY_LEFTSHIFT],
                KeyCode::KEY_RIGHT,
            )
        }
        WindowsAction::WindowToPreviousScreen => {
            app_window()?;
            super::input::shortcut(
                &[KeyCode::KEY_LEFTMETA, KeyCode::KEY_LEFTSHIFT],
                KeyCode::KEY_LEFT,
            )
        }
        WindowsAction::NextDesktop => super::input::shortcut(
            &[KeyCode::KEY_LEFTCTRL, KeyCode::KEY_LEFTMETA],
            KeyCode::KEY_RIGHT,
        ),
        WindowsAction::PreviousDesktop => super::input::shortcut(
            &[KeyCode::KEY_LEFTCTRL, KeyCode::KEY_LEFTMETA],
            KeyCode::KEY_LEFT,
        ),
        WindowsAction::ShowDesktop => toggle_desktop(),
        WindowsAction::Lock => unsafe { windows::Win32::System::Shutdown::LockWorkStation() }
            .map_err(|error| format!("Lock failed: {error}")),
        WindowsAction::Overview => {
            super::input::shortcut(&[KeyCode::KEY_LEFTMETA], KeyCode::KEY_TAB)
        }
        WindowsAction::AppLauncher => {
            super::input::shortcut(&[KeyCode::KEY_LEFTCTRL], KeyCode::KEY_ESC)
        }
        WindowsAction::ScreenOff => turn_off_screen(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_every_plain_action_but_not_virtual_desktops() {
        for id in crate::config::actions::PLAIN_SYSTEM_ACTIONS {
            assert_eq!(
                resolve(id).is_some(),
                !matches!(
                    id,
                    "moveWindowToNextDesktop" | "moveWindowToPreviousDesktop"
                ),
                "{id}"
            );
        }
        assert!(matches!(
            resolve("switchToKeyboardLayout3"),
            Some(SystemAction::Windows(WindowsAction::SwitchLayout(3)))
        ));
        assert!(matches!(
            resolve("activateTaskManagerEntry10"),
            Some(SystemAction::Keys(_))
        ));
        assert!(resolve("activateTaskManagerEntry11").is_none());
        for id in [
            "switchToKeyboardLayout0",
            "switchToKeyboardLayout11",
            "switchToDesktop1",
            "moveWindowToDesktop2",
        ] {
            assert!(resolve(id).is_none(), "{id}");
        }
    }
}
