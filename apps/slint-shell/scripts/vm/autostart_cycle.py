"""Session startup checks in disposable Linux guests (OS registration is authoritative)."""

import subprocess

from vmlib import StepFailed, wait_for


ENTRY = '~/.config/autostart/dev.bozonx.tecloid.desktop'


def reboot(vm, guest):
    # A changed boot ID prevents an old SSH/session response from passing the test.
    vm.ssh_multiplex = False
    before = vm.ssh('cat /proc/sys/kernel/random/boot_id').strip()
    vm.ssh('sudo systemd-run --quiet --on-active=2 --timer-property=AccuracySec=100ms '
           '--unit=tecloid-autostart-reboot systemctl reboot')

    def ready():
        try:
            return vm.ssh('cat /proc/sys/kernel/random/boot_id', timeout=10).strip() != before
        except (StepFailed, subprocess.SubprocessError, OSError):
            return False

    wait_for('new boot', 180, ready, interval=2)
    guest('wait-session --timeout 180')


def disabled_cycle(vm, guest, install=None):
    guest('quit-app')
    # Model disabling in desktop settings; retain the file to test Hidden semantics.
    vm.ssh(f"cp {ENTRY} ~/tecloid-e2e/autostart-enabled.desktop && "
           f"printf '\\nHidden=true\\n' >> {ENTRY}")
    reboot(vm, guest)
    guest('wait-app --absent --timeout 90')
    guest('mapper --absent --timeout 1')
    # A manual start must observe the OS setting instead of re-enabling startup.
    guest('start-app')
    guest('quit-app')
    if install is not None:
        install()
    reboot(vm, guest)
    guest('wait-app --absent --timeout 90')
    vm.ssh(f'cp ~/tecloid-e2e/autostart-enabled.desktop {ENTRY}')
    reboot(vm, guest)
    guest('wait-app --timeout 90')
    guest('autostart')
    guest('tray')
    guest('mapper --timeout 30')
    return 'disabled across login and manual launch; restored startup, tray and mapper'


def upgraded_cycle(vm, guest, install):
    install()
    guest('autostart')
    guest('quit-app')
    reboot(vm, guest)
    guest('wait-app --timeout 90')
    guest('tray')
    guest('mapper --timeout 30')
    return 'registration and session startup survived package replacement'
