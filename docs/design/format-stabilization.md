# Format stabilization proposals (before the first release with users)

Status: **proposal**, not implemented. Every item here touches the on-disk
format and needs the maintainer's approval (see AGENTS.md, "Ask first").
Once a file with these values is released, the meaning below is frozen.

Guiding rule: **the reader accepts the widest form we can ever want; the UI
offers a narrow, convenient subset.** Widening a reader later is compatible,
narrowing it makes released files unloadable.

## 1. Numbered slot actions

Affected: `app:activateKeymapN`, `app:showQuickMenuN`, `app:showEmojiMenuN`,
`sys:switchToDesktopN`, `sys:moveWindowToDesktopN`,
`sys:switchToKeyboardLayoutN`.

Today the limits differ: menus accept any `N`, `activateKeymap` stops at 5
(`KEYMAP_SLOT_COUNT`), `sys:*N` at 10 (`SYSTEM_NUMBERED_COUNT`).

Proposal:

- **Reader:** every numbered family accepts any `N` in `1..=999` (decimal,
  no sign, no leading zero, as `slot_number` already enforces). One rule for
  all families. The upper bound only guards against absurd values and is
  part of the contract; it may be raised, never lowered.
- **Runtime:** a slot that resolves to nothing (no 7th keymap in
  `keymaps.order`, no 12th menu page, no 11th desktop) does nothing and
  produces a diagnostic; it is never a load error.
- **Action picker:** shows a fixed default range, e.g. keymaps `1..=5`,
  menu pages `1..=10`, desktops/layouts `1..=10`, extended to
  `max(default, actual count)` (more keymaps in `order` or more menu pages
  than the default → their slots appear). The currently bound value is always
  shown even if it is outside the range, so editing a rule never silently
  drops it.
- **Contract:** `schemas/names.v1.txt` records families as patterns
  (`action app:activateKeymap<N>`) instead of the enumerated `1..10`, and the
  test checks the pattern plus the bound. Concrete names stay as examples.
- Semantics stay positional (documented in `actions.rs`): `N` is the
  position in `keymaps.order` / the menu page list / the OS list. Reordering
  changes what a key does; this is intended and is frozen.

## 2. Library keymap ids

Today: `user:<file name without .json>`; referenced from `manualKeymap`,
`keymaps.order[]`, `keymaps.auto.rules[].keymap`, `keymaps.auto.fallback`.

Proposal: **keep the file name as the id; do not put an id inside the keymap
file.**

Why not an id inside the file: a keymap file is portable. Files downloaded
from the internet, copied between machines or duplicated by the user would
carry the same id, so the app would have to detect collisions and rewrite
ids anyway, i.e. modify files the user owns. The file name is already unique
within the library directory and is the identity the user sees.

Rules to freeze:

- The id is exactly the file stem; the app never normalizes it (no
  lowercasing, trimming, transliteration) when it writes references.
- Uniqueness **ignoring case**, same as other ids, so a library copied
  between Linux and case-insensitive file systems (Windows, macOS) keeps
  meaning. Two files differing only in case → diagnostic, the second is not
  loaded.
- **Import / download:** the file is copied into the library under its own
  name; on a clash the app picks `Name (2)`, `Name (3)`, … The file content is
  not changed.
- **Rename in the app:** renames the file and rewrites every reference in
  `config.json` (already implemented).
- **Rename outside the app:** references become dangling → existing
  "missing keymap" diagnostic, `auto.fallback` / first keymap in `order` is
  used, the settings UI offers to re-point. Never a load error, never an
  automatic rewrite of `config.json` on load.
- Reserved for the future: other prefixes (e.g. `bundled:`) for keymaps not
  in the user library. `user:` stays the only prefix in v1; the reader already
  rejects others, adding one later is compatible.

## 3. `devices.keyboards` / `devices.mice`

Today: arrays, but at most one element (`DevicesDoc::single`).

Proposal: keep arrays and **freeze the meaning of several elements now**,
even though v1 still accepts only one:

- The list is the **set of devices Tecloid grabs**. The active keymap applies
  to all of them identically, as if they were one keyboard (shared modifier,
  layer and tap-hold state). Order has no meaning; duplicates are load errors.
- An empty list means "no device chosen" (current behaviour), not "all".
- Lifting the one-element limit later is a widening, no version bump.
- **Per-device keymaps**, if ever needed, are a separate optional field, for
  example a condition `when.devices: [...]` in auto rules or rules. They do not
  change the meaning of `devices.keyboards`.
- Each element stays a device path; on Linux a stable `/dev/input/by-id/…`
  or `by-path/…` link. An object form (`{ "path": …, "name": … }`) next to the
  string form may be added later without a version bump.

## Not changed, documented as frozen

- `kde:` / `xdg:` carry external names; their syntax check must not be
  tightened.
- `linux.pasteShortcut` / `terminalPasteShortcut` enum values share spelling
  with action strings (`Ctrl+KeyV`); changing canonical key spelling would
  break these enum values.
- `meta.keymapVersion` is a free string, never parsed as semver.
- Numeric ranges (`timeoutMs`, `retroTapMs`, `longHoldMs`, macro timing) may
  only be widened.
