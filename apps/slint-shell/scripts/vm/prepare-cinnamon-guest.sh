#!/bin/sh
# Run as root in a disposable Linux Mint Cinnamon guest, never on the host.
# Usage: prepare-cinnamon-guest.sh <user> "<ssh public key>"
set -eu
user=${1:-tester}
key=${2:-}
id "$user"
if ! command -v cinnamon >/dev/null 2>&1; then
    echo 'Install Linux Mint Cinnamon first (cinnamon must be on PATH).' >&2
    exit 1
fi
export DEBIAN_FRONTEND=noninteractive
apt-get update
# xdotool/x11-utils/x11-xserver-utils are what the .deb recommends; zenity is
# the receiver window; python3-gi and xclip help diagnostics.
apt-get install -y openssh-server python3 python3-gi zenity xdotool x11-utils \
    x11-xserver-utils x11-xkb-utils xclip desktop-file-utils fonts-noto-color-emoji

guest_home=$(getent passwd "$user" | cut -d: -f6)
group=$(id -gn "$user")
if [ -n "$key" ]; then
    install -d -m 700 -o "$user" -g "$group" "$guest_home/.ssh"
    printf '%s\n' "$key" >> "$guest_home/.ssh/authorized_keys"
    chown "$user:$group" "$guest_home/.ssh/authorized_keys"
    chmod 600 "$guest_home/.ssh/authorized_keys"
fi
printf '%s ALL=(ALL) NOPASSWD:ALL\n' "$user" > /etc/sudoers.d/90-tecloid-e2e
chmod 440 /etc/sudoers.d/90-tecloid-e2e
# Use only the shared key for SSH; unlock the local account for LightDM autologin.
passwd -d "$user"
printf 'PasswordAuthentication no\n' > /etc/ssh/sshd_config.d/90-tecloid-e2e.conf
systemctl enable ssh

# LightDM autologin into the Cinnamon X11 session (not "cinnamon-wayland").
mkdir -p /etc/lightdm/lightdm.conf.d
cat > /etc/lightdm/lightdm.conf.d/90-tecloid-e2e.conf <<CONFIG
[Seat:*]
autologin-user=$user
autologin-user-timeout=0
autologin-session=cinnamon
user-session=cinnamon
CONFIG
groupadd -f autologin
gpasswd -a "$user" autologin
systemctl set-default graphical.target
# No input membership in the frozen base: access comes from uaccess ACLs.
gpasswd -d "$user" input 2>/dev/null || true

# First-login dialogs that would steal focus from the receiver window.
install -d -o "$user" -g "$group" "$guest_home/.linuxmint/mintwelcome"
: > "$guest_home/.linuxmint/mintwelcome/norun.flag"
chown "$user:$group" "$guest_home/.linuxmint/mintwelcome/norun.flag"
install -d -o "$user" -g "$group" "$guest_home/.config/autostart"
for entry in mintwelcome mintupdate mintreport; do
    printf '[Desktop Entry]\nType=Application\nName=%s\nHidden=true\n' "$entry" \
        > "$guest_home/.config/autostart/$entry.desktop"
done
chown -R "$user:$group" "$guest_home/.config/autostart"

runuser -u "$user" -- dbus-run-session sh -c '
    gsettings set org.cinnamon.desktop.wm.preferences num-workspaces 4
    gsettings set org.gnome.libgnomekbd.keyboard layouts "[\"us\", \"ru\"]" || true
    gsettings set org.cinnamon.desktop.session idle-delay 0
    gsettings set org.cinnamon.desktop.screensaver lock-enabled false
    gsettings set org.cinnamon.settings-daemon.plugins.power sleep-display-ac 0
    gsettings set org.cinnamon.settings-daemon.plugins.power sleep-inactive-ac-timeout 0
'
echo "Prepared $user: reboot, verify autologin into Cinnamon (X11), then shut down and freeze the base image."
