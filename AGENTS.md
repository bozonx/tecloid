# AGENTS.md

Rules for AI coding agents in this repository. Read before making changes.
Details live in [`docs/`](docs/README.md); this file is the short, prescriptive
version.

## Project

**Tecloid** is a desktop keyboard mapper: it intercepts a physical keyboard,
applies layers, rules, macros and commands, and emits the result through a
virtual device. It also shows popups (Emoji, Quick actions) and lives in the
system tray.

- The product is the Slint shell `apps/slint-shell/` on top of
  `crates/tecloid-core/`. `_old/` is an inactive archive.
- Primary and only accepted target: **KDE Plasma 6 on Wayland**. GNOME Wayland
  (Shell extension) is experimental, Windows has a mapper backend, macOS only the
  shell. Matrix: [`docs/platforms.md`](docs/platforms.md).
- **Formats are frozen from 0.5.0 on.** Users of 0.5.0 and later have
  `config.json` files and keymaps on disk; never break them (see
  [Compatibility contract](#compatibility-contract)). 0.4.0 had no users; its
  format is not supported.

Stack: Rust edition 2024 (MSRV 1.92, toolchain in `rust-toolchain.toml`), Cargo
workspace, Slint **=1.18.1** with winit 0.30, `ksni`, `evdev`/`uinput`, `zbus`,
vendored `spell-framework` for layer-shell popups.

## Commands

From the repo root; artefacts go to the shared `target/`.

```sh
cargo run -p slint-shell                                       # popups as winit windows
TECLOID_POPUPS=auto cargo run -p slint-shell --features spell  # layer-shell popups
target/debug/tecloid --help                                    # CLI client (USAGE in command.rs)
```

Checks (CI):

```sh
cargo fmt -p tecloid-core -p slint-shell --check
cargo clippy --locked -p tecloid-core -p slint-shell \
  --features slint-shell/spell --all-targets --no-deps -- -D warnings
cargo test --locked --workspace --features slint-shell/spell
cargo check --locked -p slint-shell --all-targets              # build without Spell
cargo deny check
```

While iterating run only affected tests (`cargo test --locked -p <crate> --lib
--tests`, or a single test by name). Builds are heavy and the machine and
`target/` may be shared:

- run full workspace tests, clippy `--all-targets` and UI scenarios once, at the end;
- do not start UI scenarios or e2e while another build or test is running;
- do not raise `CARGO_BUILD_JOBS` / `RUST_TEST_THREADS`.

UI scenarios (graphical session, temp config, exit on their own):

```sh
cargo run --locked -p slint-shell --example editor -- --smoke
cargo run --locked -p slint-shell --example interactions
```

Page examples, mapper e2e, VM stands: [`docs/testing.md`](docs/testing.md).
Environment and run modes: [`docs/development.md`](docs/development.md).

## Architecture

Full description: [`docs/architecture.md`](docs/architecture.md).

- **Core vs shell.** `tecloid-core` owns config types and `ConfigDocument`,
  storage, platform detection, watchers, mapper, device I/O, text injection,
  validation. The shell displays state and calls core operations. Logic that
  could be reused or tested without a UI goes into the core.
- **Events.** The core never calls a UI framework. It publishes `CoreEvent`s on
  `tecloid_core::events::bus()`; the shell subscribes once in
  `apps/slint-shell/src/app/mapper.rs`. No shell-specific callbacks in the core.
- **Config writes** only through `Document` (`src/document.rs`): `edit` for
  settings/keymap transactions, `edit_library` for one core library operation,
  `sync_autostart` for system registration. Never hold a `Document::read()`
  borrow across UI calls that may edit.
- **Paths** only from `StoragePaths::resolve()`.
- **Processes.** One binary: settings process, Spell worker (`--spell-worker`),
  CLI client over IPC. The mapper holds a cross-process lock.
- **Popups.** `popup_model.rs` keeps winit and Spell behaviour identical.
- **Action strings** (`Ctrl+KeyC`, `macro:`, `builtin:`, `cmd:`, `sys:`,
  `app:`, `kde:`, `xdg:`, `text:`, `pause:`, `null`, empty) are parsed and
  built only through `Action` in `tecloid_core::config::actions`.

### Where does a change go?

| Change | Location |
| --- | --- |
| Config field, validation, file format | `crates/tecloid-core/src/config/`, `config_document/` |
| Mapper behaviour, key handling, system actions | `crates/tecloid-core/src/mapper/` |
| Text injection | `crates/tecloid-core/src/text_input/` |
| OS/DE detection or watcher | `crates/tecloid-core/src/platform/`, `keyboard_layout/`, `gamemode/`, `active_window/` |
| GNOME Shell side | `crates/tecloid-core/assets/gnome-extension/` |
| Settings page | `apps/slint-shell/src/pages/<page>.rs` + `ui/<page>.slint` |
| Popup content or keys | `apps/slint-shell/src/popup_model.rs`, `ui/popup-*.slint` |
| Tray, global hotkeys, focus return | `apps/slint-shell/src/platform/{linux,portable}/` |
| CLI/IPC command | `apps/slint-shell/src/command.rs` |
| User-visible text | `src/i18n.rs` + `ui/i18n.slint` + every PO catalog ([`docs/i18n.md`](docs/i18n.md)) |

## Compatibility contract

Current format versions: **`config.json` = 1, keymap = 1**
(`CONFIG_SCHEMA_VERSION`, `KEYMAP_SCHEMA_VERSION` in
`crates/tecloid-core/src/config/schema.rs`). Details:
[`docs/config-format.md`](docs/config-format.md#compatibility-contract).

- **Every file a released build wrote must keep loading** with the same meaning.
  Readers are strict (unknown fields/values are errors), so an incompatible
  change does not degrade gracefully — it makes the user's file unloadable.
- **Within a version** only add optional fields, enum values, or an object form
  next to an existing string form. Never rename, remove or retype fields or enum
  values, never change the meaning of an existing value.
- **Anything else bumps the version**: move the old `*Doc` structs into `vN`,
  add a reader arm converting them, publish a new schema, add a fixture test with
  a real old file. Storage keeps `<file>.v<N>.bak` before overwriting. No
  fallbacks for files without `version`.
- **Action ids are data in user keymaps:** `sys:`, `app:`, `builtin:` ids and
  key names are never renamed or removed; add new ones instead. They are
  listed in `schemas/names.v1.txt` (test `stored_names_are_never_removed`),
  fields and enum values in `schemas/contract.v1.json`, size limits in
  `schemas/limits.v1.txt` (only widen), built-in macro steps (a reviewed
  snapshot; changing them is a CHANGELOG-documented behaviour change) in
  `schemas/builtin-macros.v1.txt`, value grammars (window patterns,
  `kde:`/`xdg:`, device ids, …) as examples in `schemas/grammar.v1.txt`;
  the fixtures `crates/tecloid-core/fixtures/*.v1.json` are never edited.
- `config.json` is written in full, defaults included.
- **Defaults that affect input** (`settings.timing`, tap decision) change only as
  a deliberate, CHANGELOG-documented behaviour change.
- **Never lose user data:** a file that failed to parse is not overwritten;
  upgrades and uninstalls keep config and keymaps.
- After a format change regenerate schemas:
  `TECLOID_UPDATE_SCHEMAS=1 cargo test --locked -p tecloid-core --lib published_schemas`.
- CLI commands, the IPC wire format and installed paths (binary, desktop id
  `dev.bozonx.tecloid`, autostart entry, udev rule) are public too.

## Conventions

**Rust**
- Platform code behind `#[cfg(...)]` and target-specific dependencies; shared UI
  and core logic are OS-independent.
- Dispatch on `platform::linux::detect()`, never on raw `XDG_CURRENT_DESKTOP`.
- `Result<_, String>` or typed errors at module boundaries; no `unwrap()` on
  runtime paths (build scripts and tests are fine).
- Typed values across boundaries (`command::Window` / `Popup`, `ui/types.slint`
  enums).
- Comments match surrounding code: short `//!` module headers, `///` where the
  purpose is not obvious, inline comments explain *why*.
- Minimal, focused diffs; no drive-by refactors.

**Slint UI**
- Follow [`docs/ui-principles.md`](docs/ui-principles.md) (e.g. delete buttons
  are never red).
- One Slint global per settings page, bound by one module in `src/pages/`.
  Shared widgets from `controls*.slint`, colours and metrics from `theme.slint`.
  System, light, dark, E-ink, high-contrast and reduce-motion must all work.
- All visible text via `@tr(...)`; Rust sends `i18n::Msg`, never formatted UI
  text. A new message updates `src/i18n.rs`, `ui/i18n.slint` and **every** PO
  catalog together (Russian by hand).

**Vendored crates**
- Every change in `vendor/i-slint-core` or `vendor/spell-framework` is recorded in
  that directory's `PATCHES.md`. On a Slint upgrade, port or drop patches and keep
  the `=1.18.1` pins (`slint`, `i-slint-core`, `slint-build`) in sync.

## Definition of done

1. fmt, clippy `-D warnings` and workspace tests pass.
2. `cargo check --locked -p slint-shell --all-targets` passes.
3. UI changes: `editor -- --smoke`, `interactions` and the relevant page example
   pass in a graphical session.
4. New strings: all catalogs updated, build-time translation checks pass.
5. Format changes: old-version fixture loads, schemas regenerated.
6. User-visible changes: entry under `## [Unreleased]` in `CHANGELOG.md` (no
   version or date).
7. Platform-dependent behaviour: state the verified environment and what remains
   untested.

Report the exact commands you ran and their results. If something could not run
(no graphical session, no `/dev/uinput`), say so.

## Boundaries

**Always**
- Domain and platform logic in `tecloid-core`; keep the shell thin.
- Config through `Document`, paths through `StoragePaths::resolve()`.
- Keep every released config/keymap version loadable.

**Ask first**
- Any change to the on-disk config or keymap format, including a version bump.
- Renaming or removing an action id, CLI command, IPC message, installed path or
  setting.
- Changing a default that affects input behaviour.
- Adding a dependency or bumping Slint / `spell-framework` / `i-slint-core`.
- `cargo release --execute` or pushing `v*` tags.
- Extending X11 support or touching `_old/`.

**Never**
- Read or write the real user config from debug builds or tests (use
  `dev-files/`, `TECLOID_DEV_DIR` or a temp dir).
- Hardcode absolute paths, call Slint from `tecloid-core`, add UI text outside `@tr`.
- Use `_old/` as specification or depend on it.
- Commit build artefacts, test results, dev config/data or dependency caches.
- Enable `slint/live-preview` in regular or release builds.
