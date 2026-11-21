# Development

All commands run from the repository root; artefacts go to the shared `target/`.

## Environment

Rust stable ≥ 1.92 (edition 2024; CI toolchain pinned in `rust-toolchain.toml`).
Skia is built from source on the first build, so clang, cmake, ninja and Python
are required.

```sh
# Arch / Manjaro
sudo pacman -S --needed base-devel clang cmake ninja python pkgconf \
  fontconfig freetype2 libxkbcommon wayland wl-clipboard
# Debian / Ubuntu
sudo apt install build-essential clang cmake ninja-build python3 pkg-config \
  libfontconfig1-dev libfreetype-dev libxkbcommon-dev libwayland-dev \
  wl-clipboard
```

Optional tools: `cargo-deny` (`cargo install --locked cargo-deny`),
`cargo-release`, `nfpm` (packages), `desktop-file-validate`, `kwin_wayland` +
`dbus-run-session` (isolated KWin checks), QEMU/KVM (VM tests).

### Device access (Linux)

```sh
sudo install -D -m 644 apps/slint-shell/packaging/linux/60-tecloid.rules /etc/udev/rules.d/60-tecloid.rules
sudo install -D -m 644 apps/slint-shell/packaging/linux/modules-load.conf /etc/modules-load.d/tecloid.conf
sudo modprobe uinput
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=misc --sysname-match=uinput
sudo udevadm trigger --subsystem-match=input --action=change
sudo udevadm settle
```

The rule grants every active local session access to keyboards, mice, touchpads
and `/dev/uinput` through logind (`uaccess`), without a new login. Other event
nodes remain restricted. This does not protect against keyloggers in the same
session. Packages grant this access to every local user while their session is
active. Existing group memberships are retained.

For systems without logind or operation outside an active local session,
explicitly add the user with `sudo usermod -aG input "$USER"` and log in again.
This fallback grants persistent access to all input devices, including from SSH.

Runtime needs per feature: mapper — `/dev/input` read + `/dev/uinput` write;
Ctrl+Alt+F11/F12 — `/dev/input` read; `text:` — `xdg-desktop-portal` with the DE
backend; active-window conditions — see [platforms.md](platforms.md).

## Running

```sh
cargo run -p slint-shell                                              # popups as winit windows
TECLOID_POPUPS=auto cargo run -p slint-shell --features spell         # layer-shell popups where available
cargo build --release --locked -p slint-shell --features spell        # release build
SLINT_LIVE_PREVIEW=1 cargo run -p slint-shell --features slint/live-preview --features spell  # .slint hot reload
```

Windows start hidden; the app lives in the tray. Open settings from the tray or
with `target/debug/tecloid show settings`.

Live Preview reloads pure `.slint` changes only; renaming a property or callback
used from Rust needs a rebuild. Quit the running instance first. Never enable
`slint/live-preview` in regular or release builds.

Renderer of the settings window: `SLINT_BACKEND=winit-skia` (default, GPU) or
`winit-skia-software`. The Slint software renderer is not built (no colour emoji).

### CLI

```sh
target/debug/tecloid show settings      # show emoji|quick [page]
target/debug/tecloid hide
target/debug/tecloid toggle-mapper
target/debug/tecloid game toggle        # auto|on|off|toggle
target/debug/tecloid preferences dark ru
target/debug/tecloid execute <action>
target/debug/tecloid ping
target/debug/tecloid quit
target/debug/tecloid --version          # --version / --help need no instance
```

Full syntax: `USAGE` in `apps/slint-shell/src/command.rs`.

A second instance next to the main one (its mapper will not start while the main
one holds the lock):

```sh
TECLOID_DEV_DIR=/tmp/tecloid-dev TECLOID_SOCKET=tecloid-dev.sock TECLOID_HOTKEYS=off cargo run -p slint-shell
TECLOID_SOCKET=tecloid-dev.sock target/debug/tecloid quit
```

## Paths

Resolved only by `tecloid_core::storage::StoragePaths::resolve()`:

| Build | Config | Data (keymap library) |
| --- | --- | --- |
| `TECLOID_DEV_DIR=<dir>` (any build) | `<dir>/<os>/` mirroring the home layout, e.g. `<dir>/linux/.config/dev.bozonx.tecloid/` | `<dir>/linux/.local/share/dev.bozonx.tecloid/` |
| debug | `dev-files/linux/.config/dev.bozonx.tecloid/config.json` | `dev-files/linux/.local/share/dev.bozonx.tecloid/` |
| release, Linux | `~/.config/dev.bozonx.tecloid/` | `~/.local/share/dev.bozonx.tecloid/` |
| release, Windows | `%APPDATA%\dev.bozonx.tecloid\` | same |

Debug builds and tests never touch the real user config. Use `dev-files/`,
`TECLOID_DEV_DIR` or a temp dir; never commit `dev-files/` contents.

## Environment variables

| Variable | Purpose |
| --- | --- |
| `RUST_LOG` | Log filter, default `info,zbus=warn,tracing=warn`. Mapper trace: `RUST_LOG=info,tecloid_core::mapper::trace=debug` (text is logged by length only) |
| `TECLOID_DEV_DIR` | Explicit config/data base directory (also for isolated release acceptance) |
| `TECLOID_POPUPS` | `winit` / `auto` / `spell` |
| `SLINT_BACKEND` | Settings window renderer |
| `TECLOID_HOTKEYS=off` | Do not listen for Ctrl+Alt+F11/F12 |
| `TECLOID_INPUT=/dev/input/eventN` | Hotkeys from one device only |
| `TECLOID_OUTPUT=<name>` | Monitor for Spell popups |
| `TECLOID_SOCKET` | IPC socket name (parallel instance) |
| `TECLOID_PORT` | Windows IPC port |
| `TECLOID_METRICS=<file.csv>` | Popup latency CSV (worker writes `<file>.spell.csv`) |
| `TECLOID_INSERT=1`, `TECLOID_TEST_INJECT=1`, `TECLOID_ISOLATED_INPUT`, `TECLOID_SPELL_LIFECYCLE`, `TECLOID_SPELL_INITIAL` | Diagnostic stands only (feature `probes`, benches) |

Names are defined in `apps/slint-shell/src/env.rs`.

## Cargo features (slint-shell)

| Feature | Use |
| --- | --- |
| `spell` | Layer-shell popup worker (Linux); CI and releases build with it |
| `probes` | Diagnostic examples and focus-return/insert probes; never in releases |
| `slint/live-preview` | Hot reload, command line only |

## Conventions

The authoritative rules are in [AGENTS.md](../AGENTS.md). In short: domain and
platform logic in `tecloid-core`; platform code behind `#[cfg]`; dispatch on
`platform::linux::detect()`; `Result` at module boundaries, no `unwrap()` on
runtime paths; typed values across boundaries; all UI text via `@tr`
([i18n.md](i18n.md)); UI follows [ui-principles.md](ui-principles.md); minimal
focused diffs.

Every user-visible change gets an entry under `## [Unreleased]` in
`CHANGELOG.md` ([release.md](release.md#changelog)).

## Troubleshooting

- **Second launch opens settings instead of a new instance:** the socket is
  taken — `tecloid quit` or set another `TECLOID_SOCKET`. A stale socket after a
  crash is removed on the next start.
- **First build is very slow or fails in Skia:** check clang, cmake, ninja, Python.
- **`No readable Ctrl+Alt+F11/F12 evdev devices`:** no `/dev/input` access — set
  up permissions or `TECLOID_HOTKEYS=off`.
- **Monochrome or boxed emoji:** use `winit-skia`. The app draws emoji with the bundled
  CBDT font `apps/slint-shell/ui/fonts/TecloidEmoji.ttf` (Noto Color Emoji renamed to
  "Tecloid Emoji"); our Skia build draws COLRv1 emoji fonts, such as Fedora's, blank.
- **`compositor does not advertise zwlr_layer_shell_v1`:** no layer-shell (GNOME) —
  use `TECLOID_POPUPS=auto` or `winit`.
- **`RefCell already borrowed` panic on a page:** a `Document::read()` borrow is
  alive while a callback edits; drop it before calling into the UI.
- **Windows: `Access is denied (os error 5)` at link:** quit the running
  `tecloid.exe` before rebuilding. PowerShell shows Cargo's stderr in red; trust
  `$LASTEXITCODE`.

Removing the rule does not immediately revoke ACLs already granted in the
current session. End the local session or reboot after removing permissions.
Other system rules may independently grant access to some devices.

## Experimental features and debug isolation

`settings.experimental` (Rust/UI: `in_development`) is a runtime opt-in,
independent of the build profile. Experimental code ships in release builds.
Enable it through the hidden area at the bottom right of the general settings;
once enabled, the visible checkbox can turn it off.

With the flag off, experimental controls and entry points must be absent, not
disabled placeholders. Experimental runtime behaviour must be inactive even if
its settings remain enabled in a previously saved file. Keep those settings and
keymap data intact so enabling the flag again restores them. Stable features
must remain usable, including mouse support, standard system shortcuts and
built-in macros. The flag gates game mode and rules that depend on it, automatic
keymap selection, user macros, commands, desktop actions, long-hold actions,
E-ink appearance, keymap OSD and held-layer hints. Assigning a key to open a
specific or recent Emoji or Quick actions page also requires the flag. The
built-in hotkeys and tray entries for the popups are available in either mode.

Debug storage isolation serves a separate purpose: development and tests must
not modify real user configuration or OS registrations. Debug builds use the
development storage layout; `TECLOID_DEV_DIR` explicitly isolates any build,
including release. Neither mechanism enables experimental features.
