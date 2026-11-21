# Manual acceptance


## What requires an operator

The primary acceptance target is **KDE Plasma 6 on Wayland**. GNOME, Windows
and macOS sections below are supplemental platform checks, not release gates
substituting for KDE. A VM operator run does not prove physical hardware behavior.

Use [testing.md](testing.md) for automated commands and VM preparation. Do not
rerun the entire checklist by hand just to reproduce a green automated suite.

| Coverage already automated | Manual work that adds evidence |
| --- | --- |
| L0/L1: parsing, persistence, mapping sequences, tap/hold timing | Natural typing, quick rolls, modifier overlap and perceived repeat/latency on real devices; no exhaustive timing boundary enumeration |
| L0+: page callbacks, geometry, navigation, themes, translation checks | Visual readability with actual fonts/DPI, real keyboard navigation and screen reader; no manual audit of every serialized field |
| L3/scripts: popup recovery, clipboard in nested KWin | Real application caret/focus, rich clipboard formats, permission dialog interaction and compositor positioning |
| L4: installation, permissions, autostart, upgrade, removal, suspend, hotplug | Physical devices/docks/Bluetooth; real session integration. Repeat lifecycle checks manually when packaging/session code changed or automation is unavailable |

A check may deliberately overlap automation: record the additional boundary
(real hardware, application, compositor or user interaction) it verifies.
Missing automatic coverage remains NOT RUN; a manual smoke does not silently
replace the required automated suite.

## Select the scope before starting

- Every release: run the physical KDE release smoke in
  [testing.md](testing.md#every-release-release-smoke), plus relevant physical
  hardware checks below. Record missing equipment explicitly.
- Changes to UI, insertion, focus or permissions: run the corresponding sections
  below in the real KDE session, including the additional interaction checks.
- Changes to packaging/autostart or lack of automated lifecycle evidence: run
  lifecycle checks against the actual release package.
- Full acceptance: follow the triggers in
  [testing.md](testing.md#full-acceptance-triggers). The detailed checklist below
  is the operator reference, not an unconditional every-release requirement.

## Additional real-session interaction checks

- [ ] With a nonsensitive test field active, open a popup, then switch to another
  application or another window of the same application before selecting. Record
  whether selection cancels or returns to the original destination; text must
  never silently land in the newly focused unrelated field. Close the original
  window and repeat: no insertion into another window or stranded popup.
- [ ] With mapper active, lock and unlock the physical KDE session using test
  credentials. Verify normal input after unlock, F6 and popup focus return; no
  stuck modifiers. Do not collect screenshots/logs containing passwords.
- [ ] With the session's screen reader enabled, navigate settings and a dialog
  using only the keyboard. Names, roles, values and focus changes are announced;
  icon-only controls can be identified and focus can leave every control.
  Record reader/version; headless accessibility-tree tests alone are insufficient.
- [ ] Change system theme and display scaling while settings are open, then open
  Emoji and Quick. Text/focus remains readable and controls stay reachable on
  each display. Check actual font and emoji rendering, not only widget geometry.

Use a fresh copy of this checklist for **each OS/desktop/package combination**.
For full acceptance, run the common checks and every applicable section below.
For release smoke and targeted acceptance, use the selection rules below. Mark an item only
when its expected result was observed: `PASS`, `FAIL`, `NOT RUN` (with a reason),
or `N/A` (not implemented on this platform). Automated success is not a manual
check. Keep completed copies, logs and screenshots outside Git.

| Environment | Sections to run | Acceptance scope |
| --- | --- | --- |
| Fedora KDE + RPM; Debian KDE + DEB | Common, mapper, Linux, KDE, lifecycle | Primary: Plasma 6 Wayland; test both distributions |
| Ubuntu GNOME + application tar.gz | Common, mapper, Linux, GNOME, lifecycle | Experimental; bundled Shell extension required |
| Windows 11 + portable ZIP | Common, mapper, Windows, lifecycle | Implemented Windows features; desktop actions have explicit gaps |
| macOS | Common shell checks, macOS, supported lifecycle checks | Shell only; mapper/layers/macros are N/A |

See [platforms.md](platforms.md) for capabilities. Sway/Hyprland and X11 are not
additional acceptance targets; an exploratory run there does not replace KDE.

### Prepare one run

- [ ] Record OS, desktop version, Wayland/session type, package filename,
  version/commit and SHA256. Confirm this is the intended build.
- [ ] Use a disposable VM account or a separate test profile on physical
  hardware. Back up any existing data before using a real-machine test profile;
  do not replace a personal keymap with this fixture.
- [ ] Install/extract the package as a normal user using the platform's documented
  procedure. Installation completes without errors; the executable reports the
  intended version.
- [ ] Open two ordinary text editors/fields, a terminal and a browser field.
  Use nonsensitive test clipboard data. For display/workspace checks, prepare
  two monitors and two **fixed** workspaces; mark unavailable hardware NOT RUN.
- [ ] On platforms with a mapper, create a keymap named `Manual acceptance` in
  the UI, select the intended keyboard and mouse, and save it. Keep it separate
  from personal keymaps.

Suggested temporary assignments (create them in the editor, not by modifying
`config.json`): F6 → `KeyB`, F7 → `sys:selectAll`, F8 → `text:❓`,
F9 → `text:Привет 🙂`. Also create a layer mapping J → K, a macro typing
`one`, pausing, then typing `two`, and a command launching a harmless editor.
Choose a spare key for the layer/tap-hold tests. macOS skips these assignments.

### Common: launch, settings and popups

- [ ] Launch from the application menu/overview (or the extracted Windows/macOS
  application). Tecloid opens; its icon is visible in the tray/menu bar.
- [ ] Close settings with the window close button. The application stays alive;
  reopen settings from the tray/menu bar.
- [ ] Launch a second instance. It opens/controls the existing application;
  no second tray icon or second mapper appears.
- [ ] Open each settings page. Controls and dialogs are usable; no blank pages,
  clipped important buttons or unexpected errors appear.
- [ ] Change a setting, close/reopen settings, then quit/relaunch the application.
  The saved value remains. On mapper platforms, verify a saved assignment too.
- [ ] Check system, light, dark, E-ink and high-contrast themes; reduce-motion
  mode and increased text scaling. Text, selection, focus and disabled controls
  remain readable. The minimum window size remains usable.
- [ ] Switch each shipped language, including Russian, and reopen a dialog and
  popup. Labels fit, translated text updates, and emoji glyphs render.
- [ ] Navigate a page and a dialog using Tab, Shift+Tab, arrows, Enter and Escape.
  Focus is visible; the intended control activates and Escape cancels safely.
- [ ] In a text editor, put the caret **in the middle** of `before|after`.
  Invoke Emoji through its configured hotkey; choose an emoji with the keyboard.
  It is inserted at the caret in the original field, and typing continues there.
- [ ] Repeat Emoji selection with the mouse. Then reopen and cancel with Escape:
  nothing is inserted, focus returns, and no popup remains on screen.
- [ ] Open Quick actions, search/navigate, execute a harmless action and cancel
  a second invocation. Selection is correct and focus returns to the target app.
- [ ] Repeat popup open/choose/cancel ten times, including switching between two
  windows of one application. No duplicate insertion, stale popup or insertion
  into the wrong window occurs.

### Mapper: Linux KDE/GNOME and Windows

- [ ] Enable the mapper. Unassigned letters, numbers, punctuation, navigation
  keys and shortcuts still behave normally in the editor.
- [ ] Press F6 once and then hold it. It produces B with ordinary repeat
  behavior; there are no extra F6 events or duplicate characters.
- [ ] Select part of a line, then press F7. The built-in Select All action selects
  the whole field. Test built-in copy/cut/paste/undo in English and Russian layouts.
- [ ] Test left/right Ctrl, Shift, Alt and Super/Win separately and in overlapping
  presses. Releasing them restores ordinary typing; modifiers never stay down.
- [ ] Test tap/hold: a quick tap performs its tap action; holding performs the
  configured modifier/layer action. Roll into another key quickly and repeat
  ten times; the result remains predictable and no key is lost or stuck.
- [ ] Hold the layer key and press J: K is produced. Release the layer key and
  press J: J is produced. The layer hint agrees with the active layer.
- [ ] If configured, test double tap and long hold separately. Each action fires
  once, without also firing an unrelated tap action.
- [ ] Run the `one` / pause / `two` macro. The exact sequence is delivered with
  the pause; subsequent ordinary typing follows it without interleaving.
- [ ] Enable command permission for this test keymap and run the editor command:
  one editor launches. Disable permission and repeat: the command is blocked.
- [ ] Press F8 and F9 in an editor, browser field and terminal. Exact `❓` and
  `Привет 🙂` appear once in the intended field. Repeat in English and Russian
  layouts; check the terminal's configured paste shortcut separately.
- [ ] Press F8 five times rapidly: exactly `❓❓❓❓❓` appears. Repeat after
  ordinary copy/paste; no later insertion fails due to a stale clipboard offer.
- [ ] Copy ordinary text, insert F8/F9, then paste the original clipboard normally:
  it is unchanged. Repeat with formatted text or an image where the target apps
  support it. A new copy made during insertion remains the newest clipboard.
- [ ] With a mouse selected, test left/right/middle clicks, wheel movement,
  dragging/selecting text, and an assigned extra button. No original extra-button
  action leaks alongside its replacement; ordinary pointer movement remains usable.
- [ ] Switch between two application windows. Active-window rules apply to the
  correct window and stop applying after leaving it.
- [ ] Enter game mode using the UI or configured action. Verify its configured
  blocking behavior, then leave game mode: normal mappings and popups recover.
- [ ] Disable the mapper while keys/buttons are held, release them, then type.
  Ordinary input works without stuck modifiers/buttons. Re-enable and verify F6.

### Linux: installation and text permission

- [ ] Start in a normal graphical session without manually adding the user to
  `input` or changing device permissions with chmod. Mapper starts with the
  installed udev/uaccess rules; failures are reported clearly.
- [ ] On the first F8/F9 insertion, the portal permission dialog opens. Keep
  **both keyboard and mouse selected in mapper**: move the pointer, click toggles,
  navigate with the keyboard and cancel the request. Physical input remains usable.
- [ ] Explicitly retry/reset text permissions through settings. Request insertion
  again, grant the required keyboard/remote-interaction and clipboard permissions,
  then verify exact insertion into the original field.
- [ ] After consent, confirm F6 remapping, mouse dragging/clicks and F8 insertion
  all work. Consent clicks/keystrokes are not replayed into the target application.
- [ ] Repeat insertion at least five times. Consent is not requested for every
  character; saved consent is reused after relaunch when the portal supports it.
- [ ] Reset text permissions once more and verify a new consent request. Refusal
  does not create a repeated dialog loop or leave input devices captured unusably.

### KDE Plasma 6 Wayland

- [ ] Emoji/Quick popups open and remain usable in normal and maximized windows;
  check Spell positioning, keyboard navigation, Escape and focus return.
- [ ] Test window close, minimize, maximize/restore, keep-above, and moving a
  window between monitors. Each action affects the focused test window only.
- [ ] On two monitors, repeat next-screen and previous-screen actions through
  both ends. The window cycles between displays and remains usable.
- [ ] On two fixed workspaces, repeat `sys:moveWindowToNextDesktop` three times:
  `1 → 2 → 1 → 2`. Repeat the previous-desktop action. The view follows the same
  window and focus remains on it. Separately test switching desktops without moving.
- [ ] Switch keyboard layouts using Tecloid, then type in English and Russian.
  The reported active layout and produced characters agree.
- [ ] Test an existing `kde:` shortcut and an `xdg:` application action. The
  intended desktop/application operation occurs once.

### GNOME Wayland (experimental)

- [ ] After the prepared session relogin, the Tecloid Shell extension is enabled
  and Tecloid appears in the application overview. Launch it from there.
- [ ] With the AppIndicator extension enabled, the tray icon opens settings.
  Disable the tray host temporarily: settings remain reachable. Re-enable it:
  the icon returns without starting another Tecloid process.
- [ ] Perform **all Linux permission checks with the selected mouse**, including
  cancel, reset and grant. The Remote Desktop dialog accepts real clicks without
  removing the mouse from mapper.
- [ ] Test F8 separately, five times in succession, then as a rapid five-character
  burst. Exact Unicode is inserted each time; original clipboard contents remain
  available. GNOME's session indicator may remain while restored clipboard data
  is being served; record actual insertion/clipboard results, not just the indicator.
- [ ] Test Emoji and Quick actions as winit popups: keyboard and mouse selection,
  cancellation, positioning and return of focus to the original application.
- [ ] Use fixed workspaces (the manual stand prepares four). Repeat the window
  move actions past the last/first workspace and verify a cycle with focus:
  `1 → 2 → 1`, and the reverse. Check desktop-switch and direct numbered actions
  separately; do not assume those have the same wrap behavior.
- [ ] On two monitors, test next/previous screen independently of workspace moves.
  A screen is a physical display; a desktop is a workspace.
- [ ] Test GNOME keyboard-layout switching and close/minimize/maximize actions.
  `showDesktop` is unavailable: mark it N/A rather than a successful check.
- [ ] Disable the Tecloid Shell extension, try a bridge-dependent action, and
  verify a clear failure rather than an unrelated action. Re-enable the extension
  (relogin if required) and verify recovery.

Dynamic GNOME workspaces add/remove entries while windows move. Test stable
cyclic movement with a fixed workspace count; record exploratory dynamic-mode
results separately. The acceptance fixture must not silently change host settings.

### Windows 11

- [ ] Extract the portable ZIP and launch as a normal user. Dependencies are
  present; there is no unexpected console window or missing-runtime error.
- [ ] Run mapper checks in Notepad, a browser and another ordinary desktop app.
  Unicode/emoji is inserted exactly once; held-key repeat works.
- [ ] If several keyboards are connected, verify the documented session-wide
  behavior (All keyboards). Do not count per-device isolation as supported.
- [ ] Open a UAC prompt and an elevated editor. Secure-desktop input remains
  usable; unsupported injection into the elevated editor reports a failure.
  After returning, Ctrl/Alt/Shift/Win and ordinary mappings are not stuck.
- [ ] Lock/unlock Windows and repeat remap/text/popup checks. Test Ctrl+Alt+Del
  as a secure-desktop transition, not as a remappable shortcut.
- [ ] Verify window, monitor, volume and layout actions supported on Windows.
  Virtual-desktop actions are unsupported: mark them N/A and check the notice.
- [ ] With the mapper active, terminate Tecloid forcibly through Task Manager.
  Release physical keys and verify normal typing; the input guard releases
  injected held keys. Relaunch and repeat F6.

### macOS (shell only)

- [ ] Launch the application, open/close settings from the menu-bar icon and
  verify saved preferences after relaunch.
- [ ] Without Accessibility permission, request popup insertion and verify a
  clear failure. Grant permission through macOS settings and retry successfully.
- [ ] Invoke Emoji and Quick popups from an editor, choose/cancel using keyboard
  and mouse, and verify exact insertion and focus return. Repeat with two windows
  of the same application to check destination protection.
- [ ] Check themes, scaling, language and keyboard-only settings navigation.
- [ ] Mapper/device controls are disabled or clearly unsupported. Mark keyboard
  interception, layers, mapper macros and unsupported system actions N/A; do not
  report them as working merely because the shell opens.

### Lifecycle: repeat on each applicable platform

- [ ] Quit through Tecloid, then relaunch. Tray/menu-bar state is clean; settings
  and test keymap remain, and no duplicate mapper/process survives.
- [ ] Enable autostart and **reboot/sign out and in**. The application starts in
  the background; verify the tray and a remap before opening settings (shell/tray
  only on macOS where supported).
- [ ] Disable autostart, reboot/relogin, and observe for at least 90 seconds
  (120 seconds on Windows). Tecloid does not launch. Start it manually afterward.
- [ ] Re-enable autostart and repeat login. Startup and mappings recover. Mark
  unsupported autostart mechanisms N/A with the platform reason.
- [ ] With the previous release installed, create the test settings/keymap and
  upgrade to the candidate. They load with the same meaning; startup and mapper
  still work. Reinstalling the same version is not an upgrade check.
- [ ] Remove the package/application using its documented procedure. Executable,
  desktop/menu entries and startup registration are removed as applicable;
  user configuration/keymaps remain. Record manual registration steps for a
  portable installation rather than assuming a package manager performed them.

### Physical hardware: VM results are insufficient

- [ ] Type naturally and rapidly with overlapping presses. Check both sides of
  modifiers, numpad, media keys, Caps/Num Lock LEDs and extra mouse buttons.
- [ ] Test two keyboards and a mouse, a USB hub/dock and Bluetooth where available.
  Unplug/replug and change ports: correct devices recover and unrelated devices
  are not captured accidentally. Verify an unselected mouse remains ordinary.
- [ ] Sleep/wake with the mapper active, then repeat remap, text and popup checks.
  Repeat with a dock/external monitor; no stuck keys or lost devices remain.
- [ ] Move settings and popups across monitors with different scaling/DPI; connect
  and disconnect an external monitor. Controls remain accessible and focus returns.
- [ ] In a real fullscreen/borderless game, check game mode, Alt+Tab and shortcut
  conflicts; after exiting, ordinary desktop input and popups recover.

### Record the result

Save a completed checklist per environment, including failed and unrun items:

```text
OS / version / desktop / session:
Package / version / commit / SHA256:
VM or physical machine; keyboards / mouse / displays / scaling:
Applicable sections:
PASS:
FAIL: steps → expected → actual; log/screenshot:
NOT RUN: reason:
N/A: platform capability reason:
```

A release report must retain the gaps: e.g. one-monitor VM is not multi-monitor
acceptance, successful shell launch is not macOS mapper acceptance, and an
experimental GNOME run is not KDE acceptance.

