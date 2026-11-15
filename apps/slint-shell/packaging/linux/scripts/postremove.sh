#!/bin/sh
# Drop the removed udev rule; user configuration is left untouched.
set -e
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules || true
    udevadm trigger --subsystem-match=input --action=change || true
    udevadm trigger --subsystem-match=misc --sysname-match=uinput || true
    udevadm settle || true
fi
exit 0
