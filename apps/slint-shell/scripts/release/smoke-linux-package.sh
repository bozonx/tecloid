#!/bin/sh
# Install a release package in a disposable container as root, check its files,
# libraries and non-graphical commands, then remove it.
# Usage: smoke-linux-package.sh <file.deb|file.rpm|file.tar.gz>
set -eu

package=$(realpath -- "$1")
fixtures=$(dirname "$(realpath -- "$0")")/../../../../crates/tecloid-core/fixtures
app_id=dev.bozonx.tecloid
if [ "$(id -u)" -ne 0 ]; then
    echo 'Run this check as root inside a disposable container.' >&2
    exit 1
fi

check_binary() {
    if ldd "$1" | grep 'not found'; then
        echo "Unresolved libraries in $1" >&2
        exit 1
    fi
    "$1" --version
    "$1" --help >/dev/null
}

# AppStream 0.x (Ubuntu 22.04) predates tags the metainfo uses, such as <developer>.
validate_metainfo() {
    if appstreamcli validate --no-net "$1"; then return; fi
    case "$(appstreamcli --version)" in
        *' 0.'*) echo 'Ignoring findings of an AppStream 0.x validator.' ;;
        *) exit 1 ;;
    esac
}

case "$package" in
    *.deb)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq "$package" desktop-file-utils appstream >/dev/null
        ;;
    *.rpm)
        if command -v dnf >/dev/null 2>&1; then
            dnf install -y -q "$package" desktop-file-utils appstream
        else
            zypper --non-interactive install --allow-unsigned-rpm "$package" desktop-file-utils AppStream
        fi
        ;;
    *.tar.gz)
        pacman -Sy --noconfirm --needed fontconfig freetype2 libxkbcommon wayland \
            desktop-file-utils appstream >/dev/null
        work=$(mktemp -d)
        tar -xzf "$package" -C "$work"
        prefix="$work/prefix"
        TECLOID_INSTALL_PREFIX=$prefix sh "$work"/tecloid-*/install-linux.sh
        check_binary "$prefix/bin/tecloid"
        desktop-file-validate "$prefix/share/applications/$app_id.desktop"
        validate_metainfo "$prefix/share/metainfo/$app_id.metainfo.xml"
        TECLOID_INSTALL_PREFIX=$prefix sh "$prefix/lib/$app_id/install-linux.sh" uninstall
        test ! -e "$prefix/bin/tecloid"
        echo "Smoke check passed: $package"
        exit 0
        ;;
    *) echo "Unknown package type: $package" >&2; exit 1 ;;
esac

check_binary /usr/bin/tecloid
for path in \
    /usr/share/applications/$app_id.desktop \
    /usr/share/metainfo/$app_id.metainfo.xml \
    /usr/share/icons/hicolor/scalable/apps/$app_id.svg \
    /usr/share/icons/hicolor/512x512/apps/$app_id.png \
    /usr/lib/udev/rules.d/60-tecloid.rules \
    /usr/lib/modules-load.d/tecloid.conf; do
    test -f "$path" || { echo "Missing $path" >&2; exit 1; }
done
desktop-file-validate /usr/share/applications/$app_id.desktop
validate_metainfo /usr/share/metainfo/$app_id.metainfo.xml

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
case "$package" in
    *.deb) dpkg-query -L tecloid > "$work/manifest" ;;
    *.rpm) rpm -ql tecloid > "$work/manifest" ;;
esac
# Package managers list shared directories alongside package files.
while IFS= read -r path; do
    if [ -L "$path" ] || [ -f "$path" ]; then
        printf '%s\n' "$path"
    fi
done < "$work/manifest" > "$work/files"

test_user=tecloid-smoke
useradd -m "$test_user"
trap 'userdel -r "$test_user"; rm -rf -- "$work"' EXIT
test_home=$(getent passwd "$test_user" | cut -d: -f6)
config="$test_home/.config/$app_id/config.json"
keymap="$test_home/.local/share/$app_id/keymaps/Smoke.json"
mkdir -p "$(dirname "$config")" "$(dirname "$keymap")"
cp "$fixtures/config.v1.defaults.linux.json" "$config"
cp "$fixtures/keymap.v1.json" "$keymap"
chown -R "$test_user" "$test_home"
sha256sum "$config" "$keymap" > "$work/user-data.sha256"

case "$package" in
    *.deb)
        apt-get remove -y -qq tecloid >/dev/null
        if [ "$(dpkg-query -W -f='${db:Status-Status}' tecloid 2>/dev/null || :)" = installed ]; then
            echo 'DEB package is still installed' >&2
            exit 1
        fi
        ;;
    *.rpm)
        if command -v dnf >/dev/null 2>&1; then dnf remove -y -q tecloid
        else zypper --non-interactive remove tecloid; fi
        if rpm -q tecloid >/dev/null; then
            echo 'RPM package is still installed' >&2
            exit 1
        fi
        ;;
esac
while IFS= read -r path; do
    if [ -e "$path" ] || [ -L "$path" ]; then
        echo "Not removed: $path" >&2
        exit 1
    fi
done < "$work/files"
sha256sum -c "$work/user-data.sha256"
echo "Smoke check passed: $package"
