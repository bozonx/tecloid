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
`config.json` = 1, keymap = 1.** The contract below applies from release
0.5.0 on: user files in these versions must keep loading in every later
build, including 1.0. Files of 0.4.0 and earlier (`schemaVersion`,
`layouts/`) are a different, unsupported format. Features shown only in
development mode (game mode, automatic keymaps, `when.gameMode`, `xdg:`) are
part of version 1 under the same rules.

## Compatibility contract

- Files are UTF-8 JSON; a leading byte order mark is accepted and not
  written back.
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
- `config.json` is written in full, defaults included, so changing a default
  affects only new installations and settings added later. Only fields whose
  absence has its own meaning are left out (`manualKeymap` before a choice,
  `ydotoolPath`/`xdotoolPath` = search `PATH`). In keymaps, a missing rule or
  macro timing means "inherit `settings.timing`", and is therefore not
  written. Input-affecting defaults still change only as a user-visible
  change in `CHANGELOG.md`.
- Every value has one spelling. Key and shortcut actions, layer key codes
  (`keyh` → `KeyH`) and conditions (see `when`) are stored canonically; a
  layer key listed twice in two spellings is a load error. Ids, prefixes and
  keymap references are taken verbatim, without trimming; optional text
  fields (`name`, `description`, `meta.*`, `sh`, `powershell`,
  `workingDirectory`) are missing rather than empty, an empty or blank string
  is an error; numbers in ids have no sign or leading zero (`switchToDesktop3`,
  not `switchToDesktop03`). `null` is accepted only where it means something
  (`tap`, `hold.action`, a layer key, `keymap` of an auto rule,
  `auto.fallback`); elsewhere "none" is a missing field.
- **Stored names are frozen.** Action prefixes, `sys:`, `app:` and
  `builtin:` ids, key names, pattern prefixes, device ids with a scheme
  (`windows:keyboard`) and the keymap id prefix `user:` are listed in [`schemas/names.v1.txt`](../schemas/names.v1.txt)
  (test `stored_names_are_never_removed`); field names, JSON types, enum
  values, required fields and numeric limits of both formats in
  [`schemas/contract.v1.json`](../schemas/contract.v1.json) (test
  `frozen_fields_and_enum_values_remain`: names, types and values only grow,
  required lists only shrink, limits, `minLength`/`maxLength`,
  `minItems`/`maxItems` only widen, an id `pattern` changes only after
  review); size limits and bounds outside the schemas in
  [`schemas/limits.v1.txt`](../schemas/limits.v1.txt) (test
  `limits_only_widen`); how values outside the schemas are read (action
  strings, window patterns, keyboard layout ids, `kde:`/`xdg:` ids, device
  ids, keymap names, ids, trigger keys, slot numbers) as input/result
  examples in [`schemas/grammar.v1.txt`](../schemas/grammar.v1.txt) (test
  `value_grammars_keep_their_meaning`). These lists only grow: a missing
  line fails the test. New entries are added with `TECLOID_UPDATE_SCHEMAS=1`;
  action prefixes come from `actions::PREFIXES`, the table `Action::parse`
  uses.
- The steps of every `builtin:` macro are a reviewed snapshot in
  [`schemas/builtin-macros.v1.txt`](../schemas/builtin-macros.v1.txt) (test
  `builtin_macro_steps_are_reviewed`). The id is frozen, the steps are what
  it does today: fixing them or changing them for a platform is allowed as a
  deliberate behaviour change with a `CHANGELOG.md` entry.
- **Limits** ([`config/limits.rs`](../crates/tecloid-core/src/config/limits.rs)):
  any JSON array or object holds at most 10 000 entries, any string (key or
  value) at most 65 536 UTF-8 bytes; at most 999 quick and 999 emoji menu
  pages (one `app:` slot each). A file over a limit is a load error.
- A JSON object key written twice (in `layers[].keys`, menu `cells`, or a
  field name) is a load error rather than "the last one wins".
- The application never writes a file it would refuse to load: every save
  reads the new text back first and fails instead of writing it.
- Fixtures `crates/tecloid-core/fixtures/{config,keymap}.v1.json` use every
  field of version 1. They are never edited; `version_1_fixtures_keep_loading`
  checks that they load without blocking diagnostics, and
  `version_1_fixtures_save_unchanged` that they save exactly as
  `*.v1.saved.json`. `config.v1.defaults.{linux,windows,macos}.json` are
  what a new installation writes on each system, all checked on every build
  host (`config_defaults_are_frozen`): a diff there is a change of
  defaults and needs a `CHANGELOG.md` entry.
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

Only entities something refers to have an `id`: layers (`hold.layer`),
macros (`macro:<id>`) and commands (`cmd:<id>`). It is unique within its
collection ignoring ASCII case (`Nav` and `nav` are duplicates), while
references must spell it exactly; grammar `[A-Za-z0-9_-]{1,64}`, so ids
appear unescaped in actions. Duplicates are load errors. Rules, menu pages
(addressed by position), auto rules and game-mode processes have no `id`
field. The editor names new layers after their name (`Navigation` → `navigation`,
`layer` for non-Latin names), new macros the same way (`Copy line` →
`copy-line`, `macro` for non-Latin names) and new commands `c1`, `c2`, ….

Library keymap ids are `user:<file name without .json>`. The name is 1–128
characters (at most 250 UTF-8 bytes), no control characters or
`\ / : * ? " < > |`, no leading or trailing blank, does not start or end with
`.`. Other `*.json` files in `keymaps/` are not keymaps: they are ignored
with a warning, never listed or referenced. The id is exactly the file
stem and is never normalized; keymap files carry no id of their own, so
downloaded or copied files never collide. Ids are unique ignoring case: of
library files whose names differ only in case, the first in sort order is
loaded and the others are ignored with a warning. Importing a file whose
name is taken stores it as `Name (2)`, `Name (3)`, … without changing its
content. Renaming a keymap in the app updates every reference in
`config.json`; renaming a file outside the app leaves dangling references,
reported as a missing keymap (the fallback is used) and never rewritten on
load. `user:` is the only prefix in version 1; others may be added later.

### Conditions (`when`)

Used by keymap rules and auto rules (`settings.keymaps.auto.rules`). All given
conditions must match; an empty object always matches.

| Field | Meaning |
| --- | --- |
| `gameMode` | `on` / `off` |
| `keyboardLayouts` | short system layout ids; any matches. The id is what the OS reports: the XKB layout name on Linux (`us`, `de`; the variant is not compared, so `us` also matches US Dvorak), the language subtag on Windows (`en`, `ru`). A keymap using this condition is therefore tied to the kind of system it was made on. Only plain ids `[a-z0-9_-]+` (any case) are compared; other spellings such as `us(dvorak)` or `lang:en` are reserved for later forms (layout variant, platform-neutral language) and match nothing |
| `windows` | active-window patterns; any matches |
| `excludeWindows` | patterns that block the condition; if the active window is unknown, the condition fails |

List items are stored trimmed and without duplicates, keyboard layouts in
lowercase and the title prefix as `title:` (`" US "` → `us`,
`Title: Foo` → `title:Foo`); window patterns keep their case.

A window pattern is an app id or a full process name, case-insensitive
(`org.telegram.desktop`, `Game.exe`; `.exe` optional), with `*`/`?` wildcards,
or `title:<text>` (title contains the text, case-insensitive; `*` and `?` are ordinary characters there). Any other prefix of two or more
lowercase letters followed by `:` (`class:`, `exe:`) is reserved for later
pattern kinds: such a pattern matches nothing and diagnostics report it.
Implemented in `config::app_match`.

The same program has different ids on different systems and install
methods: a distribution package, a Flatpak and Windows each report their
own (`org.telegram.desktop`, `Telegram.exe`). Because any pattern of the
list matches, a keymap meant to be shared lists every id it should cover in
one condition: `"windows": ["org.telegram.desktop", "Telegram.exe"]`. Ids
that never occur on the running system are harmless and simply do not match.

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
| `pause:<ms>` macro step (digits without a leading zero) | 1–10000 |
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
| Tecloid's own action | `app:showQuickMenu1`, `app:toggleGameMode` |
| KDE global shortcut | `kde:kwin/Window Maximize` |
| freedesktop application action (Linux) | `xdg:firefox/new-private-window` |
| literal text | `text:Hello` |
| pause (macro steps only) | `pause:200` |

Where an action is optional, a missing field means native behaviour of the
physical key and `null` means "do nothing". An empty string is an error. An
unknown `word:…` prefix (e.g. from a newer build) loads and saves unchanged, but
diagnostics report it and the mapper does not start with it.

- **Shortcuts** (a key with modifiers in an action): modifiers then key,
  joined by `+`. In an action `Ctrl`, `Alt`, `Shift`, `Meta` mean the
  **left** modifier; in a rule trigger (`rules[].key`) they mean **either**
  side. Sided codes (`ControlRight`) are explicit. Any case, order and
  `ControlLeft`-style spelling is accepted and stored canonically on load:
  catalog key names, modifiers in the order `Ctrl`, `Alt`, `Shift`, `Meta`,
  left modifiers by their generic name (`shiftleft+ctrl+keyv` →
  `Ctrl+Shift+KeyV`). An action of modifiers only names the same keys in any
  order, so they are sorted and the last one is written as a physical code:
  `Ctrl` → `ControlLeft`, `Shift+ControlLeft` → `Ctrl+ShiftLeft`. Blanks
  around the parts of a shortcut are dropped (`Ctrl + KeyC`). "Chord" means
  only several trigger keys pressed together (`timing.chordMs`, array
  `key`); an action with modifiers is a shortcut.
- **Key names** (`config/key_catalog.rs`) are the W3C UI Events
  `KeyboardEvent.code` values (<https://www.w3.org/TR/uievents-code/>) wherever
  that spec has one: `KeyA`, `Digit1`, `ArrowLeft`, `ControlLeft`, `Backquote`.
  They name the physical key position, not the character it types in the
  current keyboard layout. Keys the spec does not cover use Chromium's
  `DomCode` name (`BrightnessUp`, `MailSend`), and failing that a name
  derived from the Linux `KEY_*` code. Derived names follow the spec's
  PascalCase, acronyms written as words like the spec's `IntlRo`: `Wlan`,
  `Wwan`, `Uwb`, `PlayCd`, `EjectCloseCd`, `VideoPrevious`. Of the spec's
  alternatives the catalog uses the primary name (`Lang3`, not the legacy
  `Katakana`). Mouse buttons: `MouseLeft`,
  `MouseRight`, `MouseMiddle`, `MouseBack` (`BTN_SIDE`), `MouseForward`
  (`BTN_EXTRA`) — the side buttons of ordinary mice — and `Mouse6`, `Mouse7`,
  `Mouse8` for the further buttons (`BTN_FORWARD`, `BTN_BACK`, `BTN_TASK` in
  that order; despite their kernel names they are not the usual back/forward).
  `ScrollUp` and `ScrollDown` are keyboard keys (`KEY_SCROLLUP`,
  `KEY_SCROLLDOWN`), not the mouse wheel. `WheelUp`, `WheelDown`, `WheelLeft`
  and `WheelRight` are reserved for the wheel and never name a key.
- **Built-in macros** (`mapper/builtin_macros.rs`) start with a verb: `goTo…`,
  `select…`, `copy…`, `cut…`, `paste…`, `replace…WithClipboard`, `insert…`,
  `duplicate…`. `builtin:` and `macro:` namespaces are independent.
  `builtin:copy`, `cut`, `paste`, `undo`, `redo` (`Ctrl+Shift+Z`),
  `selectAll`, `find`, `save`, `newTab` and `closeTab` are standard
  application commands, for keymaps that should
  work on any keyboard layout. In every built-in macro the letter keys of
  `Ctrl+C/V/X/Z/A/F/S/T/W` steps are resolved at playback by the character they
  type in the active layout (on QWERTZ undo is the physical `KeyY`), unlike
  user actions, which always name physical keys. On Linux plain
  `Ctrl+C/V/X` also follow the terminal and paste settings.
- **Naming sources.** `sys:` ids act on the desktop and are named after KDE
  Plasma's actions; `app:` ids act on Tecloid itself and are Tecloid's own
  names; `builtin:` ids are Tecloid's own names for editing macros; key
  names follow W3C. Toggling is part of an id's meaning: in `app:` it is
  spelled out (`toggleGameMode`, `setGameModeOn`), in `sys:` it is
  whatever the Plasma action does, so a `sys:` id without `toggle` may still
  toggle.
- **`sys:`** ids follow the KDE Plasma names of the actions (Plasma is the
  reference platform), and so does their meaning: an action does what the
  Plasma action of that name does, toggling included (`maximizeWindow`,
  `keepWindowAbove`, `showDesktop`, `showOverview` toggle there), and other
  platforms reproduce it. The ids: `closeWindow`, `minimizeWindow`, `maximizeWindow`,
  `maximizeWindowVertically`, `keepWindowAbove`, `moveWindowToNextDesktop`,
  `moveWindowToPreviousDesktop`, `moveWindowToNextScreen`,
  `moveWindowToPreviousScreen`, `switchToNextDesktop`, `switchToPreviousDesktop`,
  `showDesktop`, `showOverview`, `showAppLauncher`, `turnOffScreen`,
  `lockSession`, and `switchToDesktopN`, `moveWindowToDesktopN`,
  `switchToKeyboardLayoutN` (see numbered slots below). Several map to system toggles. Platform
  support: [platforms.md](platforms.md).
- **`kde:<component>/<shortcut>`**: a kglobalaccel shortcut. The component
  is the last segment of its D-Bus object path (`org_kde_powerdevil`, not
  `org.kde.powerdevil`); the shortcut is its untranslated name.
- **`xdg:<desktop-id>/<action>`**: desktop-id is the `.desktop` file name without
  extension (`kde4/foo.desktop` → `kde4-foo`), action is from `Actions=`.
  `DBusActivatable=true` apps get `org.freedesktop.Application.ActivateAction`,
  others run the action's `Exec`.
- **`app:`** (`config/app_action.rs`): `showQuickMenuN`, `showEmojiMenuN`
  (past the end opens the last page), `showQuickMenuRecent`,
  `showEmojiMenuRecent`, `showSettings`, `activateKeymapN` (by
  `settings.keymaps.order`), `toggleGameMode`, `setGameModeAuto|On|Off`.

**Numbered slots.** Every numbered family (`switchToDesktopN`,
`moveWindowToDesktopN`, `switchToKeyboardLayoutN`, `showQuickMenuN`,
`showEmojiMenuN`, `activateKeymapN`) accepts any `N` in `1…999`
(`MAX_SLOT`; may be raised, never lowered). `N` is a position — in the OS
list, the menu page list or `keymaps.order` — on purpose: reordering changes
what a key does and does not rewrite actions. A slot that resolves to nothing
at run time does nothing (with a notice), it is never a load error. The
action picker offers only the first slots (desktops and layouts 1…10, menu
pages 1…10, keymaps 1…5), more when the keymap or library has more, and
always the value already bound. `schemas/names.v1.txt` records each family
as `action sys:switchToDesktop<1..=999>`.

## config.json

Example with every field (the application writes all of them except
`manualKeymap` before a keymap is chosen and the tool paths):

```json
{
  "version": 1,
  "settings": {
    "experimental": false,
    "launchAtLogin": false,
    "startMapperOnLaunch": false,
    "interface": {
      "appearance": "system",
      "highContrast": "system",
      "reduceMotion": "system",
      "locale": "auto",
      "keymapOsd": false,
      "layerHint": false,
      "copyOsd": false
    },
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
          { "keymap": null, "enabled": true, "when": { "gameMode": "on" } },
          { "keymap": "user:Blender", "enabled": true,
            "when": { "windows": ["org.blender.Blender"], "excludeWindows": ["title:Preferences"], "keyboardLayouts": ["us"] } }
        ],
        "fallback": "user:Main"
      },
      "commandsAllowed": ["user:Main"]
    },
    "gameMode": {
      "control": "auto",
      "detectors": {
        "gamemoded": true, "fullscreen": false, "steam": true, "lutris": true,
        "heroic": true, "umu": true, "gamescope": true, "appCategory": false, "gameBar": false
      },
      "foregroundOnly": true,
      "processes": [
        { "name": "factorio", "foregroundOnly": true, "exclude": false },
        { "name": "obs", "foregroundOnly": false, "exclude": true }
      ]
    },
    "linux": {
      "textMode": "libeiWithClipboardFallback",
      "pasteShortcut": "Ctrl+KeyV",
      "terminalPasteShortcut": "Shift+Insert",
      "ydotoolPath": "/usr/local/bin/ydotool",
      "xdotoolPath": "/usr/local/bin/xdotool"
    }
  }
}
```

| Field | Values | Default |
| --- | --- | --- |
| `experimental` | enable experimental UI and runtime behaviour in every build; see [runtime policy](development.md#experimental-features-and-debug-isolation). With `false`, settings of experimental features are kept but have no effect (e.g. `keymaps.mode: auto` acts as `manual`). When a feature stops being experimental its saved settings start to apply with the flag off: a behaviour change listed in `CHANGELOG.md` | `false` |
| `interface.appearance` | `system`, `light`, `dark`, `eInk` | `system` |
| `interface.highContrast`, `interface.reduceMotion` | `system` (XDG Settings portal; KDE animation speed), `on`, `off` | `system` |
| `interface.keymapOsd` | on-screen notice when the active keymap changes | `false` |
| `interface.layerHint` | overlay with the held layer's keys | `false` |
| `interface.copyOsd` | on-screen notice when Tecloid sends a copy shortcut (shown on Linux) | `false` |
| `interface.locale` | `auto`, `en`, `ru`, `de`, `fr`, `es`, `it`, `pt`, `pl`, `tr`, `uk`, `cs`, `hu`, `ro`, `el`, `hr`, `ka`, `kk`, `zh-Hans`, `zh-Hant`, `ja`, `ko` | `auto` |
| `devices.keyboards`, `devices.mice` | device ids, see [Device ids](#device-ids). The list is the set of devices Tecloid grabs; the active keymap applies to all of them as to one keyboard (shared modifier, layer and tap-hold state), order has no meaning, an empty list means none is chosen. At most one of each in this version (a longer list is a load error); allowing more is a compatible widening. Per-device keymaps, if added, will be a separate field | empty |
| `timing.tapDecision` | `permissiveHold` or `holdOnOtherKeyPress` | `permissiveHold` |
| `keymaps.mode` | `manual` (`manualKeymap`) or `auto` (first matching `auto.rules[]`, else `auto.fallback`) | `manual` |
| `keymaps.manualKeymap`, `keymaps.order[]`, `keymaps.auto.rules[].keymap`, `keymaps.auto.fallback` | keymap ids `user:<name>` with a valid library name; any other string is a load error, a missing keymap is a diagnostic | — |
| `keymaps.order` | library order; numbers for `activateKeymapN`; unlisted keymaps follow; each id at most once | empty |
| `keymaps.auto.rules[]` | `keymap` (required: id or `null` = native input), `enabled` (default `true`), `when` (missing = always) | — |
| `keymaps.auto.fallback` | keymap id or `null` | `null` |
| `keymaps.commandsAllowed` | keymap ids whose `cmd:` actions may run; every other keymap's commands are skipped. Trust belongs to this machine, not to the keymap file, so an imported or copied keymap starts without it. Renaming a keymap in the app keeps its entry, deleting it removes the entry; each id at most once | empty |
| `gameMode.control` | `auto`, `on`, `off` | `auto` |
| `gameMode.detectors` | per-source auto detection; each OS uses only its own | OS-dependent |
| `gameMode.foregroundOnly` | launcher/Game Bar games count only while focused | `true` |
| `gameMode.processes[]` | `name` pattern (not blank, no `title:`: a process has no title), `foregroundOnly` (default `true`), `exclude` (default `false`) | — |
| `linux.textMode` | `libeiWithClipboardFallback`, `libeiOnly`, `portalKeycode`, `clipboardPaste`, `ydotool`, `xdotool` | `libeiWithClipboardFallback` |
| `linux.pasteShortcut` | `Shift+Insert`, `Ctrl+KeyV`, `Ctrl+Shift+KeyV` (modifier order irrelevant) | `Ctrl+KeyV` |
| `linux.terminalPasteShortcut` | same values or `sameAsPaste` | `Shift+Insert` |
| `linux.ydotoolPath`, `xdotoolPath` | program path; missing = search `PATH`; empty string is an error | — |

Not persisted: open editor, current page, selected layer.

### Sections and platform-specific settings

Settings are grouped by what they are about (`interface`, `devices`,
`timing`, `keymaps`, `gameMode`), not by operating system. An option that
only some systems use still lives in its topic section, is written on every
system and is ignored where it does not apply (`interface.copyOsd`,
`gameMode.detectors.gameBar`). A platform section (`linux`; later `windows`,
`macos`) holds only the choice of an OS mechanism that has no counterpart
elsewhere: how text is injected and which programs do it. New settings follow
the same rule.

### Device ids

An entry of `devices.keyboards` / `devices.mice` is one of:

- an absolute path starting with `/`: on Linux a stable
  `/dev/input/by-id/…` or `by-path/…` link;
- `<scheme>:<value>`, the scheme being two or more lowercase letters, for
  devices that are not files. Version 1 defines `windows:keyboard` and
  `windows:mouse` (every keyboard / mouse of the session; Windows has no
  per-device interception). An unknown scheme loads and names no device.

Anything else (a relative path, blanks around the value) is a load error.
Ids are taken verbatim and compared as written; on Linux two paths that
resolve to the same event device are the same device. The known scheme ids
are listed in `schemas/names.v1.txt` as `device …`.

## Keymap file

```json
{
  "version": 1,
  "meta": { "name": "Main", "author": "Ivan K", "keymapVersion": "1.0", "description": "Main keymap" },
  "layers": [
    { "id": "nav", "name": "Navigation", "description": "Arrows", "extraKeyOrder": ["F13"],
      "keys": { "KeyQ": null, "KeyH": "ArrowLeft", "F13": "builtin:copyLineText" } },
    { "id": "desktop",
      "keys": { "Tab": { "action": "Alt+Tab", "holdModifiers": true },
                "CapsLock": { "action": "Alt+Shift+Tab", "holdModifiers": true } } }
  ],
  "rules": [
    { "key": "CapsLock", "tap": "Escape", "hold": { "layer": "nav", "timeoutMs": 180 } },
    { "key": "AltLeft", "tap": "Enter", "hold": { "layer": "desktop" } },
    { "key": "MetaLeft", "tap": "Escape", "hold": { "action": "ControlLeft" },
      "tapDecision": "holdOnOtherKeyPress" },
    { "key": "ShiftLeft", "tap": "Ctrl+KeyC",
      "doubleTap": { "action": "Ctrl+KeyX", "timeoutMs": 250 } },
    { "key": ["ShiftLeft", "ControlRight"],
      "longHold": { "action": "sys:turnOffScreen", "timeoutMs": 1500 } },
    { "key": "Space", "enabled": false, "when": { "gameMode": "off" }, "tap": null }
  ],
  "macros": [
    { "id": "copyLine", "name": "Copy line", "stepPauseMs": 10, "steps": ["Home", "Shift+End", "Ctrl+KeyC"] }
  ],
  "commands": [
    { "id": "music", "name": "Play/pause", "sh": "playerctl play-pause", "workingDirectory": "/tmp" }
  ],
  "quickMenuPages": [
    { "name": "System",
      "cells": { "KeyQ": { "name": "Lock", "action": "kde:ksmserver/Lock Session" }, "KeyW": "sys:showDesktop" } }
  ],
  "emojiMenuPages": [
    { "name": "Faces", "cells": { "KeyQ": "😀" } }
  ]
}
```

- **`meta`** (optional strings `name`, `author`, `keymapVersion`, `description`):
  author data, kept on edit/copy/rename; not an id and not used for updates.
- **Layers:** `id` required, `name` defaults to `id` (an equal name is not
  written; same for macros and commands), optional `description` shown in
  the editor. In `keys`, `null` swallows the key and
  a missing key is transparent (lower active layer, then base rules). A key is a
  string or `{ "action", "holdModifiers"? }`. `holdModifiers: true` keeps the
  chord's modifiers pressed until the layer trigger is released (Alt+Tab window
  switching). `extraKeyOrder` is editor display order only.
- **Rules:** `key` is a physical key code string, or an array of ≥ 2 distinct
  codes for a chord pressed within `timing.chordMs`. `+` is not allowed inside
  `key`. A `hold` object must not be empty.
  Generic `Shift`/`Ctrl`/`Alt`/`Meta` match either side; physical codes take
  priority over overlapping generic ones. Several rules may share a key: the
  first enabled one whose `when` matches wins.

  | Field | Meaning |
  | --- | --- |
  | `enabled` | `false` keeps the rule inactive |
  | `tap` | action on tap; missing = the key itself, `null` = nothing |
  | `doubleTap` | `{ "action", "timeoutMs"? }` |
  | `hold` | `layer` (active while held), `action` (held key/chord; missing = the key itself unless `layer` is set, `null` = nothing), `timeoutMs`, `retroTapMs` (late tap for a single-key layer trigger without `action`; only together with `layer`; missing = 2 × timeout, `0` = off) |
  | `longHold` | `{ "action", "timeoutMs"? }`; not combinable with `hold` |
  | `tapDecision` | overrides `timing.tapDecision`. Missing: `timing.tapDecision`, except for a single modifier key whose hold is the key itself or another plain modifier (`MetaLeft` held as `ControlLeft`), which uses `holdOnOtherKeyPress` so fast rolls such as Meta+C give Ctrl+C. This is part of the meaning of the field in version 1 |

- **Macros:** `steps` are action strings including `pause:<ms>`; `stepPauseMs`
  and `modifierDelayMs` override `settings.timing` (missing inherits the global
  value, not the caller's). No cycles; expansion ≤ 1000 entries, nesting ≤ 10.
- **Commands:** `sh` (POSIX sh, Linux/macOS, run with `sh -lc`) and
  `powershell` (Windows); a command without a script for the running OS is
  skipped with a notice. `workingDirectory` defaults to home; `~/…` and
  relative paths resolve from home. Run only when the keymap is listed in `settings.keymaps.commandsAllowed`.
- **Menu pages:** cell keys are the grid `KeyQ…KeyT / KeyA…KeyG / KeyZ…KeyB`,
  in any case on load and canonical on save; an empty page is written as `{}`.
  A quick cell is an action string or `{ "name"?, "action" }`; an emoji cell is
  literal text inserted as is (an emoji or a snippet, spaces kept). It is
  empty only when it is all spaces, tabs or line breaks; a no-break space is
  valid text. Emoji cells, macro steps and rule keys have no object form yet;
  one will be added the same way as for layer keys when needed.
