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

## Before a release

Automated: L0–L2 green, L3 on current `main`, L4 at least Fedora KDE (rpm) and
Debian 13 KDE (deb), Windows VM `--suite full` after changes to portable code.

Manual on physical KDE Plasma 6 Wayland and Windows 11 machines, with a test
keymap (never a personal config):

- Natural typing and fast overlapping presses on real keyboards; tap/hold feels
  right; left/right modifiers, numpad, media keys, extra mouse buttons; Caps/Num
  Lock LEDs.
- Two keyboards, USB hub/dock, Bluetooth; physical unplug/replug, port change,
  sleep/wake with hubs and external monitors.
- Monitor/dock changes, non-standard DPI, mixed scaling; popups usable.
- A real game: fullscreen/borderless, Alt+Tab, global shortcut conflicts.
- Visual check of every theme (system, light, dark, E-ink, high contrast) and
  every language; emoji rendering; focus, popups and tray in normal work.

Record OS/desktop/session, package version and SHA, device models, screens/DPI,
the checked items and remaining gaps.
