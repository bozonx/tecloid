# Platforms

**Supported Linux environments: KDE Plasma 6 on Wayland (primary), GNOME on
Wayland, and Cinnamon on X11 (Linux Mint).** X11 is a supported display server:
Cinnamon is its acceptance target; other X11 desktops (Xfce, MATE, …) use the
same generic X11 backends and are best effort without a VM stand. Other
environments compile and partly work; implemented code is not acceptance.

| Concern | KDE | GNOME | Cinnamon (X11, Linux Mint) | Other X11 (Xfce, MATE, …) | Sway / Hyprland | Windows | macOS |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Key interception | ✅ evdev + uinput | ✅ | ✅ | ✅ | ✅ | ✅ session-wide `WH_KEYBOARD_LL` + `SendInput` | ❌ stub |
| Literal text (`text:`) | ✅ Direct EIS text on compatible KWin, portal libei, data-control clipboard | ⚠ GNOME CommitText, portal keyboard and Clipboard portal | ✅ `xdotool` (XTEST) or `ydotool`, without portal consent | ⚠ same as Cinnamon | ⚠ backend dependent | ✅ Unicode `SendInput` | ❌ mapper; popups only |
| Keyboard layout | ✅ D-Bus `org.kde.keyboard` | ⚠ extension | ✅ XKB group + `_XKB_RULES_NAMES` (≤ 4 layouts) | ✅ same | 🚧 | ✅ foreground window input language | 🚧 |
| Standard shortcuts (`sys:copy`, `sys:undo`, `sys:moveWordLeft`, …) | ✅ layout via XKB, terminals by window class | ✅ | ✅ | ✅ | ✅ | ✅ layout via `VkKeyScanEx` | ❌ mapper |
| System actions (`sys:`) | ✅ kglobalaccel, `org.kde.keyboard`, ScreenSaver | ⚠ extension; Super+Arrow / Super+N for side tiling and task entries 1–9; no `showDesktop`, top/bottom tiling, clipboard history | ✅ EWMH (desktops, window state, show desktop), XKB layouts, Muffin keys for tiling, Expo over `org.Cinnamon`, menu via Super, `cinnamon-screensaver-command`, `xset`; no next/previous screen, task entries, clipboard history | ⚠ EWMH, XKB, `loginctl lock-session`, `xset`; no tiling, overview, launcher | 🚧 | ✅ except virtual-desktop actions | ❌ |
| `kde:` shortcuts | ✅ | — | — | — | — | — | — |
| `xdg:` app actions | ✅ | ✅ | ✅ | ✅ | ✅ | — | — |
| Layer-shell popups (Spell) | ✅ | ❌ winit | ❌ winit | ❌ winit | ✅ not accepted | — | — |
| Active window / fullscreen | ✅ KWin script → D-Bus | ⚠ extension | ✅ `xprop` (+ `xdotool` for titles, `xrandr`) | ✅ same | ✅ IPC; other wlroots via `wlr-foreign-toplevel` | ✅ Win32, `SetWinEventHook` | ⚠ `NSWorkspace`, `CGWindowList` (untested) |
| Game mode: GameMode daemon | ✅ D-Bus | ✅ | ✅ | ✅ | ✅ | — | — |
| Game mode: known games | ✅ Steam, Lutris, Heroic, umu, gamescope, `.desktop` `Game` | ✅ | ✅ | ✅ | ✅ | ✅ Steam `RunningAppID`, Game Bar list | ⚠ `Info.plist` category |

🚧 = skeleton returning `Ok(None)`/`None`; ❌ = explicit "not implemented"
error. Both compile and are safe to ship.

Dispatch on `platform::linux::detect()`, never on raw `XDG_CURRENT_DESKTOP`.
Platform code stays behind `#[cfg]` and target-specific dependencies.

## Linux

Runtime requirements: keyboard/mouse/touchpad event-node read and `/dev/uinput` write (group
`uaccess` in an active local session; `input` is a fallback, see [development.md](development.md#device-access-linux));
`xdg-desktop-portal` with the DE backend for `text:` on Wayland; `hyprctl` /
`swaymsg` on Hyprland / Sway; on X11 `xdotool` (text, title conditions),
`xprop` (window conditions) and `xset` (`sys:turnOffScreen`), which the deb/rpm
recommend (`xdotool`, `x11-utils`/`xprop`, `x11-xserver-utils`/`xset`). KDE Wayland
needs nothing extra (a persistent KWin script reports focus over D-Bus; calls not
from KWin are ignored).

Mouse: the selected mouse or touchpad is grabbed and replayed through
"Tecloid Pointer", which keeps the physical bus/vendor/product id (udev hwdb and
libinput quirks still match). Its buttons go through the engine; axes, touch and
tool codes (`BTN_TOUCH`, `BTN_TOOL_*`) and misc data stay in their `SYN_REPORT`
frame. Touchscreens, pen tablets and game controllers are rejected as mice.
Clicks are never held behind macros, text or popups, since pointer motion is not.

Distribution: tar.gz into `~/.local`, deb and rpm (nfpm), built on Ubuntu 22.04
(glibc 2.35). Flathub/Snap are not possible: a sandbox cannot install the udev
rule or open `/dev/uinput`.

### X11 and Cinnamon

**Supported**, with Cinnamon on Linux Mint as the acceptance target (VM stand
`run-cinnamon.py`, see [testing.md](testing.md#cinnamon-x11)). Dispatch is by
`SessionType::X11` from `platform::linux::detect()`; under Wayland `DISPLAY` is
XWayland and the X11 backends stay off. KDE on X11 keeps its KDE backends;
GNOME no longer ships an X11 session.

- **Interception** is evdev/uinput as everywhere on Linux.
- **Keyboard layout** (`keyboard_layout/linux_x11.rs`): the effective XKB group
  of the core keyboard, layout codes and variants from the `_XKB_RULES_NAMES`
  root property, long names from `evdev.lst`. A watcher polls one X connection
  every 150 ms. `switchToKeyboardLayoutN` locks the XKB group; desktop
  indicators follow. XKB has at most four groups, so N ≤ 4.
- **System actions** (`platform/x11.rs`, `SystemAction::X11`): EWMH client
  messages to the root window on a `tecloid-x11` worker with one connection —
  `_NET_CURRENT_DESKTOP` (numbered, next/previous, clamped), `_NET_WM_DESKTOP`,
  `_NET_CLOSE_WINDOW`, `WM_CHANGE_STATE` (minimize), `_NET_WM_STATE` toggles
  (maximize, vertical/horizontal, keep above), `_NET_SHOWING_DESKTOP`. Cinnamon
  adds Muffin's default Super+Arrow tiling, Super for the menu, `ShowExpo` on
  `org.Cinnamon` for `showOverview` and `cinnamon-screensaver-command --lock`.
  Other X11 desktops lock through `loginctl lock-session`. `turnOffScreen` runs
  `xset dpms force off`. Changed Cinnamon keybindings are not followed.
- **Text**: New X11 configurations prefer `xdotool` when XTEST is available
  and the executable is found. Missing layout characters use a temporary
  keycode. Other available methods can be selected in Settings.
- **Popups** are winit windows; Muffin may refuse focus stealing, the popup
  model then behaves as on GNOME. Focus return relies on the window manager.
  Popup text checks the original window against the current EWMH window ID.
- **Tray**: Cinnamon's `xapp-sn-watcher` provides `org.kde.StatusNotifierWatcher`.
- **Active window / fullscreen**: `xprop -spy` on the root window, titles with
  `xdotool`, fullscreen by `_NET_WM_STATE_FULLSCREEN` or monitor geometry.

### GNOME

Requires the bundled Shell extension `tecloid@bozonx.dev`
(`crates/tecloid-core/assets/gnome-extension/`, targets GNOME Shell 46–51),
installed with `install-gnome-extension.sh` (bundle, `~/.local/lib/dev.bozonx.tecloid/`
or `/usr/share/tecloid/`), enabled after a new login. The tray needs an
AppIndicator host; the app keeps its tray service alive across host restarts and
opens settings while no host exists. No layer-shell: popups use winit.

Bridge: object `/dev/bozonx/Tecloid/Gnome`, interface `dev.bozonx.Tecloid.Gnome`
on the `org.gnome.Shell` bus name. Every method verifies the caller's uid; no
eval, shell commands or arbitrary dispatch.

- `Snapshot(b titles) -> s` (JSON v1: focused and previous window, workspace,
  input sources), `Windows() -> s`, `Activate(s)` (only windows of the caller
  PID), `Action(s)` (fixed allowlist), signal `Changed`.
- Window ids combine a random extension-instance UUID and Mutter's window
  sequence; text delivery rechecks the live identity.
- Actions: `switchToDesktop1..20`, `moveWindowToDesktop1..20`,
  `switchToKeyboardLayout1..10`, window and shell actions, `lockSession`,
  `turnOffScreen`. `showDesktop` is unavailable.

The extension exposes `CommitText(s text, s windowId)`. It checks the caller,
session state, Wayland window identity and input method focus before committing
the complete string. XWayland windows cannot use this method.

Clipboard: the extension does not own the clipboard (GJS cannot implement a
multi-MIME selection source, and binary shims are disallowed by extension review
rules). Text uses RemoteDesktop/libei plus the Clipboard portal requested before
`Start`. Keyboard-only and clipboard sessions keep separate restore tokens; a
denied clipboard grant is respected and libei typing stays available. Restored
clipboard data keeps the RemoteDesktop session (and GNOME's indicator) alive
until another owner replaces it.

## Text input

On Linux, popup text captures the focused window when the popup opens and
allows up to 500 ms from the choice for focus to return. KDE Wayland checks
KWin window IDs, GNOME Wayland uses IDs from the Shell extension, and X11
checks the active EWMH window ID when it can open the X11 display. The fallback
compares app and process. With a supported detector but no target, input is
rejected.

`linux.textMode`: `libeiWithClipboardFallback` types via libei and pastes the
whole text through the clipboard if the layout lacks a character; `libeiOnly`
rejects such text; `portalKeycode`, `clipboardPaste`, `ydotool`, `xdotool` and
`gnomeCommitText` each use their named mechanism. Failures are reported without
trying another method. This applies to single-character actions too.

`libeiText` sends direct UTF-8 text on KDE Plasma Wayland with EIS text support
(KWin 6.8 or later built with libeis 1.6 support). It sends up to 254 UTF-8
bytes per action directly through the RemoteDesktop portal. The portal must
grant keyboard access and advertise an EIS text device. Applications without
an active Wayland text-input context may fail to receive Unicode text.
Select a clipboard mode for those applications.

New configurations on KDE Wayland prefer direct EIS text if the keyboard
portal supports EIS and the running KWin is at least 6.8 with its EIS plugin
loaded. Otherwise they prefer libei with clipboard fallback when available.
New GNOME Wayland configurations prefer CommitText when the extension exposes
it. If no ready method is found, discovery recommends xdotool on X11 with
XTEST and libei with clipboard fallback elsewhere.
Discovery does not request consent. The EIS text device itself, permission
and target-field support are checked when typing. Saved choices are used verbatim.

The settings list is filtered by the running session and its capabilities.
Plain libei, NotifyKeyboardKeycode and ydotool require In development to appear
as alternatives. A supported method that is already selected remains visible.

| Method | Shown when |
| --- | --- |
| Direct UTF-8 text (EIS) | KDE Wayland, KWin 6.8 or newer with EIS loaded, and a keyboard portal with EIS |
| GNOME CommitText | GNOME Wayland with a Shell extension that exposes CommitText |
| libei + clipboard | A keyboard portal with EIS and a Wayland clipboard transport |
| libei | A keyboard portal with EIS |
| NotifyKeyboardKeycode | A keyboard portal |
| Paste from clipboard | A keyboard portal and a Wayland clipboard transport |
| XTest (xdotool) | An X11 session with XTEST |
| ydotool | An X11 or Wayland session |

The list can offer xdotool and ydotool before they are installed. Selecting
one shows its requirements and executable path. The permissions reset appears
with portal methods. Paste shortcut controls accompany the clipboard methods,
and the shortcuts also apply to the Paste action.

Clipboard paste uses Wayland data-control (`ext_data_control_manager_v1` or
`zwlr_data_control_manager_v1`) or, on GNOME, the Clipboard portal:

- the previous selection is snapshotted (all MIME formats, ≤ 64 MiB, ≤ 128
  formats, 2 s) or the paste is refused; selections marked
  `x-kde-passwordManagerHint` are left alone;
- temporary text is marked sensitive and restored ≥ 200 ms after the shortcut;
  a newer copy wins; Tecloid then owns the restored data;
- the paste shortcut must match the destination (`pasteShortcut`,
  `terminalPasteShortcut` by window class; embedded terminals cannot be
  detected). `auto` sends Ctrl+V, and Ctrl+Shift+V to terminals.

Editing `sys:` shortcuts (`copy`, `paste`, `undo`, `find`, …) pick their keys
when they run: the letter from the active XKB layout (the layout must be
known, which KDE reports; otherwise the QWERTY key), the terminal variant by
window class, and for paste the paste settings (copy and cut do not depend
on them). The Paste key
(`pasteShortcut: Paste`) is not handled by every application. On Windows the
letter comes from the foreground window's layout and the shortcuts are the
Windows ones (redo `Ctrl+Y`). User `Ctrl+KeyC` chords stay physical. Settings
→ reset text input permissions clears portal tokens.

## Windows

- `mapper::windows` installs `WH_KEYBOARD_LL` (and `WH_MOUSE_LL` when a mouse is
  selected) on its own message-loop thread; the callback only translates and
  queues. Output is `SendInput` with scan codes, so the target's layout applies.
  Injected input is never processed (own output and other tools). Windows does
  not auto-repeat injected keys, so the engine re-presses held keys on repeats.
- Mouse buttons and the wheel are observed, not captured: they commit pending
  holds but reach the window unchanged and possibly before the committed
  modifier. Mouse button output is not injected; such actions are shown as
  unavailable.
- No per-device interception without a driver: the mapper takes every keyboard
  of the session ("All keyboards"). Input to elevated windows and the secure
  desktop (UAC, Ctrl+Alt+Del, lock screen) bypasses hooks; on return, keys
  released there are released. UIPI blocks injection into elevated windows
  (reported as an action failure). A named mutex keeps one mapper per session.
- An input guard process (same binary, private role started before config/UI)
  releases journaled keys if the owner is killed.
- Native Unicode is sent with 30 ms between UTF-16 units (WinUI repeats
  `VK_PACKET` otherwise). Popup text uses Unicode `SendInput`, no clipboard.
- No public API for `switchToDesktopN`, `moveWindowToDesktopN`,
  `moveWindowToNext/PreviousDesktop`: skipped with a notice.
- Commands run in PowerShell (`-NoProfile -NonInteractive -ExecutionPolicy
  Bypass`, no console). File dialogs use the common item dialog.
- IPC: loopback TCP with a token ([architecture.md](architecture.md#ipc)).
- Distribution: portable ZIP, not Authenticode-signed; needs the MSVC v14
  redistributable.

Acceptance status: automated VM suites pass; physical hardware, multiple
keyboards, real sleep/wake and multi-monitor DPI are not accepted yet.

## macOS

Shell only: tray, global hotkeys, popups with native insertion via Enigo after
foreground checks (needs Accessibility permission). No interception and no
mapper macros; device controls are disabled. CI checks build, tests and clippy;
there is no desktop acceptance.

## Adding a backend

- **Linux desktop:** add a variant to `platform::linux::Desktop` and
  `classify_desktop()`; create `keyboard_layout/linux_<de>.rs` with `current()`
  and `start_watcher()` reporting through `super::publish(&info)`; wire it in
  `keyboard_layout/mod.rs`; add `mod <de>` with
  `resolve(name) -> Option<SystemAction>` in `mapper/system.rs`.
- **System action:** a `sys:` id needs a documented API (or fixed system
  shortcut) on at least two platforms. Add it to `PLAIN_SYSTEM_ACTIONS` (or
  `SHORTCUT_SYSTEM_ACTIONS` with `mapper/shortcuts.rs` for an application
  shortcut) in
  `config/actions.rs`, to `MACOS_UNSUPPORTED_SYSTEM_ACTIONS` if macOS lacks it,
  implement it in every capable backend of `mapper/system.rs`, add its picker
  label to `ui/i18n.slint` and every PO catalog. Desktop-specific actions belong
  in `kde:` and similar categories. Never remove or rename an existing id.
- **macOS interception:** replace the stub in `mapper/runtime.rs` with
  `mapper/macos.rs` mirroring `mapper::windows` (`list_keyboards()`, `spawn()`,
  `Handle`, an `EventSink` and a `LoopDriver` for `mapper::output`) using
  `CGEventTapCreate` + `CGEventPost`.
