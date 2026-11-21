# Changelog

All notable changes to tecloid are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
New entries go under `[Unreleased]`; `cargo release` turns it into a dated version section.

## [Unreleased]

- Updated Rust dependencies within the workspace version constraints.

- Settings buttons release keyboard focus after pointer use. Tab and Shift+Tab reach the top navigation from keymap editors and leave drop-downs without saving a provisional value. Escape consistently cancels pending transfers and closes the device picker after clearing its search. After a click, Tab continues from the clicked control instead of the top of the window, and the first Enter or Space shows the keyboard focus without pressing the button again. Scrolling with the mouse wheel or touchpad no longer resets the keyboard position.

- Mouse input and button assignments are available with In development features off. Previously selected mice and saved button assignments work in normal mode. On Linux, keyboard rules can also emit mouse clicks without capturing a physical mouse.
- Normal mode hides experimental Quick action cells and skips game-mode rules. Experimental assignments and invalid experimental data no longer intercept keys or block mapper startup. Standard system shortcuts are available in the action picker's sidebar.

- Colour emoji are shown on Fedora and other systems whose Noto Color Emoji is the COLRv1 build: the app bundles its own copy of Noto Color Emoji (SIL OFL 1.1) and no longer requires the system emoji font package.

- System theme follows desktop color scheme updates automatically, including when the scheme first becomes available after startup.

- Manual Linux VM launcher accepts `--build` to build the shared Linux release packages before starting acceptance.

- Use “Rules” consistently for the rules tab and rule deletion; “Trigger” remains the key or combination field.

- Trigger headers show an explicitly configured late-tap duration, including 0 when late tap is disabled.

- Portable standard shortcuts as `sys:` actions, so keymaps work on any OS and keyboard layout: `copy`, `cut`, `paste`, `undo`, `redo`, `selectAll`, `find`, `save`, `print`, `newTab`, `closeTab`, `reopenClosedTab`, `nextTab`, `previousTab`, `zoomIn`, `zoomOut`, `zoomReset`, `toggleComment` (Ctrl+/ in code editors; on a Cyrillic layout the `/` key of the Latin layout), `moveWordLeft`/`Right`, `selectWordLeft`/`Right`, `deleteWordBackward`/`Forward`, `moveToLineStart`/`End`, `moveToDocumentStart`/`End` and `switchWindow`. Editing commands pick their key from the active layout (Dvorak, Colemak, QWERTZ), use Ctrl+Shift variants in terminals (undo, redo, save, print, reopen closed tab and toggle comment do nothing there, since Ctrl+Z, Ctrl+S, Ctrl+P and Ctrl+/ control the terminal; tabs switch with Ctrl+PageDown/PageUp, on Windows Ctrl+Tab) and the platform's redo (`Ctrl+Y` on Windows). On Windows, terminals (Windows Terminal, console windows) copy with Ctrl+Insert, so copying without a selection does not interrupt the program. Zoom in uses Ctrl++ on layouts without an unshifted `=` (QWERTZ). The shortcuts honour the modifier delay and leave modifiers held by hold rules down; they are available without the in-development setting. They replace `builtin:copy`, `paste`, `cut`, `undo`, `redo`, `selectAll`, `find`, `save`, `newTab` and `closeTab`, which are removed; existing keymaps must update these ids manually.
- New desktop action `sys:maximizeWindowHorizontally` (KDE, Windows; GNOME where Mutter exposes partial maximization), the counterpart of `maximizeWindowVertically`.
- New desktop actions `sys:tileWindowLeft`/`Right`/`Top`/`Bottom`, `sys:activateTaskManagerEntry1`–`10` and `sys:showClipboardHistory` (KDE, Windows; side tiling and task entries 1–9 on GNOME).
- Built-in macros that copy, cut, paste or move by words use these shortcuts, so they follow the layout and terminal windows.
- New `sys:selectToLineStart` and `sys:selectToLineEnd` (Shift+Home/End). Built-in macros move and select within a line through `sys:moveToLineStart`/`End` and these actions instead of raw Home/End keys; the keys sent on Linux and Windows are unchanged, and the macros are ready for macOS chords (Cmd+Arrow).
- Linux paste setting: new `auto` (default: Ctrl+V, Ctrl+Shift+V in terminals) and `Paste` (the Paste key) values; `terminalPasteShortcut` defaults to `sameAsPaste`. The setting is shown for every typing method and also selects the keys of `sys:paste`; values other than `auto` are sent exactly as chosen, and copy and cut do not depend on it. A separate terminal shortcut is behind a checkbox. Configs that store explicit values keep them.
- The bundled LHC1 keymap uses these portable actions instead of Ctrl/Meta chords, maps `sys:redo` to Z on the select layer and uses `sys:maximizeWindowHorizontally` instead of a KDE shortcut and `sys:toggleComment` instead of `Ctrl+Slash`.

- Mapper: a layer key whose tap runs a shortcut (CapsLock: tap `Ctrl+KeyV`, hold a layer) stays the layer when it is released just before a key the layer maps, if both were held together for at least 50 ms. A quick CapsLock+J or CapsLock+Period no longer pastes and types the key itself ("ю" on a Russian layout). Typing taps (Space, Tab, `text:`) and short typing rolls are unchanged.
