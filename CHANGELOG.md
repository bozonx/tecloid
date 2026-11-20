# Changelog

All notable changes to tecloid are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
New entries go under `[Unreleased]`; `cargo release` turns it into a dated version section.

## [Unreleased]

- Portable standard shortcuts as `sys:` actions, so keymaps work on any OS and keyboard layout: `copy`, `cut`, `paste`, `undo`, `redo`, `selectAll`, `find`, `save`, `print`, `newTab`, `closeTab`, `reopenClosedTab`, `nextTab`, `previousTab`, `zoomIn`, `zoomOut`, `zoomReset`, `moveWordLeft`/`Right`, `selectWordLeft`/`Right`, `deleteWordBackward`/`Forward`, `moveToLineStart`/`End`, `moveToDocumentStart`/`End` and `switchWindow`. Editing commands pick their key from the active layout (Dvorak, Colemak, QWERTZ), use Ctrl+Shift variants in terminals (undo, redo, save, print and reopen closed tab do nothing there, since Ctrl+Z, Ctrl+S and Ctrl+P control the terminal; tabs switch with Ctrl+PageDown/PageUp, on Windows Ctrl+Tab) and the platform's redo (`Ctrl+Y` on Windows). On Windows, terminals (Windows Terminal, console windows) copy with Ctrl+Insert, so copying without a selection does not interrupt the program. Zoom in uses Ctrl++ on layouts without an unshifted `=` (QWERTZ). The shortcuts honour the modifier delay and leave modifiers held by hold rules down; they are available without the in-development setting. They replace `builtin:copy`, `paste`, `cut`, `undo`, `redo`, `selectAll`, `find`, `save`, `newTab` and `closeTab`, which are removed; existing keymaps must update these ids manually.
- New desktop actions `sys:tileWindowLeft`/`Right`/`Top`/`Bottom`, `sys:activateTaskManagerEntry1`–`10` and `sys:showClipboardHistory` (KDE, Windows; side tiling and task entries 1–9 on GNOME).
- Built-in macros that copy, cut, paste or move by words use these shortcuts, so they follow the layout and terminal windows.
- Linux paste setting: new `auto` (default: Ctrl+V, Ctrl+Shift+V in terminals) and `Paste` (the Paste key) values; `terminalPasteShortcut` defaults to `sameAsPaste`. The setting is shown for every typing method and also selects the keys of `sys:paste`; values other than `auto` are sent exactly as chosen, and copy and cut do not depend on it. A separate terminal shortcut is behind a checkbox. Configs that store explicit values keep them.
- The bundled LHC1 keymap uses these portable actions instead of Ctrl/Meta chords, and maps `sys:redo` to Z on the select layer.

- Mapper: a layer key whose tap runs a shortcut (CapsLock: tap `Ctrl+KeyV`, hold a layer) stays the layer when it is released just before a key the layer maps, if both were held together for at least 50 ms. A quick CapsLock+J or CapsLock+Period no longer pastes and types the key itself ("ю" on a Russian layout). Typing taps (Space, Tab, `text:`) and short typing rolls are unchanged.
