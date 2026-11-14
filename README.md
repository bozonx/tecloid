# Tecloid

A desktop keyboard mapper written in Rust and Slint. Tecloid intercepts a
physical keyboard, applies layers, rules, macros and commands, and emits the
result through a virtual device. It lives in the system tray; Emoji and Quick
action popups open from global hotkeys.

Primary platform: **Linux, KDE Plasma 6 on Wayland**. GNOME Wayland is
experimental (bundled Shell extension). Windows 11 has a mapper backend and a
portable build; macOS has the shell without key interception. Details:
[platforms](docs/platforms.md).

## Features

- **Rules per physical key:** tap, hold, double tap, long hold, chords, retro
  tap, configurable tap/hold decision.
- **Layers** with transparent keys, extra keys and held modifiers (Alt+Tab-style
  window switching).
- **Actions:** key chords, literal Unicode text, macros with pauses, built-in
  editing macros, shell/PowerShell commands, system actions (desktops, windows,
  layouts, lock), KDE global shortcuts, freedesktop application actions.
- **Popups:** Emoji and Quick actions grids (Ctrl+Alt+F11 / Ctrl+Alt+F12) that
  insert into the previously focused window.
- **Keymap library** with manual or automatic selection by active application,
  window title, keyboard layout and game mode.
- **Game mode** detection (GameMode, Steam, Lutris, Heroic, umu, gamescope,
  fullscreen) — experimental.
- Themes (system, light, dark, E-ink, high contrast), 21 interface languages,
  CLI control of the running instance.

## Build and run

Rust stable ≥ 1.92. On Linux install clang, cmake, ninja, Python and the D-Bus,
Fontconfig, Wayland and xkbcommon development libraries; the mapper needs
access to `/dev/input` and `/dev/uinput`. Full setup:
[development guide](docs/development.md).

```sh
cargo run --locked -p slint-shell
TECLOID_POPUPS=auto cargo run --locked -p slint-shell --features spell   # layer-shell popups
```

The app starts hidden in the tray. Control the running instance with the CLI:

```sh
target/debug/tecloid show settings      # show emoji|quick, hide, toggle-mapper, game, quit, --help
```

Debug builds keep their config in `dev-files/` inside the repository and never
touch your real settings.

## Tests

```sh
cargo fmt -p tecloid-core -p slint-shell --check
cargo clippy --locked -p tecloid-core -p slint-shell \
  --features slint-shell/spell --all-targets --no-deps -- -D warnings
cargo test --locked --workspace --features slint-shell/spell
cargo test --locked -p tecloid-core --test mapper_e2e -- --ignored   # needs /dev/uinput
```

UI scenarios, desktop and VM acceptance: [testing](docs/testing.md).

## Documentation

- [Developer documentation index](docs/README.md): architecture, development,
  testing, platforms, config format, i18n, release
- [Instructions for AI agents](AGENTS.md)
- [Changelog](CHANGELOG.md) · [Security policy](SECURITY.md)

User documentation is published on the documentation site.

License: [GPL-3.0-or-later](LICENSE).
