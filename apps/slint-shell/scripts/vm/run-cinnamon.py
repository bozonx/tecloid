#!/usr/bin/env python3
"""Linux Mint Cinnamon (X11) acceptance in a disposable VM; stop at the first failed step.

Usage: run-cinnamon.py --vm <prepared Mint Cinnamon directory> --package <.deb or tar.gz> [--keep]
       run-cinnamon.py --manual --vm <dir> --package <file>
"""

import argparse
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

from manual_acceptance import MANUAL_CHECKS, manual_acceptance
from autostart_cycle import disabled_cycle, reboot, upgraded_cycle
from text_corpus import TEXT_UNICODE
from vmlib import Report, StepFailed, Vm, commit, environment_ok, receiver_text, result_path, verify_close_window, verify_popup_destination, verify_session_lock

GUEST = "~/tecloid-e2e"
PY = f"python3 {GUEST}/linux-guest.py"
APP_ID = "dev.bozonx.tecloid"


CINNAMON_MANUAL_CHECKS = MANUAL_CHECKS[:-1] + (
    ("x11-session", "Убедись, что сессия X11 (Система → О системе или echo $XDG_SESSION_TYPE). "
                    "Трей Tecloid виден на панели Cinnamon, меню трея открывается."),
    ("x11-layout", "Раскладки us и ru: sys:switchToKeyboardLayout1/2 переключают, индикатор на панели следует. "
                   "Переключи раскладку штатно (Alt+Shift или Super+Space): правила для раскладки следуют за ней."),
    ("x11-window-actions", "Проверь sys: closeWindow, minimizeWindow, maximizeWindow, keepWindowAbove, "
                           "switchToDesktop1/2, moveWindowToDesktop2, showDesktop, tileWindowLeft/Right, "
                           "showOverview (Expo), showAppLauncher (меню), lockSession, turnOffScreen."),
    ("x11-text", "text: Привет 🙂 в xed, gnome-terminal и Firefox. Emoji/Quick actions возвращают фокус "
                 "в исходное окно и вставляют текст туда. Без запроса разрешения портала."),
    ("x11-conditions", "Правило с условием окна (класс xed) срабатывает только в xed; полноэкранное окно "
                       "(F11 в Firefox) включает условие fullscreen."),
) + MANUAL_CHECKS[-1:]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vm", required=True)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--autostart-only", action="store_true", help="only startup, disable/re-enable and reinstall checks")
    parser.add_argument("--manual", action="store_true", help="prepare a visible Cinnamon session for guided operator checks")
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--ydotool-bin-dir", type=Path,
                        help="directory with guest-compatible ydotool and ydotoold 1.0.4")
    parser.add_argument("--examples", type=Path, help="directory with current editor and interactions binaries")
    args = parser.parse_args()
    if args.manual and (args.autostart_only or args.examples):
        parser.error("--manual excludes --autostart-only and --examples")
    if args.manual and not sys.stdin.isatty():
        parser.error("--manual needs an interactive terminal")
    environment_ok()
    package = args.package.resolve()
    archive = package.name.endswith(".tar.gz")
    if not package.is_file() or not (archive or package.name.endswith(".deb")):
        parser.error("--package must be an existing release .deb or .tar.gz")
    if args.examples and any(not (args.examples / name).is_file() for name in ("editor", "interactions")):
        parser.error("--examples needs both editor and interactions binaries")
    vm = Vm(args.vm)
    if not archive and not vm.get("install"):
        parser.error("a .deb needs `install`, `reinstall` and `remove` in vm.json")
    report = Report()
    if args.manual:
        report.require("manual-" + name for name, _ in CINNAMON_MANUAL_CHECKS)
    state = {}

    def step(name, function, *arguments):
        return report.step(name, function, *arguments, fatal=True)

    def guest(command, **kwargs):
        # Every session wait must import the X11 environment into the user manager.
        if command.startswith("wait-session") and "--x11" not in command:
            command += " --x11"
        return vm.ssh(f"{PY} {command}", **kwargs).strip()

    def popup_destination():
        def open_popup():
            vm.qmp.tap("e")
            guest("x11-popup quick")

        return verify_popup_destination(vm, guest, open_popup)

    def ydotool_backend():
        socket = "/run/user/$(id -u)/tecloid-e2e-ydotool.sock"
        daemon = "ydotoold"
        tool_option = ""
        if args.ydotool_bin_dir:
            fixture = vm.ssh('printf %s "$HOME"').strip() + "/tecloid-e2e"
            for tool in ("ydotool", "ydotoold"):
                vm.copy_to(args.ydotool_bin_dir / tool, "tecloid-e2e/")
            daemon = shlex.quote(f"{fixture}/ydotoold")
            tool_option = " --ydotool-path " + shlex.quote(f"{fixture}/ydotool")
        version = vm.ssh(f"{daemon} --version", check=False).strip()
        if not version.startswith("1.0.4"):
            raise StepFailed("ydotool 1.0.4 fixture required; use --ydotool-bin-dir")
        guest("quit-app")
        guest("set-text-backend ydotool" + tool_option)
        guest(f"start-app --popups auto --ydotool-socket={socket}")
        guest("mapper")
        before = int(guest("text-failures --reason Tool"))
        expect("ydotool-no-daemon", "", ("t",))
        if int(guest("text-failures --reason Tool")) != before + 1:
            raise StepFailed("ydotool without a daemon must report one error")
        vm.ssh("sudo systemd-run --unit=tecloid-e2e-ydotoold --collect "
               f"{daemon} --socket-path={socket} --socket-own=$(id -u):$(id -g)")
        try:
            vm.ssh(f"for i in $(seq 1 50); do test -S {socket} && exit 0; sleep .1; done; exit 1")
            return expect("ydotool-ascii", "tecloid-ok", ("t",))
        finally:
            vm.ssh("sudo systemctl stop tecloid-e2e-ydotoold")
            guest("quit-app")
            guest("set-text-backend xdotool")
            guest("start-app --popups auto")
            guest("mapper")

    def boot():
        vm.start(show=args.manual)
        vm.wait_ssh()
        vm.ssh(f"mkdir -p {GUEST}")
        for name in ("linux-guest.py", "text_corpus.py"):
            vm.copy_to(Path(__file__).with_name(name), "tecloid-e2e/")
        session = guest("wait-session --timeout 180")
        if session != "x11":
            raise StepFailed(f"expected an X11 session, got {session!r}")
        info = vm.ssh("cat /etc/os-release; cinnamon --version; "
                      "systemctl --user show-environment | grep -E 'XDG_CURRENT_DESKTOP|XDG_SESSION_TYPE|DISPLAY'; "
                      "dpkg-query -W muffin xdotool x11-utils x11-xserver-utils xapp-sn-watcher xdg-desktop-portal-xapp",
                      check=False)
        (vm.logs / "environment.txt").write_text(info)
        return info.strip().splitlines()[0] if info.strip() else session

    def ui_example(name):
        vm.copy_to(args.examples / name, "tecloid-e2e/")
        arguments = " --smoke" if name == "editor" else ""
        home = vm.ssh('printf %s "$HOME"').strip()
        snapshots = f"{home}/tecloid-e2e/ui-snapshots/{name}"
        vm.ssh(f"mkdir -p {shlex.quote(snapshots)}")
        return vm.ssh(f"systemd-run --user --wait --pipe --collect --unit=tecloid-e2e-ui-{name} "
                      f"--setenv=TECLOID_EDITOR_SNAPSHOTS={shlex.quote(snapshots)} "
                      f"{GUEST}/{name}{arguments}", timeout=180).strip()

    def install(key="install"):
        vm.copy_to(package, "tecloid-e2e/")
        if archive:
            vm.ssh(f"cd {GUEST} && rm -rf bundle && mkdir bundle && "
                   f"tar -xzf {shlex.quote(package.name)} -C bundle && cd bundle/*/ && ./install-linux.sh")
            vm.ssh(f"~/.local/lib/{APP_ID}/install-linux.sh permissions")
        else:
            vm.ssh(vm.get(key).format(package=f"{GUEST}/{package.name}"))
        return guest("check-install" + ("" if archive else " --packaged"))

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
        guest(f"write-config --device {shlex.quote(state['device'])} --popup-mode auto --text-backend xdotool")
        # Startup registration is authoritative; a JSON flag alone does not enable it.
        source = (f"~/.local/share/applications/{APP_ID}.desktop" if archive
                  else f"/usr/share/applications/{APP_ID}.desktop")
        vm.ssh(f"mkdir -p ~/.config/autostart && cp {source} ~/.config/autostart/{APP_ID}.desktop")
        return state["device"]

    def desktop_settings():
        # Manual mode: four fixed workspaces and us/ru, no generated keymap,
        # config or autostart entry; the operator creates those.
        vm.ssh("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus sh -c '"
               "gsettings set org.cinnamon.desktop.wm.preferences num-workspaces 4'")
        guest("x11-layouts --configure")
        return "four workspaces; us,ru layouts"

    def x11_state():
        return json.loads(guest("x11-state"))

    def wait_for(description, predicate, timeout=30):
        deadline = time.monotonic() + timeout
        while not predicate():
            if time.monotonic() >= deadline:
                raise StepFailed(f"timed out waiting for {description}")
            time.sleep(0.3)

    def relogin():
        # A real reboot: session ACLs (uaccess), layouts and workspaces of the
        # test config apply at login; autostart starts the app.
        guest("quit-app")
        reboot(vm, guest)
        layouts = guest("x11-layouts")
        current = x11_state()
        if current["desktops"] is None or current["desktops"] < 2:
            raise StepFailed(f"Muffin reports {current['desktops']} desktops; expected 2")
        return f"layouts={layouts}; desktops={current['desktops']}"

    def expect(name, expected, keys=(), action=None, settle=3):
        guest(f"start-receiver --name {name}")
        if action:
            action()
        if keys:
            vm.qmp.tap(*keys)
        time.sleep(settle)
        vm.qmp.screendump(vm.logs / f"{name}.png")
        vm.qmp.tap("ret")
        result = receiver_text(guest(f"receiver-result --name {name}"))
        if result != expected:
            raise StepFailed(f"expected {expected!r}, received {result!r}; see {name}.png and app.log")
        return repr(result)

    def layer():
        def keys():
            vm.qmp.tap("f")
            vm.qmp.sequence(("f", True), 0.4, ("j", True), 0.05,
                            ("j", False), 0.05, ("f", False))
        return expect("layer", "gk", action=keys)

    def quick_popup():
        def choose():
            vm.qmp.tap("e")
            guest("x11-popup quick")
            vm.qmp.screendump(vm.logs / "quick-popup-open.png")
            vm.qmp.tap("q")
            time.sleep(3)
        return expect("quick-popup", "quick-ok", action=choose)

    def expect_x11(key, field, expected):
        vm.qmp.tap(key)
        wait_for(f"key {key} to set X11 {field} to {expected}",
                 lambda: x11_state()[field] == expected, timeout=10)

    def system_actions():
        guest("x11-layouts")
        start = x11_state()
        if (start["desktop"], start["group"]) != (0, 0):
            raise StepFailed(f"unexpected start state {start}")
        for key, field, expected in (("3", "group", 1), ("4", "group", 0),
                                     ("1", "desktop", 1), ("2", "desktop", 0)):
            expect_x11(key, field, expected)
        return "us→ru→us (XKB group); desktop 1→2→1 (EWMH) via mapper sys: actions"

    def russian_layout():
        def type_in_russian():
            vm.qmp.tap("3")
            wait_for("XKB group 2", lambda: x11_state()["group"] == 1, timeout=10)
            vm.qmp.tap("c")
            vm.qmp.tap("a")
        try:
            # Passthrough C and remapped A→B follow the active group.
            return expect("russian-layout", "си", action=type_in_russian)
        finally:
            vm.qmp.tap("4")
            wait_for("XKB group 1", lambda: x11_state()["group"] == 0, timeout=10)

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

    def remove():
        if archive:
            vm.ssh(f"~/.local/lib/{APP_ID}/install-linux.sh uninstall")
        else:
            vm.ssh(vm.get("remove"))
        return guest("check-removed")

    try:
        step("boot-session", boot)
        step("install", install)
        if args.manual:
            step("desktop-settings", desktop_settings)
            step("relogin-with-settings", relogin)
            print(f"Logs and retained disk: {vm.run_dir}")
            manual_acceptance(report, checks=CINNAMON_MANUAL_CHECKS)
        else:
            step("keyboard-and-config", keyboard)
            step("start-with-session-acl", guest, "start-app --popups auto")
            step("keymap-loaded", guest, f"check-keymap --device {shlex.quote(state['device'])}")
            step("tray", guest, "tray")
            step("mapper-start", guest, "mapper --timeout 30")
            step("autostart-entry", guest, "autostart")
            step("relogin-with-settings", relogin)
            step("autostart", guest, "wait-app --timeout 90")
            step("tray-after-login", guest, "tray")
            step("mapper-running", guest, "mapper --timeout 30")
            step("autostart-disabled-cycle", disabled_cycle, vm, guest, lambda: install("reinstall"))
            if not args.autostart_only:
                if args.examples:
                    step("editor-smoke", ui_example, "editor")
                    step("interactions", ui_example, "interactions")
                step("passthrough", expect, "passthrough", "c", ("c",))
                step("remap", expect, "remap", "b", ("a",))
                step("chord", expect, "chord", "X", ("d",))
                step("layer-tap-hold", layer)
                step("macro", expect, "macro", "nmacO", ("m",))
                step("stop-restart-native", stop_keys)
                step("system-layout-desktop", system_actions)
                step("russian-layout", russian_layout)
                step("command", command)
                step("game-mode-on-off", game_mode)
                # No portal on X11: text goes through xdotool without consent.
                step("text-ascii", expect, "text-ascii", "tecloid-ok", ("t",))
                step("text-unicode", expect, "text-unicode", TEXT_UNICODE, ("y",), None, 5)
                step("text-composed-emoji", expect, "text-composed-emoji",
                     "Привет 👩🏽‍💻", ("i",), None, 5)
                step("quick-popup-focus-delivery", quick_popup)
                step("popup-open-hide", guest, "popups")
                step("popup-destination", popup_destination)
                step("close-focused-window", verify_close_window, vm, guest)
                step("cli", guest, "cli")
                step("second-instance", guest, "second-instance")
                step("ydotool-explicit", ydotool_backend)
                step("lock-unlock", verify_session_lock, vm, guest)
                step("remap-after-unlock", expect, "after-unlock", "b", ("a",))
                step("keyboard-replug", vm.qmp.replug_keyboard)
                step("mapper-after-replug", guest, "mapper --timeout 20")
                step("remap-after-replug", expect, "after-replug", "b", ("a",))
            step("autostart-after-reinstall", upgraded_cycle, vm, guest, lambda: install("reinstall"))
            step("quit", guest, "quit-app")
            step("remove", remove)
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
                (vm.logs / "xsession-errors.txt").write_text(vm.ssh("cat ~/.xsession-errors", check=False))
        except (StepFailed, subprocess.SubprocessError, OSError, ValueError) as error:
            report.skip("collect-diagnostics", str(error))
        finally:
            if vm.qemu and vm.qemu.poll() is None:
                try:
                    vm.ssh("sudo systemctl poweroff", check=False, timeout=15)
                except (StepFailed, subprocess.SubprocessError, OSError) as error:
                    report.skip("poweroff", str(error))
            vm.stop(keep=args.manual or args.keep or report.unsuccessful)
    report.write([f"Date: {time.strftime('%F %T %z')}", f"Commit: {commit()}",
                  f"Command: {shlex.join(sys.argv)}",
                  f"VM: {vm.name}", f"Package: {package}",
                  f"Autostart only: {args.autostart_only}",
                  f"Manual acceptance: {args.manual}; operator results",
                  f"Keyboard: {state.get('device', 'not detected')}",
                  f"Logs and screenshots: {vm.logs}",
                  ("Scope: manual Cinnamon X11 acceptance; operator checklist results only." if args.manual else
                   "Scope: Cinnamon X11 smoke; suspend, other window actions, "
                   "clipboard and upgrade from a previous release not covered.")],
                 result_path("linux", vm.name))
    raise SystemExit(1 if report.unsuccessful else 0)


if __name__ == "__main__":
    main()
