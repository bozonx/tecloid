# Typing any character without the clipboard via an XKB option (research)

Status: **research, not implemented** (2026-10-14).

## Problem

On KDE Wayland text is typed through libei (`text_input/portal/`). libei sends
key codes; KWin turns them into characters through the current XKB layout. A
character missing from the layout (`→`, `é`, `😀`) cannot be typed, so
`libeiWithClipboardFallback` pastes the whole text through the clipboard.

Alternatives do not work on KDE: `zwp_virtual_keyboard_v1` is not offered to
normal clients; an input method would replace the user's fcitx5/IBus; AT-SPI
`InsertText` misses Chromium/Electron and terminals; the portal's
`NotifyKeyboardKeysym` also looks the keysym up in the current layout.

## Idea

Generate an XKB option `tecloid:extra` in `~/.config/xkb/` that places the needed
characters on key codes absent from real keyboards, and enable it in KWin. The
existing libei code then types them like any other key. System files and the
user's layouts stay untouched. One option works in every layout because it fills
only group 1 and XKB wraps missing groups to it (`GroupsWrap`).

libxkbcommon looks in `$XDG_CONFIG_HOME/xkb` before `/usr/share/X11/xkb`.

`~/.config/xkb/rules/evdev` (the include is mandatory, otherwise system rules are
replaced):

```
! include %S/evdev

! option = symbols
  tecloid:extra = +tecloid(extra)
```

`~/.config/xkb/symbols/tecloid` (two levels: plain and Shift):

```
default partial alphanumeric_keys
xkb_symbols "extra" {
    key <I154> { [ rightarrow, leftarrow ] };   // → ←
    key <I168> { [ U1F600,     eacute    ] };   // 😀 é
};
```

`~/.config/kxkbrc`: `Options=…,tecloid:extra`.

**Keysym names must match what the code looks up:** `text_keysym()` in
`portal/keymap.rs` uses `xkb_utf32_to_keysym`, which returns legacy keysyms for
many characters (`→` → `rightarrow`, not `U2192`). The generator must write
`xkb_keysym_get_name(xkb_utf32_to_keysym(ch))`. Multi-codepoint emoji need every
codepoint in the table.

## Free key codes

F13–F24 are unusable (mic mute, touchpad toggles, user macros). Use codes defined
in `keycodes/evdev` without symbols in the compiled layout, computed at run time
(xkeyboard-config 2.48 with `ru,us,latam`: 14 codes ≤ 255 after excluding
Japanese/Brazilian keys, 19 above 255 — about 60 characters on two levels). Codes
above 255 do not work in XWayland, so frequent characters get low codes.

## Algorithm

1. Read layouts, variants and options from `kxkbrc`.
2. Collect characters from `text:` actions, macro steps, emoji and quick cells.
3. Keep characters missing from at least one configured layout.
4. Assign free codes, frequent characters first.
5. Write files to a temp dir and verify with
   `xkbcli compile-keymap --include <tmp> --include-defaults …`; abort on failure.
6. Atomically replace the files, add the option to `kxkbrc`.
7. Ask KWin to reload (`org.kde.keyboard.reloadConfig`, unverified) or ask the
   user to log in again. Regenerate when the characters change.

Rollback: remove the option and both files, reload.

## Code changes needed

`build_keymap_table()` in `portal/keymap.rs` compiles only the current layout and
variant without options and for group 0. It must compile the full KWin keymap
(all layouts, variants, options), search the active group with wrapping
(`group % xkb_keymap_num_layouts_for_key`) and drop its cache when `kxkbrc`
changes. `level_to_mods()` treats right Alt as AltGr, so only levels 1–2 are used.

## Verified

On Manjaro (libxkbcommon 1.13.2, xkeyboard-config 2.48) in a temp
`XDG_CONFIG_HOME`: the rules file and option compile; `<I168>` yields `😀`/`é`
in all three groups of `ru,us,latam`; a character written as `U2192` is not found
by `xkbcli how-to-type →`.

## Not verified

KWin picking up the user rules and option; libei typing in Qt, GTK, Firefox,
Chromium/Electron and XWayland; reload without relogin; whether KDE keyboard
settings drop the unknown option on save; exposing the option via `evdev.xml`.

## Risks

| Risk | Mitigation |
| --- | --- |
| Broken generated files make KWin fall back to `us` | compile check before writing; atomic replace |
| A device actually sends a "free" code | exclude Japanese/Brazilian keys; show used codes |
| User `rules/evdev` affects every libxkbcommon client | only the include plus one option line |
| KDE settings overwrite `Options` | check at startup and notify |
| More characters than free codes | frequent first; the rest via clipboard with a warning |

Changing the user's layout files and `kxkbrc` (and any new setting enabling this
mode) needs the owner's approval before implementation.
