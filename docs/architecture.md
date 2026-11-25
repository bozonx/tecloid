# Architecture

Tecloid intercepts a physical keyboard (and optionally a mouse), runs the events
through a rule engine (layers, tap/hold, chords, macros, commands) and emits the
result through a virtual device. Settings, a tray icon and two popups (Emoji,
Quick actions) form the UI.

## Crates

```
crates/tecloid-core/     domain + platform code, no UI framework
apps/slint-shell/        the product: Slint UI, tray, popups, IPC, CLI; binary `tecloid`
apps/slint-shell/vendor/ patched i-slint-core ([patch.crates-io]) and spell-framework (workspace member)
```

**Rule of thumb:** if logic could be reused by another shell or tested without a
UI, it belongs in `tecloid-core`. The core never calls Slint.

### tecloid-core

| Module | Responsibility |
| --- | --- |
| `config/` | Domain model, readers/writers of `config.json` and keymaps (`*Doc` structs), `schema.rs` versions, `actions.rs` action strings, `app_action.rs`, `app_match.rs`, diagnostics |
| `config_document/` | Editable document: transactions, validation, saving, active keymap, library operations |
| `storage.rs` | `StoragePaths::resolve()`, atomic writes, `.v<N>.bak` backups |
| `events.rs` | `CoreEvent` bus — the only core → shell channel |
| `mapper/` | `engine/` (pure state machine), `output.rs` (shared output loop), `linux/` (evdev + uinput), `windows/` (hooks + `SendInput`), `system.rs` (system actions per DE), `runtime.rs` (lifecycle), `key_code.rs`, `scancode.rs`, `builtin_macros.rs` |
| `text_input/` | `text:` injection: portal (RemoteDesktop + libei), clipboard paste, keycode, ydotool/xdotool, job accounting |
| `keyboard_layout/`, `gamemode/`, `active_window/` | Watchers feeding rule conditions and auto keymap selection |
| `platform/` | OS / DE / session detection (`platform::linux::detect()`), KDE and GNOME integrations |
| `assets/` | Bundled keymap templates, GNOME Shell extension |

### slint-shell

| Path | Responsibility |
| --- | --- |
| `src/lib.rs` | Process roles |
| `src/app/` | Settings process: popups, mapper bridge (`mapper.rs` subscribes to `CoreEvent`), Spell worker supervision, layer hint |
| `src/document.rs` | The single config write path (`Document`) |
| `src/pages/` | One module per settings page, each bound to one Slint global |
| `src/command.rs`, `src/ipc.rs` | Typed commands and the IPC wire format |
| `src/popup_model.rs` | Popup data and key handling shared by winit and Spell |
| `src/spell.rs` | Layer-shell popup worker (feature `spell`, Linux) |
| `src/i18n.rs` | Message ids, plural rules ([i18n.md](i18n.md)) |
| `src/env.rs` | Names of `TECLOID_*` variables |
| `src/platform/linux/` | evdev hotkeys, ksni tray, Wayland activation, focus return |
| `src/platform/portable/` | Windows/macOS: `global-hotkey`, `tray-icon`, native input |
| `ui/` | `.slint` files: `app.slint` entry, `types.slint` enums, `i18n.slint`, `theme.slint`, `controls*.slint`, `widgets/` (trimmed Fluent copy, see its README) |
| `build-support/` | Build-time i18n checks, `rust_messages.rs` |
| `packaging/` | Linux nfpm, udev rule, desktop file, metainfo, icons; bundle READMEs |

## Processes

One binary, three roles:

1. **Settings process** (no arguments): tray, settings window, mapper, watchers,
   winit popups, IPC server. A second launch without arguments opens settings
   of the process at the same IPC endpoint. On Unix, the settings process also
   locks its settings directory, so parallel instances require separate
   endpoints and separate settings directories.
2. **Spell worker** (`--spell-worker`): both layer-shell popups in a child
   process, used with `TECLOID_POPUPS=auto|spell`. The parent restarts it at most
   three times a minute and restores theme and language; the worker exits on
   parent exit or stdin EOF.
3. **CLI client** (any command): sends one command over IPC and exits.
   `--version`/`--help` need no instance.

On Linux the mapper holds an account-wide file lock across runtime directories.
On Windows it holds a named mutex. This keeps keyboard grabs exclusive even
when multiple settings processes run.

## IPC

- Linux/macOS: Unix socket in a private `0700` directory
  (`$XDG_RUNTIME_DIR/tecloid-slint-shell/`, or `<temp>/tecloid-slint-shell-<uid>/`).
- Windows: loopback TCP `127.0.0.1:43176` (`TECLOID_PORT`), every request signed
  with a token from `%LOCALAPPDATA%\tecloid-slint-shell\ipc-token` (or the
  `TECLOID_DEV_DIR` tree).
- Wire format: one JSON line with a text command; reply `queued` or `error: …`.
  `queued` means accepted by the target's queue, not executed. Action errors are
  delivered through events and notifications.
- `command::Command` is the CLI/IPC representation; `ShellCommand` and
  `WorkerCommand` are per-executor sets (`TryFrom` rejects the wrong target, so
  the worker cannot be asked to show settings). `source` is provenance metadata
  for focus and game mode, not authorization.
- Settings → worker commands go through a bounded FIFO with one background
  sender, preserving the order of config, appearance and show. On a transport
  error the sender stops and reports; a new worker gets a new socket and the
  current config. The UI thread never waits on the socket.

CLI commands and the wire format are a public contract (see
[AGENTS.md](../AGENTS.md#compatibility-contract)). Syntax: `USAGE` in
`src/command.rs`.

## Configuration flow

```
config.json + keymaps/*.json
        │ readers (config/)
        ▼
ConfigDocument ── transaction() ──► save (storage.rs, atomic, 0600)
        │ runtime_config()
        ▼
RuntimeConfig { MapperConfig, menus, … } ──► mapper::runtime ──► engine
```

- Pages change configuration only through `Document`:
  - `edit()` runs `ConfigDocument::transaction()`: changes are prepared on a
    copy; settings are saved after a successful callback. A failing callback or
    save leaves memory and files unchanged. No direct file side effects via
    `paths()` inside the callback.
  - `edit_library()` runs exactly one core library operation with its own file
    rollback. Compound operations live in the core
    (`create_and_select_library_keymap()`), not in a callback.
  - `sync_autostart()` refreshes system autostart registration.
- After a successful change the shell bumps the revision, pushes the runtime
  config to the mapper and notifies views. `Saved<T>` distinguishes an applied
  change from a mapper sync error.
- Never hold a `Document::read()` borrow across UI calls that may edit
  (`RefCell already borrowed` panic).
- Settings auto-save (choices immediately, text after 500 ms). Keymaps are
  written to the library only on explicit save; the editor works on an in-memory
  working copy. Library JSON files and `config.json` are watched; a fixed load
  error is picked up automatically.
- The running mapper uses the menus from the `RuntimeConfig` snapshot passed with
  its rules; a failed context switch keeps the old rules and menus.

## Events

`CoreEvent`s are published on `tecloid_core::events::bus()`; the shell subscribes
once in `src/app/mapper.rs` and posts to the Slint event loop.

- Each subscriber has its own queue (256 events) and thread; producers never wait
  for the UI.
- State notifications (keymap, game mode, active window, held layer, device
  availability) coalesce: a pending one of the same kind is replaced.
  Diagnostics coalesce by kind. On overflow the oldest request is dropped.
- `AppAction` requests keep relative order, expire after one second and are
  revoked when the mapper stops or restarts. `subscribe_with_context` passes
  origin time and generation so the shell can recheck them.
- The shell keeps at most one core callback queued on the UI thread. A
  subscription does not replay a snapshot: the shell reads state at startup.

## Mapper

- **Engine** (`mapper/engine/`): a deterministic state machine driven by events
  with explicit `Instant`s, `next_deadline()` and `tick()`. Handles base rules,
  layers (transparent keys fall through), tap/hold with `permissiveHold` /
  `holdOnOtherKeyPress`, retro tap, double tap, long hold, chords, macros (one
  running + three queued, global timeout), `holdModifiers`.
- **Key codes:** Linux key codes everywhere (`mapper::key_code`); Linux adapts to
  evdev, Windows to set-1 scan codes (`mapper::scancode`).
- **Output loop** (`mapper/output.rs`) is shared; backends provide an event sink
  and a loop driver.
- **Linux:** grabs the selected keyboard (and mouse) via evdev, emits through
  uinput. Interfaces that also advertise pointer axes are rejected before grab.
  A selected mouse is grabbed too: motion goes through a separate virtual
  pointer, buttons through the engine, so mod-tap modifiers precede clicks.
- **Runtime guarantees:** live config updates keep held virtual keys and layer
  triggers until physical release; identical configs are ignored; changed configs
  cancel the active macro, stale text jobs and pending popup side effects; stop
  and crash release held keys. An action this OS/desktop cannot run (`kde:` off
  KDE, `xdg:` off Linux, `sys:switchToDesktopN` on Windows, a command without a
  script for this OS) is skipped with an "unavailable" notice; a macro containing
  one is skipped as a whole.
- **Text method selection contract**: resolve the initial recommendation before
  dispatch. An explicitly selected method must either perform the operation or
  report its failure through `CoreEvent::TextInputFailed`. Only the explicit
  libei + clipboard mode may switch mechanisms: if the active keymap cannot
  represent the entire string, paste it once before typing any characters.
  A lost focus, timeout, denied permission or partial write must end the job.
  The legacy X11 mapping of saved libei modes to xdotool is documented in
  [the platform matrix](platforms.md) and must be handled when loading legacy
  settings so it cannot mask the result of an explicit backend test.
- **Text input** (`text_input/`): jobs ≤ 1 MiB, ≤ 16 queued, 15 s queue age,
  15 s without progress. Keys pressed after a `text:` wait until it is typed
  (dropped after 4 s with a notice). The destination window is captured with the
  job and rechecked before input; see [platforms.md](platforms.md#text-input).

## Popups

`TECLOID_POPUPS`:

- `winit` (default): borderless in-process winit windows; always-on-top requested.
- `auto`: Spell layer-shell worker if the compositor advertises
  `zwlr_layer_shell_v1` (KDE, Sway, Hyprland), otherwise winit (GNOME).
- `spell`: layer-shell required; startup fails without it.

`popup_model.rs` holds data and key handling, so both paths behave identically.
Spell renders with Skia software + Wayland SHM (`SLINT_BACKEND` affects only the
parent). Layers: Emoji 520×460, Quick 520×500, overlay, anchored bottom with a 24
px margin; `TECLOID_OUTPUT` picks the monitor. Spell lifecycle patches are in
`vendor/spell-framework/PATCHES.md`.

Popups open from Ctrl+Alt+F11 (Emoji) / Ctrl+Alt+F12 (Quick) via evdev
listeners (devices are not grabbed; disable with `TECLOID_HOTKEYS=off`), from the
tray, IPC (`show emoji|quick`) and `app:` actions. Cells are chosen by the
grid letters independent of layout; Tab/PgDn and Shift+Tab/PgUp switch pages,
digits jump to a page, Escape closes, focus loss hides. The chosen action runs in
the mapper after the popup has returned focus. Built-in hotkeys are suppressed
while game mode is on.

On Wayland the IPC client forwards `XDG_ACTIVATION_TOKEN` and the shell applies
it via `xdg_activation_v1`; evdev-triggered shows have no token.

## Persistence and paths

See [config-format.md](config-format.md) for formats and the compatibility
contract, [development.md](development.md#paths) for directories.

## Autostart

Linux: XDG Autostart entry `~/.config/autostart/dev.bozonx.tecloid.desktop`
(respecting `XDG_CONFIG_HOME`) with `TryExec`. Windows: `dev.bozonx.tecloid` in
`HKCU\…\CurrentVersion\Run`. The system is the source of truth: on every launch
the setting is reconciled with the registration, and a live entry is rewritten to
the current executable. Debug builds and `TECLOID_DEV_DIR` write into the dev tree
instead of the user session.
