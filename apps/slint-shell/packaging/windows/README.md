# Tecloid for Windows 11 x64 (portable)

Requires the [Microsoft Visual C++ v14 Redistributable (x64)](https://aka.ms/vc14/vc_redist.x64.exe).
Verify the ZIP with `Get-FileHash -Algorithm SHA256 <archive.zip>` against
`WINDOWS-SHA256SUMS`. The executable is not Authenticode-signed.

Extract the whole ZIP into a folder writable by your user and run `tecloid.exe`
without administrator rights. It starts hidden in the tray; open Settings from
the tray or with `tecloid.exe show settings`. Settings are stored in your user
profile, not next to the executable.

Update: quit (`tecloid.exe quit`), extract the new ZIP into a new folder and
start it once so "Launch on startup" points at it. Uninstall: turn off
"Launch on startup", quit and delete the folder. Configuration is kept.

Input to elevated windows and the secure desktop (UAC, lock screen) is not
intercepted.

Documentation: https://github.com/bozonx/tecloid. License: GPL-3.0-or-later.
