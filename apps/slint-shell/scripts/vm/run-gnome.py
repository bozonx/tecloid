#!/usr/bin/env python3
"""GNOME Wayland acceptance in a disposable VM; stop at the first failed step.

Usage: run-gnome.py --vm <prepared Ubuntu GNOME directory> --package <tar.gz> [--keep]
"""

import argparse
import ast
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

from manual_acceptance import MANUAL_CHECKS, manual_acceptance
from autostart_cycle import disabled_cycle, upgraded_cycle
from vmlib import Report, StepFailed, Vm, commit, environment_ok, receiver_text, result_path, verify_close_window, verify_popup_destination, verify_session_lock

GUEST = "~/tecloid-e2e"
PY = f"python3 {GUEST}/linux-guest.py"
APP_ID = "dev.bozonx.tecloid"
INSTALL = f"~/.local/lib/{APP_ID}"
UUID = "tecloid@bozonx.dev"
BRIDGE = "gdbus call --session --dest org.gnome.Shell --object-path /dev/bozonx/Tecloid/Gnome --method dev.bozonx.Tecloid.Gnome."


GNOME_MANUAL_CHECKS = tuple(
    (name, "Удаление: ~/.local/lib/dev.bozonx.tecloid/install-gnome-extension.sh uninstall, затем ~/.local/lib/dev.bozonx.tecloid/install-linux.sh uninstall. Настройки должны сохраниться.")
    if name == "removal" else (name, instruction)
    for name, instruction in MANUAL_CHECKS
)
GNOME_MANUAL_CHECKS = GNOME_MANUAL_CHECKS[:-1] + (
    ("gnome-bridge", "В GNOME проверь переключение раскладки/рабочего стола и возврат фокуса после Quick actions."),
    ("gnome-clipboard", "Задай содержимое буфера обмена, вставь через Tecloid ASCII и Привет 🙂 после разрешения портала. Исходный буфер должен сохраниться."),
) + GNOME_MANUAL_CHECKS[-1:]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vm", required=True)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--autostart-only", action="store_true", help="only startup, disable/re-enable and reinstall checks")
    parser.add_argument("--manual", action="store_true", help="prepare a visible GNOME session for guided operator checks")
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--examples", type=Path, help="directory with current editor and interactions binaries")
    args = parser.parse_args()
    if args.manual and (args.autostart_only or args.examples):
        parser.error("--manual excludes --autostart-only and --examples")
    if args.manual and not sys.stdin.isatty():
        parser.error("--manual needs an interactive terminal")
    environment_ok()
    package = args.package.resolve()
    if not package.is_file() or not package.name.endswith(".tar.gz"):
        parser.error("--package must be an existing release .tar.gz")
    if args.examples and any(not (args.examples / name).is_file() for name in ("editor", "interactions")):
        parser.error("--examples needs both editor and interactions binaries")
    vm = Vm(args.vm)
    report = Report()
    if args.manual:
        report.require("manual-" + name for name, _ in GNOME_MANUAL_CHECKS)
    state = {}

    def step(name, function, *arguments):
        return report.step(name, function, *arguments, fatal=True)

    def guest(command, **kwargs):
        return vm.ssh(f"{PY} {command}", **kwargs).strip()

    def popup_destination():
        def open_popup():
            before = int(guest("popup-count quick t4_focused"))
            vm.qmp.tap("e")
            guest(f"popup-wait quick t4_focused --after {before}")

        return verify_popup_destination(vm, guest, open_popup)

    def boot():
        vm.start(show=args.manual)
        vm.wait_ssh()
        vm.ssh(f"mkdir -p {GUEST}")
        vm.copy_to(Path(__file__).with_name("linux-guest.py"), "tecloid-e2e/")
        vm.copy_to(Path(__file__).with_name("text_corpus.py"), "tecloid-e2e/")
        guest("wait-session --timeout 180")
        info = vm.ssh("cat /etc/os-release; gnome-shell --version; "
                      "systemctl --user show-environment | grep -E 'XDG_CURRENT_DESKTOP|XDG_SESSION_TYPE'; "
                      "dpkg-query -W xdg-desktop-portal xdg-desktop-portal-gnome")
        (vm.logs / "environment.txt").write_text(info)
        return info.strip()

    def ui_example(name):
        vm.copy_to(args.examples / name, "tecloid-e2e/")
        arguments = " --smoke" if name == "editor" else ""
        home = vm.ssh('printf %s "$HOME"').strip()
        snapshots = f"{home}/tecloid-e2e/ui-snapshots/{name}"
        vm.ssh(f"mkdir -p {shlex.quote(snapshots)}")
        return vm.ssh(f"systemd-run --user --wait --pipe --collect --unit=tecloid-e2e-ui-{name} "
                      f"--setenv=TECLOID_EDITOR_SNAPSHOTS={shlex.quote(snapshots)} "
                      f"{GUEST}/{name}{arguments}", timeout=180).strip()

    def install():
        vm.copy_to(package, "tecloid-e2e/")
        vm.ssh(f"cd {GUEST} && mkdir -p bundle && tar -xzf {shlex.quote(package.name)} -C bundle && "
               "cd bundle/* && ./install-linux.sh && ./install-gnome-extension.sh")
        return guest("check-install")

    def keyboard():
        probe = vm.ssh(f"sudo {PY} probe-keyboard --timeout 20", background=True)
        time.sleep(2)
        for _ in range(4):
            vm.qmp.tap("shift")
            time.sleep(0.5)
        out, err = probe.communicate(timeout=25)
        if probe.returncode:
            raise StepFailed(err.strip())
        state["device"] = out.strip()
        guest(f"write-config --capture-tablet --device {shlex.quote(state['device'])}")
        # Startup registration is authoritative; a JSON flag alone does not enable it.
        vm.ssh(f"mkdir -p ~/.config/autostart && "
               f"cp ~/.local/share/applications/{APP_ID}.desktop ~/.config/autostart/{APP_ID}.desktop")
        return state["device"]

    def permissions():
        vm.ssh(f"{INSTALL}/install-linux.sh permissions")
        # Preserve Ubuntu's installed extensions; enable the bridge after a fresh login.
        script = ("from gi.repository import Gio, GLib; s=Gio.Settings.new('org.gnome.shell'); "
                  "s.set_boolean('disable-user-extensions',False); "
                  "v=list(s.get_strv('enabled-extensions')); "
                  f"s.set_strv('enabled-extensions',list(dict.fromkeys(v+['{UUID}','ubuntu-appindicators@ubuntu.com']))); "
                  f"s.set_strv('disabled-extensions',[x for x in s.get_strv('disabled-extensions') if x!='{UUID}']); "
                  "Gio.Settings.sync()")
        if args.manual:
            # Keep workspace targets stable while the operator moves windows.
            script += ("; Gio.Settings.new('org.gnome.mutter').set_boolean('dynamic-workspaces',False); "
                       "Gio.Settings.new('org.gnome.desktop.wm.preferences').set_int('num-workspaces',4); "
                       "Gio.Settings.new('org.gnome.desktop.input-sources').set_value('sources',"
                       "GLib.Variant('a(ss)', [('xkb','us'),('xkb','ru')])); "
                       "Gio.Settings.sync()")
        vm.ssh(f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus python3 -c {shlex.quote(script)}")
        vm.ssh("sudo systemd-run --quiet --on-active=2 --unit=tecloid-e2e-relogin "
               f"sh -c 'systemctl stop display-manager; loginctl terminate-user {shlex.quote(vm.user)}; sleep 3; systemctl start display-manager'")
        time.sleep(8)
        vm.wait_ssh()
        # An SSH login can succeed while terminate-user is still draining the old
        # session. Retry transport loss until the new graphical session is ready.
        session_deadline = time.monotonic() + 180
        while True:
            try:
                guest("wait-session --timeout 15")
                break
            except (StepFailed, subprocess.TimeoutExpired):
                if time.monotonic() >= session_deadline:
                    raise
                time.sleep(2)
        deadline = time.monotonic() + 30
        while True:
            try:
                return vm.ssh(f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus {BRIDGE}Snapshot false",
                              timeout=10).strip()
            except (StepFailed, subprocess.TimeoutExpired) as error:
                if time.monotonic() >= deadline:
                    raise StepFailed(f"GNOME bridge did not become available after login: {error}") from error
            time.sleep(0.5)

    def wait_for(description, predicate, timeout=30):
        deadline = time.monotonic() + timeout
        while not predicate():
            if time.monotonic() >= deadline:
                raise StepFailed(f"timed out waiting for {description}")
            time.sleep(0.3)

    def tray_recovery():
        def disable_host():
            vm.ssh("gnome-extensions disable ubuntu-appindicators@ubuntu.com")
            wait_for("tray host to disappear", lambda: "false" in vm.ssh(
                "busctl --user call org.freedesktop.DBus /org/freedesktop/DBus "
                "org.freedesktop.DBus NameHasOwner s org.kde.StatusNotifierWatcher"))

        def settings_focused():
            output = vm.ssh(f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus {BRIDGE}Snapshot true")
            window = json.loads(ast.literal_eval(output.strip())[0])["window"]
            return window and window["appId"] == "dev.bozonx.tecloid"

        def restore_host():
            pid = vm.ssh("pgrep -u $(id -u) -x tecloid").strip()
            vm.ssh("gnome-extensions enable ubuntu-appindicators@ubuntu.com")
            guest("tray")
            if vm.ssh("pgrep -u $(id -u) -x tecloid").strip() != pid:
                raise StepFailed("tray recovery restarted the application")

        vm.ssh("~/.local/bin/tecloid hide")
        disable_host()
        wait_for("settings fallback after tray host loss", settings_focused)
        vm.qmp.screendump(vm.logs / "tray-host-absent.png")
        restore_host()
        disable_host()
        guest("quit-app")
        guest("start-app --popups auto")
        # Leave the host unavailable: the settings fallback must work before
        # restoring it, rather than depending on a successful registration.
        time.sleep(3)
        wait_for("settings fallback at startup without a tray host", settings_focused)
        restore_host()
        return "host loss/reappearance and startup without host: settings reachable; same-process registration recovered"

    def expect(name, expected, keys=(), action=None, *, xwayland=False):
        guest(f"start-receiver --name {name}" + (" --xwayland" if xwayland else ""))
        if action:
            action()
        if keys:
            vm.qmp.tap(*keys)
        time.sleep(3)
        vm.qmp.screendump(vm.logs / f"{name}.png")
        vm.qmp.tap("ret")
        result = receiver_text(guest(f"receiver-result --name {name}"))
        if result != expected:
            raise StepFailed(f"expected {expected!r}, received {result!r}; see {name}.png and app.log")
        return repr(result)

    def commit_text():
        text = "Привет 👩🏽‍💻"

        def commit():
            bridge = f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus {BRIDGE}"
            snapshot = json.loads(ast.literal_eval(vm.ssh(bridge + "Snapshot true").strip())[0])
            target = snapshot["window"]["windowId"]
            vm.ssh(f"if {bridge}CommitText wrong stale; then exit 1; fi")
            vm.ssh(f"{bridge}CommitText {shlex.quote(text)} {shlex.quote(target)}")

        return expect("gnome-commit-text", text, action=commit)

    def clipboard_only():
        guest("quit-app")
        guest("set-text-backend clipboardPaste")
        guest("start-app --popups auto")
        guest("mapper")
        clipboard_seed()
        expect("clipboard-only-unicode", "Привет 👩🏽‍💻", ("i",))
        return clipboard_preserved()

    def commit_through_mapper():
        guest("quit-app")
        guest("set-text-backend gnomeCommitText")
        guest("start-app --popups auto")
        guest("mapper")
        expect("commit-mapper", "Привет 👩🏽‍💻", ("i",))

        def emoji():
            before = int(guest("popup-count emoji t4_focused"))
            vm.ssh("~/.local/bin/tecloid show emoji")
            guest(f"popup-wait emoji t4_focused --after {before}")
            vm.qmp.tap("q")

        expect("commit-emoji", "🙂", action=emoji)
        before = int(guest("text-failures --reason NoInputContext"))
        expect("commit-xwayland", "", ("t",), xwayland=True)
        if int(guest("text-failures --reason NoInputContext")) != before + 1:
            raise StepFailed("expected one GNOME CommitText error for XWayland")
        return "mapper and emoji popup delivered exact text; XWayland reported an error"

    def layer():
        def keys():
            vm.qmp.tap("f")
            vm.qmp.sequence(("f", True), 0.4, ("j", True), 0.05,
                            ("j", False), 0.05, ("f", False))
        return expect("layer", "gk", action=keys)

    def clipboard_seed():
        vm.ssh("systemctl --user stop tecloid-e2e-clipboard-owner.service", check=False)
        vm.copy_to(Path(__file__).with_name("clipboard-owner.py"), "tecloid-e2e/")
        vm.ssh(f"rm -f {GUEST}/clipboard-ready; systemd-run --user --collect --unit=tecloid-e2e-clipboard-owner python3 {GUEST}/clipboard-owner.py")
        deadline = time.monotonic() + 30
        while "ready" not in vm.ssh(f"test ! -f {GUEST}/clipboard-ready || echo ready"):
            if time.monotonic() >= deadline:
                raise StepFailed("clipboard fixture did not acquire focus and seed data")
            time.sleep(0.3)
        return clipboard_preserved()

    def clipboard_preserved():
        expected = {"text/plain;charset=utf-8": b"tecloid-before-unicode",
                    "application/x-tecloid-e2e": b"binary\x00\xffpayload"}
        import base64
        for mime, data in expected.items():
            output = vm.ssh("bash -o pipefail -c " + shlex.quote(
                f"wl-paste --no-newline --type {shlex.quote(mime)} | base64 -w0"))
            if base64.b64decode(output.strip()) != data:
                raise StepFailed(f"clipboard format {mime} was not preserved")
        return "original text and binary MIME data preserved"

    def clipboard_interruption(mode):
        vm.ssh(f"rm -f {GUEST}/clipboard-ready {GUEST}/clipboard-paste-received; "
               f"systemd-run --user --collect --unit=tecloid-e2e-clipboard-{mode} "
               f"python3 {GUEST}/clipboard-owner.py --paste {mode}")
        def marker(name):
            deadline = time.monotonic() + 15
            while "ready" not in vm.ssh(f"test ! -f {GUEST}/{name} || echo ready"):
                if time.monotonic() >= deadline:
                    raise StepFailed(f"clipboard fixture timed out: {name}")
                time.sleep(0.1)
        marker("clipboard-ready")
        time.sleep(3.5)
        vm.qmp.tap("y")
        marker("clipboard-paste-received")
        if mode == "hold":
            guest("toggle-mapper")
        expected = "tecloid-new-owner"
        deadline = time.monotonic() + 6
        while True:
            actual = vm.ssh("wl-paste --no-newline --type 'text/plain;charset=utf-8'", check=False).strip()
            if actual == expected:
                break
            if time.monotonic() >= deadline:
                raise StepFailed(f"{mode}: clipboard overwritten: {actual!r}")
            time.sleep(0.1)
        vm.qmp.screendump(vm.logs / f"clipboard-{mode}.png")
        return "new owner retained" if mode == "replace" else "pending paste cancelled on mapper stop; clipboard restored"

    def consent():
        wait_for("Tecloid to grab the configured tablet", lambda: guest("tablet-grabbed") == "yes")

        def keys():
            vm.qmp.tap("t")
            time.sleep(4)
            vm.qmp.screendump(vm.logs / "portal-consent.png")
            wait_for("the portal to release the tablet", lambda: guest("tablet-grabbed") == "no")
            clicks = vm.get("portal_clicks", [])
            if clicks:
                for x, y in clicks:
                    vm.qmp.click(x, y)
                    time.sleep(0.4)
            else:
                vm.qmp.tap(*vm.get("portal_keys", ["ret"]))
            time.sleep(4)
        result = expect("text-ascii", "tecloid-ok", action=keys)
        wait_for("Tecloid to reclaim the tablet", lambda: guest("tablet-grabbed") == "yes")
        return result

    def quick_popup():
        def choose():
            vm.qmp.tap("e")
            time.sleep(2.5)
            vm.qmp.screendump(vm.logs / "quick-popup-open.png")
            vm.qmp.tap("q")
            time.sleep(3)
        return expect("quick-popup", "quick-ok", action=choose)

    def bridge_snapshot():
        output = vm.ssh(f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus {BRIDGE}Snapshot false",
                        timeout=10)
        return json.loads(ast.literal_eval(output.strip())[0])

    def expect_system_action(key, field, expected):
        vm.qmp.tap(key)
        wait_for(f"key {key} to set GNOME {field} to {expected}",
                 lambda: bridge_snapshot()[field] == expected, timeout=10)

    def system_actions():
        for key, field, expected in (("3", "current", 1), ("4", "current", 0),
                                     ("1", "workspace", 1), ("2", "workspace", 0)):
            expect_system_action(key, field, expected)
        return "us→ru→us; workspace 1→2→1 via mapper sys: actions"

    def bridge_recovery():
        pid_command = "pgrep -u $(id -u) -x tecloid"
        pid = vm.ssh(pid_command).strip()
        if bridge_snapshot()["workspace"] != 0:
            raise StepFailed("bridge recovery requires starting on workspace 1")
        log = f"{GUEST}/app.log"
        # Only inspect messages written after the baseline actions.
        first_line = int(vm.ssh(f"wc -l < {log}")) + 1

        def bridge_available():
            return vm.ssh(
                f"if DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus "
                f"{BRIDGE}Snapshot false >/dev/null 2>&1; then echo ready; fi",
                timeout=10).strip() == "ready"

        try:
            vm.ssh(f"gnome-extensions disable {UUID}")
            wait_for("GNOME bridge to disappear", lambda: not bridge_available())
            vm.qmp.tap("1")
            wait_for("Tecloid to report the missing GNOME Action method", lambda: vm.ssh(
                f"tail -n +{first_line} {log} | "
                "awk '/action-failed: org[.]gnome[.]Shell[.]Action failed:.*(UnknownMethod|UnknownObject|UnknownInterface)/'"),
                timeout=10)
        finally:
            vm.ssh(f"gnome-extensions enable {UUID}")

        wait_for("GNOME bridge to return", bridge_available)
        if bridge_snapshot()["workspace"] != 0:
            raise StepFailed("workspace changed while the bridge was disabled")
        if vm.ssh(pid_command).strip() != pid:
            raise StepFailed("bridge recovery restarted Tecloid")
        expect_system_action("1", "workspace", 1)
        expect_system_action("2", "workspace", 0)
        return "missing bridge action reported; workspace stayed on 1; same-process switch 1→2→1 recovered"

    def command():
        guest("command-marker reset")
        vm.qmp.tap("n")
        return guest("command-marker check")

    def game_mode():
        expect("game-off", "j", ("g",))
        guest("game on")
        expect("game-on", "h", ("g",))
        guest("game off")
        return expect("game-off-again", "j", ("g",))

    def stop_keys():
        guest("toggle-mapper")
        expect("mapper-stopped-native", "a", keys=("a",))
        guest("toggle-mapper")
        return expect("mapper-restarted", "b", keys=("a",))

    try:
        step("boot-session", boot)
        step("install", install)
        if args.manual:
            step("permissions-and-extension", permissions)
            print(f"Logs and retained disk: {vm.run_dir}")
            manual_acceptance(report, checks=GNOME_MANUAL_CHECKS)
        else:
            step("keyboard-and-config", keyboard)
            step("no-permissions-start", guest, "start-app --popups auto")
            step("no-permissions-no-grab", guest, "mapper --absent --timeout 5")
            step("quit-before-permissions", guest, "quit-app")
            step("autostart-entry", guest, "autostart")
            step("permissions-and-extension", permissions)
            # Autostart is expected to launch the app after the fresh login.
            step("autostart", guest, "wait-app --timeout 90")
            step("autostart-disabled-cycle", disabled_cycle, vm, guest, install)
            step("autostart-after-reinstall", upgraded_cycle, vm, guest, install)
            step("mapper-start", guest, "mapper")
            step("tray", guest, "tray")
            if not args.autostart_only:
                step("tray-host-lifecycle", tray_recovery)
                if args.examples:
                    step("editor-smoke", ui_example, "editor")
                    step("interactions", ui_example, "interactions")
                step("passthrough", expect, "passthrough", "c", ("c",))
                step("remap", expect, "remap", "b", ("a",))
                step("chord", expect, "chord", "X", ("d",))
                step("layer-tap-hold", layer)
                step("stop-restart-native", stop_keys)
                step("system-layout-workspace", system_actions)
                step("gnome-bridge-lifecycle", bridge_recovery)
                step("command", command)
                step("game-mode-on-off", game_mode)
                step("gnome-commit-text", commit_text)
                step("clipboard-seed", clipboard_seed)
                step("text-portal-ascii", consent)
                step("quick-popup-focus-delivery", quick_popup)
                step("text-unicode", expect, "text-unicode", "Привет, ёж 😀", ("y",))
                step("clipboard-restored", clipboard_preserved)
                step("mixed-macro", expect, "mixed-macro", "nmacO", ("m",))
                step("popup-open-hide", guest, "popups")
                step("popup-destination", popup_destination)
                step("close-focused-window", verify_close_window, vm, guest)
                step("clipboard-new-owner", clipboard_interruption, "replace")
                step("clipboard-cancel-on-stop", clipboard_interruption, "hold")
                step("clipboard-only", clipboard_only)
                step("commit-through-mapper", commit_through_mapper)
                step("lock-unlock", verify_session_lock, vm, guest)
                step("remap-after-unlock", expect, "after-unlock", "b", ("a",))
                step("keyboard-replug", vm.qmp.replug_keyboard)
                step("mapper-after-replug", guest, "mapper --timeout 20")
                step("remap-after-replug", expect, "after-replug", "b", ("a",))
            step("quit", guest, "quit-app")
    except (StepFailed, subprocess.SubprocessError, OSError):
        print("Stopped on the first failed step; remaining acceptance is NOT passed.")
    except (KeyboardInterrupt, EOFError):
        report.steps.append(("controller", "FAILED", "interrupted"))
    finally:
        try:
            if vm.qemu and vm.qemu.poll() is None:
                vm.qmp.screendump(vm.logs / "final.png")
                vm.copy_from("tecloid-e2e/app.log", vm.logs)
                if args.examples:
                    vm.copy_from("tecloid-e2e/ui-snapshots", vm.logs)
                (vm.logs / "journal.txt").write_text(vm.ssh("journalctl --user -b --no-pager", check=False))
                (vm.logs / "extensions.txt").write_text(vm.ssh("gnome-extensions list --enabled; journalctl -b --no-pager _COMM=gnome-shell", check=False))
        except (StepFailed, subprocess.SubprocessError, OSError, ValueError) as error:
            report.skip("collect-diagnostics", str(error))
        finally:
            if args.manual and vm.qemu and vm.qemu.poll() is None:
                try:
                    vm.ssh("sudo systemctl poweroff", check=False, timeout=15)
                except (StepFailed, subprocess.SubprocessError, OSError) as error:
                    report.skip("manual-poweroff", str(error))
            vm.stop(keep=args.manual or args.keep or report.unsuccessful)
    report.write([f"Date: {time.strftime('%F %T %z')}", f"Commit: {commit()}",
                  f"Command: {shlex.join(sys.argv)}",
                  f"VM: {vm.name}", f"Package: {package}",
                  f"Autostart only: {args.autostart_only}",
                  f"Manual acceptance: {args.manual}; operator results",
                  f"Keyboard: {state.get('device', 'not detected')}",
                  f"Logs and screenshots: {vm.logs}",
                  ("Scope: manual GNOME acceptance; operator checklist results only." if args.manual else
                   "Scope: GNOME smoke; suspend, other receivers and previous-version upgrade are outside this run.")],
                 result_path("linux", vm.name))
    raise SystemExit(1 if report.unsuccessful else 0)


if __name__ == "__main__":
    main()
