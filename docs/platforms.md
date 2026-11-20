# Platforms

**KDE Plasma 6 on Wayland is the primary and only accepted environment.** Other
environments compile and partly work; implemented code is not acceptance.

| Concern | KDE | GNOME | Sway / Hyprland | X11 | Windows | macOS |
| --- | --- | --- | --- | --- | --- | --- |
| Key interception | ✅ evdev + uinput | ✅ | ✅ | ✅ | ✅ session-wide `WH_KEYBOARD_LL` + `SendInput` | ❌ stub |
| Literal text (`text:`) | ✅ portal libei, data-control clipboard | ⚠ portal + Clipboard portal, needs extension | ⚠ backend dependent | ⚠ keyboard backend only, no clipboard paste | ✅ Unicode `SendInput` | ❌ mapper; popups only |
| Keyboard layout | ✅ D-Bus `org.kde.keyboard` | ⚠ extension | 🚧 | 🚧 (`linux_x11.rs`) | ✅ foreground window input language | 🚧 |
| Standard shortcuts (`sys:copy`, `sys:undo`, `sys:moveWordLeft`, …) | ✅ layout via XKB, terminals by window class | ✅ | ✅ | ✅ | ✅ layout via `VkKeyScanEx` | ❌ mapper |
| System actions (`sys:`) | ✅ kglobalaccel, `org.kde.keyboard`, ScreenSaver | ⚠ extension; Super+Arrow / Super+N for side tiling and task entries 1–9; no `showDesktop`, top/bottom tiling, clipboard history | 🚧 | 🚧 | ✅ except virtual-desktop actions | ❌ |
| `kde:` shortcuts | ✅ | — | — | — | — | — |
| `xdg:` app actions | ✅ | ✅ | ✅ | ✅ | — | — |
| Layer-shell popups (Spell) | ✅ | ❌ winit | ✅ not accepted | ❌ winit | — | — |
| Active window / fullscreen | ✅ KWin script → D-Bus | ⚠ extension | ✅ IPC; other wlroots via `wlr-foreign-toplevel` | ⚠ `xprop` (+ `xdotool`, `xrandr`) | ✅ Win32, `SetWinEventHook` | ⚠ `NSWorkspace`, `CGWindowList` (untested) |
| Game mode: GameMode daemon | ✅ D-Bus | ✅ | ✅ | ✅ | — | — |
| Game mode: known games | ✅ Steam, Lutris, Heroic, umu, gamescope, `.desktop` `Game` | ✅ | ✅ | ✅ | ✅ Steam `RunningAppID`, Game Bar list | ⚠ `Info.plist` category |

🚧 = skeleton returning `Ok(None)`/`None`; ❌ = explicit "not implemented"
error. Both compile and are safe to ship.

Dispatch on `platform::linux::detect()`, never on raw `XDG_CURRENT_DESKTOP`.
Platform code stays behind `#[cfg]` and target-specific dependencies.

## Linux

Runtime requirements: keyboard/mouse/touchpad event-node read and `/dev/uinput` write (group
`uaccess` in an active local session; `input` is a fallback, see [development.md](development.md#device-access-linux));
`xdg-desktop-portal` with the DE backend for `text:`; `hyprctl` / `swaymsg` on
Hyprland / Sway; `xprop` (+ `xdotool` for title conditions) on X11. KDE Wayland
needs nothing extra (a persistent KWin script reports focus over D-Bus; calls not
from KWin are ignored).

Distribution: tar.gz into `~/.local`, deb and rpm (nfpm), built on Ubuntu 22.04
(glibc 2.35). Flathub/Snap are not possible: a sandbox cannot install the udev
rule or open `/dev/uinput`.

### X11

**Not a supported target.** Interception does not depend on the display server,
so the mapper works, and existing X11 code (xprop/xdotool, `linux_x11.rs`, winit
popups) stays as best effort without e2e. Do not extend it: GNOME dropped the X11
session and KDE drops it in Plasma 6.8. Reconsider only as "support desktop X",
on concrete demand.

### GNOME (experimental)

Requires the bundled Shell extension `tecloid@bozonx.dev`
(`crates/tecloid-core/assets/gnome-extension/`, targets GNOME Shell 50),
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

Clipboard: the extension does not own the clipboard (GJS cannot implement a
multi-MIME selection source, and binary shims are disallowed by extension review
rules). Text uses RemoteDesktop/libei plus the Clipboard portal requested before
`Start`. Keyboard-only and clipboard sessions keep separate restore tokens; a
denied clipboard grant is respected and libei typing stays available. Restored
clipboard data keeps the RemoteDesktop session (and GNOME's indicator) alive
until another owner replaces it.

## Text input

Text jobs capture the destination window; Linux waits up to 500 ms for focus to
return from a popup. KDE checks KWin window ids, GNOME extension ids; the
fallback compares app/process (cannot distinguish two windows of one app). With
a supported detector but no target, input is rejected.

`linux.textMode`: `libeiWithClipboardFallback` types via libei and pastes the
whole text through the clipboard if the layout lacks a character; `libeiOnly`
rejects such text; `keycode`, `clipboard`, `ydotool`, `xdotool` are explicit.

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
