"""Windows session startup acceptance; runs only inside a disposable VM overlay.

Uses the OS Run registration, like disabling startup outside Tecloid in Windows.
This does not exercise the settings checkbox. With a previous ZIP, replaces its
portable executable in place and checks startup with the current release.
"""

import json
import subprocess
import time

from vmlib import StepFailed, wait_for
from windows_mapper import WindowsMapper


APP_ID = 'dev.bozonx.tecloid'
RUN = r'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
APPROVED = r'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'


def cycle(vm, ps, work, previous=None):
    # Release startup must use the guest's session directories; this VM is disposable.
    # Complete setup so showing settings at first run is not mistaken for a bug.
    config = json.dumps({"version": 1, "settings": {
        "devices": {"keyboards": ["windows:keyboard"]},
        "keymaps": {"manualKeymap": "user:Autostart"},
        "startMapperOnLaunch": True, "interface": {"locale": "en"}}})
    keymap = json.dumps({"version": 1, "rules": [{"key": "KeyA", "tap": "KeyB"}]})
    ps("$ErrorActionPreference='Stop'; $dir=Join-Path $env:APPDATA 'dev.bozonx.tecloid'; "
       "New-Item -ItemType Directory -Force (Join-Path $dir 'keymaps') | Out-Null; "
       "$encoding=[Text.UTF8Encoding]::new($false); "
       f"[IO.File]::WriteAllText((Join-Path $dir 'config.json'), '{config}', $encoding); "
       f"[IO.File]::WriteAllText((Join-Path $dir 'keymaps\\Autostart.json'), '{keymap}', $encoding)")
    current = work + r'\src\target\release\tecloid.exe'
    executable = current
    previous_version = None
    if previous is not None:
        previous = previous.replace("'", "''")
        executable = work + r'\autostart-install\tecloid.exe'
        previous_version = ps(
            f"$ErrorActionPreference='Stop'; $stage='{work}\\autostart-previous'; "
            "Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue; "
            f"Expand-Archive '{previous}' -DestinationPath $stage; "
            "$files=@(Get-ChildItem $stage -Recurse -Filter tecloid.exe -File); "
            "if ($files.Count -ne 1) { throw 'Previous ZIP needs exactly one tecloid.exe' }; "
            f"New-Item -ItemType Directory -Force '{work}\\autostart-install' | Out-Null; "
            f"Copy-Item $files[0].FullName '{executable}' -Force; & '{executable}' --version").strip()
        current_version = ps(f"& '{current}' --version").strip()
        if previous_version == current_version:
            raise StepFailed('Previous and current versions must differ for startup upgrade coverage')
    ps(f"$ErrorActionPreference='Stop'; if (!(Test-Path '{executable}')) {{ throw 'Release shell missing' }}; "
       "if (Test-Path Env:TECLOID_DEV_DIR) { throw 'Startup requires non-isolated release build' }; "
       f"New-Item -Force '{RUN}' | Out-Null; "
       f"Remove-ItemProperty '{APPROVED}' -Name '{APP_ID}' -ErrorAction SilentlyContinue; "
       f"Set-ItemProperty '{RUN}' -Name '{APP_ID}' -Value '\"{executable}\"'")

    def boot_id():
        return ps('(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToFileTimeUtc()', timeout=20).strip()

    def reboot():
        before = boot_id()
        ps("shutdown.exe /r /t 3; if ($LASTEXITCODE) { throw 'Windows restart request failed' }")

        def ready():
            try:
                return boot_id() != before
            except (StepFailed, subprocess.SubprocessError, OSError):
                return False

        # Pending Windows updates can keep shutdown/boot busy for several minutes.
        wait_for('Windows reboot (including OS updates)', 900, ready, interval=3)

        def desktop_ready():
            try:
                return 'True' in ps('[bool](Get-Process explorer -ErrorAction SilentlyContinue)', timeout=20)
            except (StepFailed, subprocess.SubprocessError, OSError):
                return False

        wait_for('interactive desktop after reboot', 900, desktop_ready, interval=3)

    def running():
        return 'True' in ps("[bool](Get-Process tecloid -ErrorAction SilentlyContinue)")

    def enabled():
        reboot()
        wait_for('autostarted release shell', 120, running)
        # IPC distinguishes a usable shell from a hung process.
        result = ps(f"& '{executable}' ping; if ($LASTEXITCODE) {{ throw 'Startup IPC failed' }}", timeout=30)
        time.sleep(2)
        ps("if (Get-Process tecloid -ErrorAction SilentlyContinue | "
           "Where-Object { $_.MainWindowHandle -ne 0 }) { throw 'Unexpected startup window' }")
        # Run the existing native receiver in the autologon user's desktop.
        receiver = work + r'\out\receiver'
        script = work + r'\src\apps\slint-shell\scripts\windows\windows-receiver.ps1'
        ps("$ErrorActionPreference='Stop'; "
           "Stop-ScheduledTask -TaskName Tecloid-Autostart-Receiver -ErrorAction SilentlyContinue; "
           f"Remove-Item '{receiver}' -Recurse -Force -ErrorAction SilentlyContinue; "
           "$principal=(Get-ScheduledTask -TaskName Tecloid-E2E).Principal; "
           f"$action=New-ScheduledTaskAction -Execute powershell -Argument '-NoProfile -STA -ExecutionPolicy Bypass -File {script} -Output {receiver} -InputLocale en-US'; "
           "Register-ScheduledTask -TaskName Tecloid-Autostart-Receiver -Action $action "
           "-Principal $principal -Force | Out-Null; "
           "Start-ScheduledTask -TaskName Tecloid-Autostart-Receiver")
        try:
            wait_for('startup mapper input receiver', 60, lambda: 'True' in ps(
                f"Test-Path '{receiver}\\snapshot.json'", timeout=20))
            probe = WindowsMapper(vm, ps, None)
            probe.receiver("tray")
            probe.reset()
            vm.qmp.tap('a')
            probe.text('b')
        finally:
            ps('Stop-ScheduledTask -TaskName Tecloid-Autostart-Receiver')
        return result.strip()

    enabled()
    if previous is not None:
        ps(f"& '{executable}' quit")
        wait_for('previous shell to exit', 30, lambda: not running())
        registration = ps(f"(Get-ItemProperty '{RUN}').'{APP_ID}'").strip()
        ps(f"$ErrorActionPreference='Stop'; Copy-Item '{current}' '{executable}' -Force")
        if ps(f"(Get-ItemProperty '{RUN}').'{APP_ID}'").strip() != registration:
            raise StepFailed('Portable executable replacement changed startup registration')
        enabled()
    ps(f"$ErrorActionPreference='Stop'; Remove-ItemProperty '{RUN}' -Name '{APP_ID}'; "
       f"& '{executable}' quit")
    reboot()
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        if running():
            raise StepFailed('Tecloid launched with Windows startup disabled')
        time.sleep(2)
    # Re-enable and verify a second login, catching stale StartupApproved state.
    ps(f"Set-ItemProperty '{RUN}' -Name '{APP_ID}' -Value '\"{executable}\"'")
    enabled()
    ps(f"& '{executable}' quit")
    return ('Run entry: enabled login, disabled login (120s), re-enabled login, IPC, tray and QEMU A -> B mapper input'
            + (f'; upgraded from {previous_version}' if previous is not None else '; version upgrade NOT RUN'))
