# Testing

A green test or CI run proves only what it covers. Skipped steps are not passes;
results of a run apply to the sources it recorded, not automatically to HEAD.

## Levels

| Level | What | Where | When |
| --- | --- | --- | --- |
| L0 | Unit / component / integration tests: core, config → engine, IPC, i18n, Slint pages on a test platform | CI (`ci.yml`), Linux/Windows/macOS | Every push/PR |
| L0+ | UI scenarios (`examples/`) under headless Weston: real pointer/key events, geometry, navigation, themes. Does not cover KDE, tray, Spell, portal | CI | Every push/PR |
| L1 | Mapper e2e: evdev → engine → uinput without a desktop | Host; CI (missing `uinput` fails the job) | Every PR |
| L2 | Package smoke: install/remove in containers | CI (`release.yml`), Docker | Release tag |
| L3 | Desktop scenarios in a real KDE Wayland session + L1 | Developer machine | Before a release, after UI changes |
| L4 | Disposable KVM/QEMU VMs: package install, permissions, relogin, autostart, tray, mapper, portal, upgrade, removal | Local VMs | Before a release, after packaging/permissions/autostart/tray/portal changes |
| L5 | Manual acceptance on physical hardware | Real machines | Before a release |

Desktop e2e (L3–L5) never run on GitHub: cloud runners have no Plasma session,
tray or portal, and a self-hosted runner would let fork PRs execute code locally.

## Everyday checks

```sh
cargo fmt -p tecloid-core -p slint-shell --check
cargo clippy --locked -p tecloid-core -p slint-shell \
  --features slint-shell/spell --all-targets --no-deps -- -D warnings
cargo test --locked --workspace --features slint-shell/spell
cargo check --locked -p slint-shell --all-targets          # build without Spell
cargo deny check                                           # advisories, licences, sources (deny.toml)
python3 -m unittest discover -s apps/slint-shell/scripts/vm -p 'test_*.py'
```

While iterating, run only what you touched — the full workspace links every
example and is heavy:

```sh
cargo test --locked -p tecloid-core <name>
cargo test --locked -p slint-shell --lib <name>
cargo test --locked -p <crate> --lib --tests               # skips examples
```

Do not raise `CARGO_BUILD_JOBS` / `RUST_TEST_THREADS`; they may be limited on
purpose. On Windows/macOS run natively:
`cargo test --locked -p tecloid-core -p slint-shell --all-targets` and
`cargo check --release --locked -p slint-shell`; on Windows also
`scripts/windows/check-windows-syntax.ps1`.

The build itself checks translations ([i18n.md](i18n.md)).

## Choosing a test type

Pick the smallest boundary that reproduces the bug:

| Type | Boundary | Examples |
| --- | --- | --- |
| Unit | One algorithm; inputs and time controlled | action parsers, validation, engine with synthetic events in `crates/tecloid-core/src/` |
| Integration | Contract between components with real serialization/files/IPC/Slint bindings, temp resources | `tecloid-core/tests/{storage_override,config_to_engine,game_mode_lifecycle}.rs`; `slint-shell/tests/{frontend,page_callbacks,ipc}.rs`; document tests in `src/pages/*/tests.rs` |
| Component | Real Slint widget, stubbed save | `slint-shell/tests/inline_editors.rs` |
| System / e2e | End-to-end through real backends; state start and end of the path | `tecloid-core/tests/mapper_e2e.rs`, VM runs |

Rules:

- Callback → page → `Document` chains belong in headless tests (`tests/`). A
  graphical example in `examples/` is justified only when the check needs a real
  window: pointer/key events, geometry, focus, theme.
- Integration tests use temp dirs and endpoints, never the user config, and do
  not need a desktop or `/dev/uinput`.
- Public API tests go in `<crate>/tests/`; private contracts may stay next to code.
- Keep e2e to a few critical paths; do not repeat parser/validation variants there.
- Format changes need a fixture test that loads a real file of every supported
  older `version` ([config-format.md](config-format.md#compatibility-contract)).

## UI scenarios

Need a graphical session; each uses a temporary config and exits on its own:
`cargo run --locked -p slint-shell --example <name>`. Do not start them while
another build or test run is in progress.

| Example | Checks |
| --- | --- |
| `editor -- --smoke` | Keyboard page renders in a real window (**required for UI changes**) |
| `interactions` | Settings, save, navigation, theme, language, hidden popups (**required for UI changes**); `TECLOID_EDITOR_SNAPSHOTS=<dir>` saves screenshots |
| `library`, `rules`, `macros`, `menus`, `layer-keys` | Page editors (`TECLOID_MENUS_NAV=1` for menu navigation) |
| `popup-grid` | Shared Emoji/Quick grid: focus, keys, clicks |
| `undo-redo`, `inline-edit`, `drag-scroll`, `notifications` | Shared controls |
| `accessibility`, `keyboard-navigation`, `contrast-scaling` | Accessibility tree, keyboard-only use, high contrast and text scaling |
| `small-window` | All pages and dialogs at minimum size; `TECLOID_WINDOW_SIZE=WxH`, `TECLOID_SMALL_LANGUAGE=<lang>` (`qps-ploc` = pseudo-locale, debug only), `TECLOID_SMALL_TEXT_SCALE=1.3`, `TECLOID_SMALL_RELEASE=1` |

Helpers live in `examples/support/` (temp document, snapshots, accessibility
lookup, wait-with-timeout). The same scenarios work under Live Preview.

Diagnostic examples (not tests): `bench-evdev`, `bench-return`, `window-fixture`,
`panel-fixture`, `focus-window`, `inspect-geometry` (features `spell` / `probes`).

## Shell check scripts

| Command | Checks |
| --- | --- |
| `python3 apps/slint-shell/scripts/check/check-recovery.py --popups spell` | Recovery of a fixed config after restart and popup delivery after a Spell restart (`--popups winit`, `--live-preview`) |
| `python3 apps/slint-shell/scripts/check/check-preferences.py` | Theme/language over IPC with open popups, clean `quit` |
| `python3 apps/slint-shell/scripts/check/check-worker-crash.py target/debug/tecloid` | Restart of a crashed Spell worker |
| `python3 apps/slint-shell/scripts/release/check-linux-install.py` | Archive installer in a temp prefix (needs `desktop-file-validate`) |
| `python3 apps/slint-shell/scripts/check/test-clipboard.py` | Clipboard paste in a private nested KWin (needs `kwin_wayland`, `dbus-run-session`) |

Game mode: `cargo test --locked -p tecloid-core gamemode`,
`--test game_mode_lifecycle`, `cargo test --locked -p slint-shell --lib game_mode`
(KWin checks run their own `dbus-daemon`).

Popup latency benches: `scripts/bench/bench-open.sh`, `bench-virtual.sh <dir>
geometry|input|lifecycle` (private KWin with two outputs), `summarize.py`
over the `TECLOID_METRICS` CSV. Metrics `t0`…`t5` are trigger, UI callback,
show, first frame commit (not presentation), focus, first handled key.

## L1: mapper e2e

```sh
cargo test --locked -p tecloid-core --test mapper_e2e -- --ignored
```

Creates a virtual "physical" keyboard via uinput, runs the mapper on it with a
test keymap and reads its output device: passthrough, remap, chord, tap/hold,
layer, double tap, macro with pause, stop without stuck keys, device removal.
Both devices are grabbed, so nothing reaches the session; the mapper lock moves
into a temp `XDG_RUNTIME_DIR`. Needs `/dev/uinput` write and `/dev/input` read.

## L3: local desktop run

```sh
apps/slint-shell/scripts/e2e-local.sh            # --no-mapper skips L1
```

Builds with Spell, runs every UI scenario, the check scripts and L1, prints a
summary and writes `target/test-results/linux/<date>.txt`. Do not touch the
keyboard or mouse meanwhile. Each scenario is limited to 180 s (needs GNU
`timeout`).

## L4: virtual machines

Stand: `apps/slint-shell/scripts/vm/`. Each run creates a disposable overlay on a
frozen base image (`--keep` keeps it, `--show` opens a GTK window), stores logs
and QMP screenshots in `<vm>/runs/<time>/logs/` and writes a summary to
`target/test-results/<linux|windows>/<date>-vm-<name>.txt`. Keys are injected
through QMP as a real USB keyboard. None of this goes into Git; see
[release.md](release.md#acceptance-results).

Host: `qemu-system-x86_64`, `qemu-img`, `/dev/kvm`, ssh; for Windows also OVMF
with Secure Boot and `swtpm`. Guest access key:
`ssh-keygen -t ed25519 -N '' -f ~/.ssh/tecloid-vm`. Images live outside the
repository, one directory per VM with a `vm.json`:

```json
{
  "name": "fedora44-kde", "base": "base.qcow2",
  "user": "tester", "ssh_key": "~/.ssh/tecloid-vm", "ssh_port": 2231,
  "vga": "VGA", "memory": 6144, "cpus": 4,
  "install": "sudo dnf install -y {package}",
  "reinstall": "sudo dnf reinstall -y {package}",
  "remove": "sudo dnf remove -y tecloid",
  "portal_keys": ["ret"]
}
```

Other keys: `nic`, `cpu`, `disk_bus`, `machine` (default `q35`; Debian 13 needs
`pc-i440fx-11.1` for repeated S3), `portal_deny_click` / `portal_deny_relative`
(Deny button of the RemoteDesktop dialog as screen or window fractions),
`suspend_skip_reason`, and for Windows `uefi` and `cache_disk`. `.tar.gz` packages
need no install commands (`install-linux.sh` is used).

### Linux (KDE)

Prepare a base image once: install a distribution with Plasma 6 Wayland (cloud
images + cloud-init work well for Fedora and Debian), then as root inside the
guest run `prepare-linux-guest.sh tester "<public key>"` (ssh, python3, kdialog,
passwordless sudo, SDDM/Plasma Login autologin, no screen lock or sleep, user not
in `input`). Reboot, verify autologin, shut down and `chmod a-w base.qcow2`.
Matrix: Fedora KDE (rpm), Debian 13 KDE (deb), Kubuntu (deb), openSUSE, Arch
(tar.gz). Start with Fedora and Debian.

```sh
apps/slint-shell/scripts/vm/run-linux.py --vm <vm-dir> --package target/release-artifacts/<file>
```

`--previous <old package>` also checks an upgrade: version changes, `config.json`
stays byte-identical and loads. Steps cover installed files and udev rule, start
without `input` (tray and mapper through session ACLs), reboot and autostart, portal
deny/consent, remap/chord/`text:`/Unicode, layer tap-hold, macro, command, system
actions, Russian layout, popups and focus return, Ctrl+Alt+F11, game mode,
screen-lock password typed through the mapper, S3 resume, second instance,
keyboard replug, stuck keys on stop and `kill -9`, upgrade over a running app,
quit and removal (user config kept). Full list: `linux-guest.py`.

Autostart checks include a real reboot, disabling through the desktop entry
(`Hidden=true`), a 90-second observation with no shell/mapper, a manual launch
followed by package replacement and another disabled login, re-enabling, and startup after package
replacement. These exercise OS registration and synchronization, not the settings
checkbox. `run-linux.py --autostart-only` runs this focused subset (without
`--acceptance`). The same cycle runs in the GNOME stand, including reinstall; it also accepts
`--autostart-only`.
Reinstalling the same package is not evidence of a previous-version upgrade;
use `--previous` for that coverage.

If portal steps fail, check `portal-deny-*.png`/`portal-consent.png` and adjust
`portal_*` keys for the guest's xdg-desktop-portal-kde version.

### Manual KDE and GNOME acceptance

Use the [manual acceptance checklist](#manual-acceptance-checklist) for the
actual checks and expected results; the commands below prepare the VM.

On the prepared local machine, from the repository root:

```sh
python3 apps/slint-shell/scripts/vm/manual-linux.py fedora
# After completing Fedora:
python3 apps/slint-shell/scripts/vm/manual-linux.py debian
# Experimental GNOME Wayland:
python3 apps/slint-shell/scripts/vm/manual-linux.py gnome
```

The launcher finds the current version and Cargo target directory itself, then
selects the RPM for Fedora, DEB for Debian or application tar.gz for GNOME.
`--check` validates the paths without
booting. It never silently selects a historical package. To rebuild packages after
source changes, build in Ubuntu 22.04 as described in [release.md](release.md#building-locally)
and put the RPM/DEB in the host Cargo `release-artifacts/` directory. Native
host builds on a newer distribution can require a GLIBC version absent in the
guests. A package's version alone does not prove it contains current sources.

Local VM location is saved in ignored `dev-files/vm/manual.json`:

```json
{"vm_root": "/mnt/disk2/vm"}
```

You can override it with `TECLOID_VM_ROOT` or `--vm-root`. This directory contains
`tecloid-fedora-kde/`, `tecloid-debian13-kde/` and `tecloid-ubuntu2604-gnome/`,
each with `vm.json` and
`base.qcow2`. Their SSH key is `~/.ssh/tecloid-vm`; package removal uses `tecloid`.
The prepared bases are standalone frozen copies; they do not depend on the
historical directories. Historical runs and packages remain archived and are
not selected by the launcher.

For another prepared stand use `run-linux.py` directly: `--vm` is the directory
with `vm.json` and `base.qcow2`; `--package` is the application RPM, DEB or bundle
archive, never a sources/vendor archive.

Requires an interactive terminal and a graphical host. Boots a disposable VM
with its QEMU window visible, uploads and installs the package, checks the Plasma
session and installed files, refreshes the KDE application menu cache, then hands control to you. Launch Tecloid from the
guest application menu and create a test keymap yourself. No automated keyboard
input, generated config, automatic app launch or removal runs in this mode.

The terminal guides you through launch/tray, mapper, layers/macros, Unicode and
popups, saved settings, autostart with reboots, recovery and removal. After each
check enter `pass`, `fail` or `skip`, optionally followed by notes. Leave the
terminal waiting while working or rebooting in QEMU. A skipped item is incomplete,
not a pass. Ctrl+C or terminal EOF interrupts acceptance. Diagnostics and the
normal summary are collected on exit, then the VM is stopped. The overlay disk
is always retained in the printed run directory; the frozen base is unchanged.
Manual results are identified as operator reports and do not replace automated
acceptance. `--manual` cannot be combined with automated suite/upgrade options.

For GNOME, the runner installs the application bundle and Shell extension,
configures input permissions and enables the Tecloid bridge and tray extension,
configures two fixed workspaces (as in the automated GNOME fixture),
then restarts the guest graphical session automatically. Wait for the terminal
checklist before interacting with the guest. No generated test keymap, injected
keys, portal consent clicks or automated mapper scenarios run in manual mode.
Launch Tecloid from the application overview and create a test keymap. The extra
GNOME checks cover desktop actions/focus and Unicode insertion with clipboard
restoration. Removal commands are shown in the checklist. GNOME remains
experimental; operator checks are not a claim of KDE feature parity.

For a custom GNOME stand:

```sh
python3 apps/slint-shell/scripts/vm/run-gnome.py --manual --vm <vm-dir> --package <application.tar.gz>
```

### GNOME

```sh
python3 apps/slint-shell/scripts/vm/run-gnome.py --vm <vm-dir> --package <tecloid-…-linux-x86_64.tar.gz> [--examples <dir>]
```

Ubuntu 26.04 cloud image with `ubuntu-desktop-minimal linux-generic zenity
python3-gi gir1.2-gtk-4.0 openssh-server desktop-file-utils wl-clipboard
fonts-noto-color-emoji gnome-shell-extension-appindicator`, then
`prepare-gnome-guest.sh tester` as root **inside the disposable guest only** (it
removes the local password). A smaller smoke than KDE: installation, permissions,
bridge, autostart, tray recovery, mapper basics, portal ASCII/Unicode with MIME
restoration, Quick popup focus, macro. Stops at the first failure.
`--examples` runs current `editor`/`interactions` binaries in the guest.

### Windows

One-time setup: Windows 11 x64 (Enterprise Evaluation ISO) in QEMU with q35,
OVMF Secure Boot, `swtpm`, AHCI disk, e1000e NIC, `usb-kbd`; an unprivileged
local user `user` with en-US and ru input languages; MSVC Build Tools and Rust via
`winget --source winget`. Create a persistent cache disk
(`qemu-img create -f qcow2 cache.qcow2 160G`), then as administrator in the guest:

```powershell
.\prepare-windows-guest.ps1 -User user -Password '<password>' -PublicKey '<tecloid-vm.pub>' -CacheDiskNumber 1
```

It enables OpenSSH, autologon, disables sleep, formats the given RAW disk as `D:`
and registers the interactive task `Tecloid-E2E`. Disable hibernation before
saving the base image.

```sh
apps/slint-shell/scripts/vm/run-windows.py --vm <vm-dir> [--suite smoke|full|release|mapper|autostart] [--extended]
```

The runner packs the working tree (tracked + untracked, not ignored), builds in
`D:\tecloid-e2e` with a warm `target\`, runs the task in the user session and
collects logs. `smoke`: native tests, debug build, `editor --smoke`,
`interactions`, IPC, CLI, popups via real hotkeys with exact Unicode checks in a
WinForms receiver, then mapper scenarios through QMP → USB keyboard → hook →
engine → `SendInput`. `full` adds release build, storage override, release
lifecycle (100 popup cycles, resources) and crash-with-held-keys. `--extended`
adds lock/unlock, S3, stress, process-tree kill, Notepad, elevated target. Host
part self-test: `python3 -m unittest discover -s apps/slint-shell/scripts/vm -p test_windows_stand.py`.

Windows `run-windows.py --suite full` and `--suite release` also reboot to check
the current user's Run registration: enabled launch with working IPC and no
visible main window, disabled login observed for 120 seconds, and re-enabled
launch. After enabled logins, QEMU keyboard input must be remapped from A to B
in the native WinForms receiver; UI Automation must also find the Tecloid
notification icon in the tray or overflow. The settings checkbox is not exercised. With
`--suite full --previous-package`, it also starts the previous ZIP executable,
replaces it in place with the current release, checks that Run registration is
unchanged and reboots into the new shell. Without that ZIP, version upgrade
coverage is explicitly NOT RUN. The debug smoke suite skips startup checks.
`--suite autostart` builds only the release shell and runs this focused cycle
without the full hardware/mapper acceptance suite.

## Manual acceptance checklist

Use a fresh copy of this checklist for **each OS/desktop/package combination**.
Run the common checks and every applicable section below. Mark an item only
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
`config.json`): F6 → `KeyB`, F7 → `builtin:selectAll`, F8 → `text:❓`,
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
- [ ] Use fixed workspaces (the manual stand prepares two). Repeat the window
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

## Before a release

Automated: L0–L2 green, L3 on current `main`, L4 at least Fedora KDE (rpm) and
Debian 13 KDE (deb), Windows VM `--suite full` after changes to portable code.

Manual checks are tiered. The full checklist is not repeated for every release;
automated L0–L4 covers installation, permissions, autostart, portal, upgrade and
S3 in VMs. Manual work covers what VMs cannot: real hardware, the real portal
dialog and subjective behaviour (stuck keys, tap/hold feel, layout).

### Full acceptance (triggers)

Complete the whole [acceptance checklist](#manual-acceptance-checklist) on
physical KDE Plasma 6 Wayland (and Windows 11 if its backend is shipped) when:

- this is the first public release, or the first release on a new platform;
- Slint, `spell-framework`, `i-slint-core` or another major dependency was bumped;
- target distributions moved to a new Plasma or xdg-desktop-portal release;
- the mapper, `text_input` or popup/focus-return code was substantially reworked.

### Every release: release smoke

About 15–20 minutes on a **physical** KDE Plasma 6 Wayland machine with the
`Manual acceptance` test keymap ([assignments](#prepare-one-run)):

- [ ] Launch from the menu; tray icon present; a second instance opens the
  existing one.
- [ ] F6 → B once and held (normal repeat); layer J → K and back; one tap/hold
  key including a quick roll; the `one`/pause/`two` macro.
- [ ] F8 and F9 insert exact text once; rapid F8 ×5 gives `❓❓❓❓❓`; the
  previously copied clipboard content is unchanged.
- [ ] Emoji and Quick: insertion at a mid-line caret, Escape cancels, focus
  returns to the original window.
- [ ] Disable the mapper while holding modifiers, release, type: nothing stuck.
- [ ] Reboot with autostart enabled: tray and F6 work before opening settings.
- [ ] Upgrade from the previous release: settings and keymap load, mapper works.

### Every release: sections by change

Additionally run the checklist sections touched by the release diff:

| Changed since the previous release | Run |
| --- | --- |
| Settings UI, themes, i18n, accessibility | Common: themes, languages, keyboard navigation |
| Portal, text injection, clipboard | Linux: installation and text permission |
| KDE system actions, layouts, `kde:`/`xdg:` | KDE Plasma 6 Wayland |
| GNOME extension, bridge, portal or popups | GNOME Wayland (experimental) |
| Windows backend or portable code | Windows 11 (plus VM `--suite full`) |
| Device handling, hotplug, suspend, DPI | Physical hardware |
| Packaging, autostart, install paths | Lifecycle |
| macOS shell | macOS (shell only) |

Record the exact environment, package/SHA, which tier was run and every remaining
gap as shown in the checklist's result template. A release smoke is not full
acceptance; say so in the report.
