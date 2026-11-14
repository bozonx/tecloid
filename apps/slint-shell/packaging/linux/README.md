# Tecloid for Linux

Keyboard mapper for KDE Plasma 6 on Wayland. Verify the archive with
`sha256sum -c --ignore-missing SHA256SUMS` before extracting it.

## Install

```sh
./install-linux.sh                 # into ~/.local, no sudo
./install-linux.sh permissions     # once: udev rule, uinput module, group "input" (asks for sudo)
```

Log out and back in after `permissions`. The binary lives at
`~/.local/lib/dev.bozonx.tecloid/tecloid`, the command `~/.local/bin/tecloid`
points to it. Set `TECLOID_INSTALL_PREFIX` to use another absolute prefix.

The group `input` lets your processes read every keyboard and mouse; grant it
deliberately. Literal text (`text:` actions) needs `xdg-desktop-portal` with
the desktop backend (`xdg-desktop-portal-kde` on KDE).

GNOME 50 (experimental): run `./install-gnome-extension.sh`, log in again,
then `gnome-extensions enable tecloid@bozonx.dev`. The tray needs an
AppIndicator extension.

## Update and remove

Quit Tecloid, then run the new archive's `install-linux.sh`; settings are kept.

```sh
~/.local/lib/dev.bozonx.tecloid/install-linux.sh remove-permissions   # optional
~/.local/lib/dev.bozonx.tecloid/install-linux.sh uninstall
```

Uninstalling keeps your configuration and keymaps.

Documentation: https://github.com/bozonx/tecloid. License: GPL-3.0-or-later.
