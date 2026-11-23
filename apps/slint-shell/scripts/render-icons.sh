#!/bin/sh
# Render the application and tray PNGs with rsvg-convert (librsvg).
set -eu

shell_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
svg="$shell_dir/assets/icon.svg"
icons="$shell_dir/packaging/linux/icons"
mkdir -p -- "$icons"
for size in 16 22 24 32 48 64 128 256 512; do
    rsvg-convert -w "$size" -h "$size" -o "$icons/$size.png" -- "$svg"
done
# Both tray states are embedded in the binary.
for name in tray tray-paused; do
    rsvg-convert -w 24 -h 24 -o "$shell_dir/assets/$name.png" -- "$shell_dir/assets/$name.svg"
done
