#!/bin/sh
# Apply the bundled udev rule and load uinput; group membership stays the
# administrator's decision.
set -e
modprobe uinput 2>/dev/null || true
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules || true
    udevadm trigger --subsystem-match=misc --sysname-match=uinput || true
    udevadm trigger --subsystem-match=input --action=change || true
    udevadm settle || true
fi
echo 'Tecloid: input access follows the active local session.'
