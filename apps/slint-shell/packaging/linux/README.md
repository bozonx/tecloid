# Tecloid for Linux

Keyboard mapper for KDE Plasma 6 and GNOME on Wayland. Before extracting, download
`SHA256SUMS` from the same release page and verify the archive with
`sha256sum -c --ignore-missing SHA256SUMS`. Changes are listed in `CHANGELOG.md`.

## Install

```sh
./install-linux.sh                 # into ~/.local, no sudo
./install-linux.sh permissions     # once: udev rule and uinput module (asks for sudo)
```

Permissions take effect in the active local session without a new login. The binary lives at
`~/.local/lib/dev.bozonx.tecloid/tecloid`, the command `~/.local/bin/tecloid`
points to it. Set `TECLOID_INSTALL_PREFIX` to use another absolute prefix.

The rule grants every active local user access to keyboards, mice and touchpads.
It does not prevent keylogging by processes in that session. For systems without
logind or use outside an active local session, explicitly add the user to `input`
and log in again. Existing group memberships are retained. Literal text (`text:` actions) needs `xdg-desktop-portal` with
the desktop backend (`xdg-desktop-portal-kde` on KDE).

GNOME 50: run `./install-gnome-extension.sh`, log in again,
then `gnome-extensions enable tecloid@bozonx.dev`. The tray needs an
AppIndicator extension.

## Update and remove

Quit Tecloid, then run the new archive's `install-linux.sh`; settings are kept.

```sh
~/.local/lib/dev.bozonx.tecloid/install-linux.sh remove-permissions   # optional
~/.local/lib/dev.bozonx.tecloid/install-linux.sh uninstall
```

Uninstalling keeps your configuration and keymaps.

Documentation: https://bozonx.github.io/tecloid. Source: https://github.com/bozonx/tecloid.
License: GPL-3.0-or-later.

Removing the rule does not immediately revoke ACLs already granted in the
current session. End the local session or reboot after removing permissions.
Other system rules may independently grant access to some devices.
