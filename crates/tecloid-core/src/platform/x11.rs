//! X11 session helpers: EWMH window/desktop requests and the XKB group.
//!
//! Every EWMH-compliant window manager (Muffin/Cinnamon, Xfwm, Marco,
//! Openbox, KWin and Mutter on X11) honours these root-window client
//! messages, so the same code serves every X11 desktop without helper
//! binaries. Layout names come from the `_XKB_RULES_NAMES` root property
//! that `setxkbmap` and the desktops' keyboard daemons keep current.

use x11rb::connection::Connection;
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, InputFocus, MapState, ModMask,
    Window,
};
use x11rb::rust_connection::RustConnection;

/// A window-manager or keyboard request on the X11 display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X11Action {
    /// Zero-based desktop.
    SwitchToDesktop(u32),
    /// `+1` next, `-1` previous; clamped to the existing desktops.
    SwitchDesktopBy(i32),
    /// Zero-based desktop for the active window.
    MoveWindowToDesktop(u32),
    MoveWindowBy(i32),
    CloseWindow,
    MinimizeWindow,
    /// Toggle `_NET_WM_STATE_MAXIMIZED_VERT` / `_HORZ`.
    ToggleMaximize {
        vertical: bool,
        horizontal: bool,
    },
    ToggleKeepAbove,
    ToggleShowDesktop,
    /// Zero-based XKB group.
    SwitchLayout(u32),
}

/// One X11 connection with the root window of its default screen.
pub struct Display {
    conn: RustConnection,
    root: Window,
    xkb: bool,
}

/// `_NET_*` source indication: a pager or other tool acting for the user.
const SOURCE_PAGER: u32 = 2;
/// `_NET_WM_STATE` toggle.
const STATE_TOGGLE: u32 = 2;
/// ICCCM `IconicState` for `WM_CHANGE_STATE`.
const ICONIC_STATE: u32 = 3;
/// XKB supports at most four groups.
pub const MAX_GROUPS: u32 = 4;

fn err(error: impl std::fmt::Display) -> String {
    format!("X11: {error}")
}

pub(crate) fn xtest_available() -> bool {
    x11rb::connect(None).ok().is_some_and(|(conn, _)| {
        conn.query_extension(b"XTEST")
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| reply.present)
    })
}

impl Display {
    /// Connect to `$DISPLAY`.
    pub fn open() -> Result<Self, String> {
        let (conn, screen) = x11rb::connect(None).map_err(err)?;
        let root = conn
            .setup()
            .roots
            .get(screen)
            .ok_or_else(|| err("no such screen"))?
            .root;
        let xkb = conn
            .xkb_use_extension(1, 0)
            .map_err(err)?
            .reply()
            .is_ok_and(|reply| reply.supported);
        Ok(Self { conn, root, xkb })
    }

    fn atom(&self, name: &str) -> Result<u32, String> {
        Ok(self
            .conn
            .intern_atom(false, name.as_bytes())
            .map_err(err)?
            .reply()
            .map_err(err)?
            .atom)
    }

    fn root_cardinal(&self, name: &str, kind: AtomEnum) -> Result<Option<u32>, String> {
        let atom = self.atom(name)?;
        let reply = self
            .conn
            .get_property(false, self.root, atom, kind, 0, 1)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        Ok(reply.value32().and_then(|mut values| values.next()))
    }

    pub(crate) fn active_window(&self) -> Result<Window, String> {
        self.root_cardinal("_NET_ACTIVE_WINDOW", AtomEnum::WINDOW)?
            .filter(|window| *window != 0)
            .ok_or_else(|| "X11: no active window".to_owned())
    }

    fn message(&self, window: Window, kind: &str, data: [u32; 5]) -> Result<(), String> {
        let event = ClientMessageEvent::new(32, window, self.atom(kind)?, data);
        self.conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(err)?;
        self.conn.flush().map_err(err)
    }

    /// Give a mapped popup keyboard focus in response to an explicit show request.
    pub fn focus_popup(&self, window: Window) -> Result<(), String> {
        let attributes = self
            .conn
            .get_window_attributes(window)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        if attributes.map_state != MapState::VIEWABLE {
            return Ok(());
        }
        self.conn
            .set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
            .map_err(err)?
            .check()
            .map_err(err)
    }

    fn desktops(&self) -> Result<(u32, u32), String> {
        let count = self
            .root_cardinal("_NET_NUMBER_OF_DESKTOPS", AtomEnum::CARDINAL)?
            .ok_or_else(|| "X11: the window manager does not report desktops".to_owned())?;
        let current = self
            .root_cardinal("_NET_CURRENT_DESKTOP", AtomEnum::CARDINAL)?
            .unwrap_or(0);
        Ok((current, count))
    }

    fn checked_desktop(&self, index: u32) -> Result<u32, String> {
        let (_, count) = self.desktops()?;
        if index < count {
            Ok(index)
        } else {
            Err(format!(
                "X11: desktop {} does not exist ({count} desktops)",
                index + 1
            ))
        }
    }

    fn relative_desktop(&self, step: i32) -> Result<u32, String> {
        let (current, count) = self.desktops()?;
        let target = i64::from(current) + i64::from(step);
        Ok(target.clamp(0, i64::from(count.max(1)) - 1) as u32)
    }

    pub fn run(&self, action: X11Action) -> Result<(), String> {
        match action {
            X11Action::SwitchToDesktop(index) => {
                let index = self.checked_desktop(index)?;
                self.message(self.root, "_NET_CURRENT_DESKTOP", [index, 0, 0, 0, 0])
            }
            X11Action::SwitchDesktopBy(step) => {
                let index = self.relative_desktop(step)?;
                self.message(self.root, "_NET_CURRENT_DESKTOP", [index, 0, 0, 0, 0])
            }
            X11Action::MoveWindowToDesktop(index) => {
                let index = self.checked_desktop(index)?;
                let window = self.active_window()?;
                self.message(window, "_NET_WM_DESKTOP", [index, SOURCE_PAGER, 0, 0, 0])
            }
            X11Action::MoveWindowBy(step) => {
                let index = self.relative_desktop(step)?;
                let window = self.active_window()?;
                self.message(window, "_NET_WM_DESKTOP", [index, SOURCE_PAGER, 0, 0, 0])
            }
            X11Action::CloseWindow => {
                let window = self.active_window()?;
                self.message(window, "_NET_CLOSE_WINDOW", [0, SOURCE_PAGER, 0, 0, 0])
            }
            X11Action::MinimizeWindow => {
                let window = self.active_window()?;
                self.message(window, "WM_CHANGE_STATE", [ICONIC_STATE, 0, 0, 0, 0])
            }
            X11Action::ToggleMaximize {
                vertical,
                horizontal,
            } => {
                let window = self.active_window()?;
                let atom = |set: bool, name| if set { self.atom(name) } else { Ok(0) };
                let first = atom(vertical, "_NET_WM_STATE_MAXIMIZED_VERT")?;
                let second = atom(horizontal, "_NET_WM_STATE_MAXIMIZED_HORZ")?;
                let (first, second) = if first == 0 {
                    (second, 0)
                } else {
                    (first, second)
                };
                self.message(
                    window,
                    "_NET_WM_STATE",
                    [STATE_TOGGLE, first, second, SOURCE_PAGER, 0],
                )
            }
            X11Action::ToggleKeepAbove => {
                let window = self.active_window()?;
                let above = self.atom("_NET_WM_STATE_ABOVE")?;
                self.message(
                    window,
                    "_NET_WM_STATE",
                    [STATE_TOGGLE, above, 0, SOURCE_PAGER, 0],
                )
            }
            X11Action::ToggleShowDesktop => {
                let showing = self
                    .root_cardinal("_NET_SHOWING_DESKTOP", AtomEnum::CARDINAL)?
                    .unwrap_or(0);
                self.message(
                    self.root,
                    "_NET_SHOWING_DESKTOP",
                    [u32::from(showing == 0), 0, 0, 0, 0],
                )
            }
            X11Action::SwitchLayout(group) => self.lock_group(group),
        }
    }

    fn require_xkb(&self) -> Result<(), String> {
        if self.xkb {
            Ok(())
        } else {
            Err("X11: the XKB extension is not available".into())
        }
    }

    /// Effective XKB group of the core keyboard.
    pub fn group(&self) -> Result<u32, String> {
        self.require_xkb()?;
        let reply = self
            .conn
            .xkb_get_state(xkb::ID::USE_CORE_KBD.into())
            .map_err(err)?
            .reply()
            .map_err(err)?;
        Ok(u32::from(u8::from(reply.group)))
    }

    /// Lock the core keyboard to group `group` (zero-based). Desktop
    /// indicators follow the XKB state, so no desktop API is needed.
    pub fn lock_group(&self, group: u32) -> Result<(), String> {
        self.require_xkb()?;
        if group < MAX_GROUPS
            && crate::platform::linux::detect().desktop == crate::platform::linux::Desktop::Cinnamon
        {
            // Muffin owns the locked group on Cinnamon 6.6 and restores it
            // after external XKB requests. Older versions use XKB directly.
            let bus = zbus::blocking::Connection::session().map_err(err)?;
            match bus.call_method(
                Some("org.Cinnamon"),
                "/org/Cinnamon",
                Some("org.Cinnamon"),
                "ActivateInputSourceIndex",
                &(group as i32,),
            ) {
                Ok(_) => return Ok(()),
                Err(zbus::Error::MethodError(name, _, _))
                    if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod" => {}
                Err(error) => return Err(err(error)),
            }
        }
        let layouts = self.layouts()?;
        if group >= MAX_GROUPS || (!layouts.is_empty() && group as usize >= layouts.len()) {
            return Err(format!(
                "X11: keyboard layout {} is not configured",
                group + 1
            ));
        }
        self.conn
            .xkb_latch_lock_state(
                xkb::ID::USE_CORE_KBD.into(),
                ModMask::from(0u16),
                ModMask::from(0u16),
                true,
                xkb::Group::from(group as u8),
                ModMask::from(0u16),
                false,
                0,
            )
            .map_err(err)?
            .check()
            .map_err(err)
    }

    /// Configured `(layout, variant)` pairs in group order.
    pub fn layouts(&self) -> Result<Vec<(String, String)>, String> {
        let atom = self.atom("_XKB_RULES_NAMES")?;
        let reply = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::STRING, 0, 1024)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        Ok(parse_rules_names(&reply.value))
    }
}

/// `_XKB_RULES_NAMES` is `rules\0model\0layouts\0variants\0options\0`.
pub(crate) fn parse_rules_names(value: &[u8]) -> Vec<(String, String)> {
    let fields: Vec<_> = value
        .split(|byte| *byte == 0)
        .map(String::from_utf8_lossy)
        .collect();
    let layouts = fields.get(2).map(|s| s.trim()).unwrap_or_default();
    if layouts.is_empty() {
        return Vec::new();
    }
    let variants: Vec<&str> = fields
        .get(3)
        .map(|s| s.split(',').collect())
        .unwrap_or_default();
    layouts
        .split(',')
        .take(MAX_GROUPS as usize)
        .enumerate()
        .map(|(index, layout)| {
            (
                layout.trim().to_owned(),
                variants
                    .get(index)
                    .map(|v| v.trim())
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_rules_names;

    #[test]
    fn rules_names_give_layouts_and_variants() {
        let names = parse_rules_names(b"evdev\0pc105\0us,ru\0dvorak,\0grp:alt_shift_toggle\0");
        assert_eq!(
            names,
            [("us".into(), "dvorak".into()), ("ru".into(), String::new())]
        );
        assert_eq!(
            parse_rules_names(b"evdev\0pc105\0de\0\0\0"),
            [("de".into(), String::new())]
        );
        assert!(parse_rules_names(b"evdev\0pc105\0\0\0\0").is_empty());
        assert!(parse_rules_names(b"").is_empty());
        assert_eq!(parse_rules_names(b"evdev\0pc105\0a,b,c,d,e\0\0\0").len(), 4);
    }
}
