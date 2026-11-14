# Config and keymap file formats

Tecloid stores two kinds of files:

- `config.json` — settings of this computer (auto-saved);
- `keymaps/<name>.json` — library keymaps (saved explicitly; portable between computers).

Paths come from `StoragePaths::resolve()` (see [development.md](development.md#paths)).
Both files are written only by the application, atomically, with mode `0600`.
Manual editing is unsupported: it applies after a restart at best and may be
overwritten by the next save.

Readers live in `crates/tecloid-core/src/config/` (`settings.rs`,
`keymap_file.rs`); versions in `config/schema.rs`. **Current versions:
`config.json` = 1, keymap = 1.** These are released formats: user files in
these versions must keep loading in every future build.

## Compatibility contract

- Every file has a root `version` (format version, not content version; a
  keymap's own version is `meta.keymapVersion`). A file without `version` is
  rejected. A file with a newer `version` is rejected with a "created by a newer
  version" error and is never overwritten.
- Parsing is strict: unknown fields, wrong types and unknown enum values are
  errors with line and column. A file that failed to parse is never rewritten,
  so the application never drops parts it did not understand.
- **Allowed within a version:** new optional fields, new enum values, and an
  object form for a value currently written as a string (as layer keys and
  quick-menu cells already have). The string form stays valid and the meaning of
  existing fields does not change.
- **Anything else bumps `version`**: renaming or removing a field or enum value,
  changing a type or meaning, making a field required, changing a default that
  affects input behaviour (`settings.timing`). Procedure (header of
  `config/schema.rs`): move the current `*Doc` structs into a `vN` module, bump
  the constant, add a reader arm `N => …` that converts the old structs, publish
  `schemas/<format>.v<N+1>.schema.json`, add a fixture test with a real old file.
- The application always writes the current version. Before it first overwrites
  an older-version file, storage keeps `<file>.v<N>.bak` next to it (an existing
  backup is not replaced).
- Defaults are not written. `config.json` stores only values that differ from
  defaults, so changing a default changes behaviour for everyone who did not set
  it; input-affecting defaults change only as a user-visible change in
  `CHANGELOG.md`.
- Semantic problems (reference to a missing macro, unknown key in an action) do
  not block loading; diagnostics report them and the mapper refuses to start
  with blocking errors.

Naming: fields and enum values are camelCase (`permissiveHold`, `eInk`); key
combinations use action syntax (`Ctrl+Shift+KeyV`); languages are BCP 47 codes.

### JSON Schemas

Both formats accept an optional root `$schema` string (kept on save, never added,
never used to pick the format). Schemas generated from the `*Doc` structs are
published in [`schemas/`](../schemas/):

- `https://raw.githubusercontent.com/bozonx/tecloid/main/schemas/config.v1.schema.json`
- `https://raw.githubusercontent.com/bozonx/tecloid/main/schemas/keymap.v1.schema.json`

They are editor hints (fields, enums, ranges, id grammar), not the validator.
After a format change regenerate and commit them:

```sh
TECLOID_UPDATE_SCHEMAS=1 cargo test --locked -p tecloid-core --lib published_schemas
```

Without the variable the test fails while the files are stale.

## Shared rules

### Identifiers

`id` is unique (case-sensitive) within its collection: layers, rules, macros,
commands, quick/emoji pages, auto rules, game-mode processes. Grammar:
`[A-Za-z0-9_-]{1,64}`, so ids appear unescaped in `macro:<id>`, `cmd:<id>`.
Duplicates are load errors. Rules, pages, auto rules and processes may omit
`id`; the loader generates `rule-1`, `quick-1`, `emoji-1`, `process-1` after
reserving explicit ids, and saving writes them.

Library keymap ids are `user:<file name without .json>`; the file name may
contain any characters allowed by the file system. Renaming a keymap in the app
updates every reference in `config.json`.

### Conditions (`when`)

Used by keymap rules and auto rules (`settings.keymaps.auto.rules`). All given
conditions must match; an empty object always matches.

| Field | Meaning |
| --- | --- |
| `gameMode` | `on` / `off` |
| `keyboardLayouts` | short system layout ids (`us`); any matches |
| `windows` | active-window patterns; any matches |
| `excludeWindows` | patterns that block the condition; if the active window is unknown, the condition fails |

A window pattern is an app id or a full process name, case-insensitive
(`org.telegram.desktop`, `Game.exe`; `.exe` optional), with `*`/`?` wildcards,
or `title:<text>` (title contains text). Implemented in `config::app_match`.

### Timing

All durations are integer milliseconds. Out-of-range, fractional and negative
values are rejected (no silent clamping). Explicit `null` in timing fields is
forbidden. A missing rule/macro timing inherits `settings.timing` and is omitted
on save. Zero is allowed only for step pause and modifier delay.

| Fields | Range, ms |
| --- | --- |
| `timing.holdMs`, `doubleTapMs`; `rules[].hold.timeoutMs`, `doubleTap.timeoutMs` | 50–1000 |
| `timing.longHoldMs`; `rules[].longHold.timeoutMs` | 100–60000 |
| `rules[].hold.retroTapMs` | 0–5000 |
| `timing.chordMs` | 10–500 |
| `timing.macroStepPauseMs`, `macroModifierDelayMs`; `macros[].stepPauseMs`, `modifierDelayMs` | 0–2000 |
| `pause:<ms>` macro step | 1–10000 |
| `timing.macroTimeoutMs` | 1000–600000 |

## Actions

An action is a string, parsed and built only through
`tecloid_core::config::actions::Action`:

| Kind | Example |
| --- | --- |
| key or chord (W3C `KeyboardEvent.code` names) | `KeyA`, `Ctrl+KeyC`, `Ctrl+Shift+ArrowLeft` |
| user macro / built-in macro | `macro:myMacro`, `builtin:copyLineText` |
| command | `cmd:<id>` |
| system action | `sys:switchToDesktop2`, `sys:showDesktop` |
| Tecloid action | `tecloid:showQuickMenu1`, `tecloid:toggleGameMode` |
| KDE global shortcut | `kde:kwin/Window Maximize` |
| freedesktop application action (Linux) | `xdg:firefox/new-private-window` |
| literal text | `text:Hello` |
| pause (macro steps only) | `pause:200` |

Where an action is optional, a missing field means native behaviour of the
physical key and `null` means "do nothing". An empty string is an error. An
unknown `word:…` prefix (e.g. from a newer build) loads and saves unchanged, but
diagnostics report it and the mapper does not start with it.

- **Chords:** modifiers then key, joined by `+`. `Ctrl`, `Alt`, `Shift`, `Meta`
  mean the left modifier; sided codes (`ControlRight`) are explicit. Canonical
  order `Ctrl`, `Alt`, `Shift`, `Meta`; other orders and full names are accepted
  and not normalized on load.
- **Key names:** W3C `KeyboardEvent.code`, then Chromium names, then names based
  on Linux `KEY_*` (`WLAN`, `VideoPrev`, …).
- **Built-in macros** (`mapper/builtin_macros.rs`) start with a verb: `goTo…`,
  `select…`, `copy…`, `cut…`, `paste…`, `replace…WithClipboard`, `insert…`,
  `duplicate…`. `builtin:` and `macro:` namespaces are independent.
- **`sys:`**: `closeWindow`, `minimizeWindow`, `maximizeWindow`,
  `maximizeWindowVertically`, `toggleWindowKeepAbove`, `moveWindowToNextDesktop`,
  `moveWindowToPreviousDesktop`, `moveWindowToNextScreen`,
  `moveWindowToPreviousScreen`, `switchToNextDesktop`, `switchToPreviousDesktop`,
  `showDesktop`, `showOverview`, `showAppLauncher`, `turnOffScreen`,
  `lockSession`, and `switchToDesktopN`, `moveWindowToDesktopN`,
  `switchToKeyboardLayoutN` (N = 1…10). Several map to system toggles. Platform
  support: [platforms.md](platforms.md).
- **`xdg:<desktop-id>/<action>`**: desktop-id is the `.desktop` file name without
  extension (`kde4/foo.desktop` → `kde4-foo`), action is from `Actions=`.
  `DBusActivatable=true` apps get `org.freedesktop.Application.ActivateAction`,
  others run the action's `Exec`.
- **`tecloid:`** (`config/app_action.rs`): `showQuickMenuN`, `showEmojiMenuN`
  (N ≥ 1, past the end opens the last page), `showQuickMenuRecent`,
  `showEmojiMenuRecent`, `showSettings`, `activateKeymapN` (N = 1…5, by
  `settings.keymaps.order`), `toggleGameMode`, `setGameModeAuto|On|Off`.
  References are by position on purpose; reordering does not rewrite them.

## config.json

Example with every field (the app writes only non-defaults):

```json
{
  "version": 1,
  "settings": {
    "experimental": false,
    "launchAtLogin": false,
    "startMapperOnLaunch": true,
    "appearance": "system",
    "highContrast": "system",
    "reduceMotion": "system",
    "keymapOsd": false,
    "layerHint": false,
    "copyOsd": false,
    "locale": "auto",
    "devices": {
      "keyboards": ["/dev/input/by-id/usb-Keyboard-event-kbd"],
      "mice": ["/dev/input/by-id/usb-Mouse-event-mouse"]
    },
    "timing": {
      "holdMs": 200, "longHoldMs": 1000, "doubleTapMs": 200, "chordMs": 50,
      "tapDecision": "permissiveHold",
      "macroStepPauseMs": 20, "macroModifierDelayMs": 5, "macroTimeoutMs": 10000
    },
    "keymaps": {
      "mode": "manual",
      "manualKeymap": "user:Main",
      "order": ["user:Main", "user:Blender"],
      "auto": {
        "rules": [
          { "id": "games", "keymap": null, "when": { "gameMode": "on" } },
          { "id": "blender", "keymap": "user:Blender",
            "when": { "windows": ["org.blender.Blender"], "excludeWindows": ["title:Preferences"], "keyboardLayouts": ["us"] } }
        ],
        "fallback": "user:Main"
      }
    },
    "commandsEnabled": false,
    "gameMode": {
      "control": "auto",
      "detectors": {
        "gamemoded": true, "fullscreen": false, "steam": true, "lutris": true,
        "heroic": true, "umu": true, "gamescope": true, "appCategory": false, "gameBar": false
      },
      "foregroundOnly": true,
      "processes": [
        { "id": "p1", "name": "factorio" },
        { "id": "p2", "name": "obs", "foregroundOnly": false, "exclude": true }
      ]
    },
    "linux": {
      "textMode": "libeiWithClipboardFallback",
      "pasteShortcut": "Shift+Insert",
      "terminalPasteShortcut": "Ctrl+Shift+KeyV",
      "ydotoolPath": "/usr/local/bin/ydotool",
      "xdotoolPath": "/usr/local/bin/xdotool"
    }
  }
}
```

| Field | Values | Default |
| --- | --- | --- |
| `experimental` | show in-development features in the UI (game mode, auto keymaps); core behaviour unchanged | `false` |
| `appearance` | `system`, `light`, `dark`, `eInk` | `system` |
| `highContrast`, `reduceMotion` | `system` (XDG Settings portal; KDE animation speed), `on`, `off` | `system` |
| `keymapOsd`, `copyOsd` | on-screen notice when the active keymap changes / when Tecloid emits a copy command | `false` |
| `layerHint` | overlay with the held layer's keys | `false` |
| `locale` | `auto`, `en`, `ru`, `de`, `fr`, `es`, `it`, `pt`, `pl`, `tr`, `uk`, `cs`, `hu`, `ro`, `el`, `hr`, `ka`, `kk`, `zh-Hans`, `zh-Hant`, `ja`, `ko` | `auto` |
| `devices.keyboards`, `devices.mice` | device paths; on Linux stable `/dev/input/by-id/…` or `by-path/…` links. At most one of each in this version (a longer list is a load error) | empty |
| `timing.tapDecision` | `permissiveHold` or `holdOnOtherKeyPress` (modifier-on-hold rules without their own value behave as the latter) | `permissiveHold` |
| `keymaps.mode` | `manual` (`manualKeymap`) or `auto` (first matching `auto.rules[]`, else `auto.fallback`) | `manual` |
| `keymaps.order` | library order; numbers for `activateKeymapN`; unlisted keymaps follow | empty |
| `keymaps.auto.rules[]` | `keymap` (required: id or `null` = native input), `enabled` (default `true`), `when` | — |
| `keymaps.auto.fallback` | keymap id or `null` | `null` |
| `commandsEnabled` | allow `cmd:` actions | `false` |
| `gameMode.control` | `auto`, `on`, `off` | `auto` |
| `gameMode.detectors` | per-source auto detection; each OS uses only its own | OS-dependent |
| `gameMode.foregroundOnly` | launcher/Game Bar games count only while focused | `true` |
| `gameMode.processes[]` | `name` pattern, `foregroundOnly` (default `true`), `exclude` | — |
| `linux.textMode` | `libeiWithClipboardFallback`, `libeiOnly`, `keycode`, `clipboard`, `ydotool`, `xdotool` | `libeiWithClipboardFallback` |
| `linux.pasteShortcut` | `Shift+Insert`, `Ctrl+KeyV`, `Ctrl+Shift+KeyV` (modifier order irrelevant) | `Shift+Insert` |
| `linux.terminalPasteShortcut` | same values or `sameAsPaste` | `Ctrl+Shift+KeyV` |
| `linux.ydotoolPath`, `xdotoolPath` | program path; missing = search `PATH`; empty string is an error | — |

Not persisted: open editor, current page, selected layer.

## Keymap file

```json
{
  "version": 1,
  "meta": { "name": "Main", "author": "Ivan K", "keymapVersion": "1.0", "description": "Main keymap" },
  "layers": [
    { "id": "nav", "name": "Navigation", "extraKeyOrder": ["F13"],
      "keys": { "KeyQ": null, "KeyH": "ArrowLeft", "F13": "builtin:copyLineText" } },
    { "id": "desktop",
      "keys": { "Tab": { "action": "Alt+Tab", "holdModifiers": true },
                "CapsLock": { "action": "Alt+Shift+Tab", "holdModifiers": true } } }
  ],
  "rules": [
    { "id": "caps", "key": "CapsLock", "tap": "Escape", "hold": { "layer": "nav", "timeoutMs": 180 } },
    { "id": "alt", "key": "AltLeft", "tap": "Enter", "hold": { "layer": "desktop" } },
    { "id": "meta", "key": "MetaLeft", "tap": "Escape", "hold": { "action": "ControlLeft" },
      "tapDecision": "holdOnOtherKeyPress" },
    { "id": "shift", "key": "ShiftLeft", "tap": "Ctrl+KeyC",
      "doubleTap": { "action": "Ctrl+KeyX", "timeoutMs": 250 } },
    { "id": "chord", "key": ["ShiftLeft", "ControlRight"],
      "longHold": { "action": "sys:turnOffScreen", "timeoutMs": 1500 } },
    { "id": "games", "key": "Space", "enabled": false, "when": { "gameMode": "off" }, "tap": null }
  ],
  "macros": [
    { "id": "copyLine", "name": "Copy line", "stepPauseMs": 10, "steps": ["Home", "Shift+End", "Ctrl+KeyC"] }
  ],
  "commands": [
    { "id": "music", "name": "Play/pause", "sh": "playerctl play-pause", "workingDirectory": "/tmp" }
  ],
  "quickMenuPages": [
    { "id": "system", "name": "System",
      "cells": { "KeyQ": { "name": "Lock", "action": "kde:ksmserver/Lock Session" }, "KeyW": "sys:showDesktop" } }
  ],
  "emojiMenuPages": [
    { "id": "faces", "name": "Faces", "cells": { "KeyQ": "😀" } }
  ]
}
```

- **`meta`** (optional strings `name`, `author`, `keymapVersion`, `description`):
  author data, kept on edit/copy/rename; not an id and not used for updates.
- **Layers:** `id` required, `name` defaults to `id` (an equal name is not
  written; same for macros and commands). In `keys`, `null` swallows the key and
  a missing key is transparent (lower active layer, then base rules). A key is a
  string or `{ "action", "holdModifiers"? }`. `holdModifiers: true` keeps the
  chord's modifiers pressed until the layer trigger is released (Alt+Tab window
  switching). `extraKeyOrder` is editor display order only.
- **Rules:** `key` is a physical key code string, or an array of ≥ 2 codes for a
  chord pressed within `timing.chordMs`. `+` is not allowed inside `key`.
  Generic `Shift`/`Ctrl`/`Alt`/`Meta` match either side; physical codes take
  priority over overlapping generic ones. Several rules may share a key: the
  first enabled one whose `when` matches wins.

  | Field | Meaning |
  | --- | --- |
  | `enabled` | `false` keeps the rule inactive |
  | `tap` | action on tap; missing = the key itself, `null` = nothing |
  | `doubleTap` | `{ "action", "timeoutMs"? }` |
  | `hold` | `layer` (active while held), `action` (held key/chord; missing = the key itself unless `layer` is set, `null` = nothing), `timeoutMs`, `retroTapMs` (late tap for a single-key layer trigger without `action`; missing = 2 × timeout, `0` = off) |
  | `longHold` | `{ "action", "timeoutMs"? }`; not combinable with `hold` |
  | `tapDecision` | overrides `timing.tapDecision` |

- **Macros:** `steps` are action strings including `pause:<ms>`; `stepPauseMs`
  and `modifierDelayMs` override `settings.timing` (missing inherits the global
  value, not the caller's). No cycles; expansion ≤ 1000 entries, nesting ≤ 10.
- **Commands:** `sh` (POSIX sh, Linux/macOS, run with `sh -lc`) and
  `powershell` (Windows); a command without a script for the running OS is
  skipped with a notice. `workingDirectory` defaults to home; `~/…` and
  relative paths resolve from home. Run only with `settings.commandsEnabled`.
- **Menu pages:** cell keys are the grid `KeyQ…KeyT / KeyA…KeyG / KeyZ…KeyB`.
  A quick cell is an action string or `{ "name"?, "action" }`; an emoji cell is
  literal text. Emoji cells, macro steps and rule keys have no object form yet;
  one will be added the same way as for layer keys when needed.
