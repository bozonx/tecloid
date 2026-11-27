#!/usr/bin/env python3
"""Measure Emoji selection to text in a GTK entry in an existing VM."""
import argparse

import json
from pathlib import Path
import queue
import shlex
import subprocess
import sys
import threading
import time

from vmlib import StepFailed, Vm, wait_for

RECEIVER = r"""
import gi, json
try:
    gi.require_version("Gtk", "4.0")
except ValueError:
    gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
app = Gtk.Application(application_id="dev.bozonx.TecloidBenchmark")
def activate(app):
    window = Gtk.ApplicationWindow(application=app, title="Tecloid benchmark receiver")
    entry = Gtk.Entry()
    if Gtk.get_major_version() == 4:
        window.set_child(entry)
    else:
        window.add(entry)
        entry.show()
    window.set_default_size(640, 80)
    entry.connect("changed", lambda entry: print(json.dumps(entry.get_text(), ensure_ascii=False), flush=True))
    window.connect("notify::is-active",
                   lambda window, _: print("focused" if window.is_active() else "blurred", flush=True))
    window.present()
    entry.grab_focus()
app.connect("activate", activate)
app.run()
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vm", required=True)
    parser.add_argument("--package", type=Path, required=True, help="Package for guest integration files")
    parser.add_argument("--binary", type=Path, required=True, help="Current portable release binary")
    parser.add_argument("--mode", action="append", required=True)
    parser.add_argument("--popups", default="winit", choices=("winit", "spell"))
    parser.add_argument("--x11", action="store_true")
    parser.add_argument("--upgrade-kde", action="store_true", help="Update KDE and libei in a disposable Fedora overlay")
    parser.add_argument("--followup-ms", type=float, help="Send c after this delay; record the order")
    parser.add_argument("--trials", type=int, default=20)
    args = parser.parse_args()
    if args.followup_ms is not None and args.followup_ms < 0:
        parser.error("--followup-ms must be nonnegative")
    if args.trials < 1:
        parser.error("--trials must be positive")
    for artifact in (args.package, args.binary):
        if not artifact.is_file():
            parser.error(f"{artifact} does not exist")
    vm = Vm(args.vm)
    work = "~/tecloid-e2e"

    def guest(command, **kwargs):
        return vm.ssh(f"python3 {work}/linux-guest.py {command}", **kwargs)

    try:
        vm.start()
        print(f"Artifacts: {vm.logs}", flush=True)
        vm.wait_ssh()
        vm.ssh(f"mkdir -p {work}")
        for name in ("linux-guest.py", "text_corpus.py"):
            vm.copy_to(Path(__file__).with_name(name), "tecloid-e2e/")
        guest("wait-session --x11" if args.x11 else "wait-session")
        vm.copy_to(args.package, "tecloid-e2e/")
        vm.copy_to(args.binary, "tecloid-e2e/benchmark-tecloid")
        install = vm.get("install", "sudo apt-get install -y {package}")
        vm.ssh(install.format(package=f"{work}/{shlex.quote(args.package.name)}"))
        vm.ssh(f"sudo install -m 755 {work}/benchmark-tecloid /usr/bin/tecloid; tecloid --version")
        vm.ssh("sudo udevadm trigger --subsystem-match=input; sudo udevadm trigger /sys/class/misc/uinput; sudo udevadm settle")
        if args.upgrade_kde:
            vm.ssh("sudo dnf upgrade -y kwin libei kglobalacceld kdecoration 'plasma*' xdg-desktop-portal-kde")
        if "gnomeCommitText" in args.mode:
            vm.ssh("/usr/share/tecloid/install-gnome-extension.sh")
            enable = (
                "from gi.repository import Gio; "
                "s = Gio.Settings.new('org.gnome.shell'); "
                "s.set_boolean('disable-user-extensions', False); "
                "uuid = 'tecloid@bozonx.dev'; "
                "s.set_strv('enabled-extensions', list(dict.fromkeys([*s.get_strv('enabled-extensions'), uuid]))); "
                "s.set_strv('disabled-extensions', [x for x in s.get_strv('disabled-extensions') if x != uuid]); "
                "Gio.Settings.sync()"
            )
            vm.ssh(f"systemd-run --user --quiet --wait --pipe --collect python3 -c {shlex.quote(enable)}")
        if args.upgrade_kde or "gnomeCommitText" in args.mode:
            vm.ssh("sudo systemd-run --quiet --on-active=2 --unit=tecloid-benchmark-reboot systemctl reboot")
            time.sleep(12)
            vm.wait_ssh()

            def desktop_ready():
                try:
                    return guest("wait-session --timeout 10", timeout=20)
                except (StepFailed, subprocess.TimeoutExpired):
                    return None

            wait_for("desktop after reboot", 180, desktop_ready, interval=2)
        if args.upgrade_kde or "libeiText" in args.mode:
            vm.ssh("pkill -x plasma-welcome || true")
            vm.ssh("systemctl --user start plasma-xdg-desktop-portal-kde.service")
            vm.ssh("systemctl --user restart xdg-desktop-portal.service")
            vm.ssh("busctl --user introspect org.freedesktop.portal.Desktop "
                   "/org/freedesktop/portal/desktop org.freedesktop.portal.RemoteDesktop")
        if "gnomeCommitText" in args.mode:
            vm.ssh("systemd-run --user --quiet --wait --pipe --collect "
                   "gnome-extensions enable tecloid@bozonx.dev")
        probe = vm.ssh(
            f"sudo python3 {work}/linux-guest.py probe-keyboard --timeout 20", background=True)
        time.sleep(2)
        for _ in range(4):
            vm.qmp.tap("shift")
        device, error = probe.communicate(timeout=25)
        if probe.returncode:
            raise RuntimeError(error)
        vm.ssh(f"cat > {work}/benchmark-receiver.py <<'PYTHON'\n{RECEIVER}\nPYTHON")
        metadata = vm.ssh("cat /etc/os-release; kwin_wayland --version 2>/dev/null || gnome-shell --version || cinnamon --version; sha256sum /usr/bin/tecloid")
        (vm.logs / "environment.txt").write_text(metadata)
        modern_kde = any(line.startswith("kwin 6.") and int(line.split(".")[1]) >= 8
                         for line in metadata.splitlines())
        rows = []
        for mode in args.mode:
            print(f"Mode: {mode}", flush=True)
            guest(f"write-config --device {shlex.quote(device.strip())} --popup-mode {args.popups}")
            config = ("import json, pathlib, sys; "
                      "p = pathlib.Path.home() / '.config/dev.bozonx.tecloid/config.json'; "
                      "c = json.loads(p.read_text()); c['settings']['linux']['textMode'] = sys.argv[1]; "
                      "p.write_text(json.dumps(c))")
            vm.ssh(f"python3 -c {shlex.quote(config)} {shlex.quote(mode)}")
            since = vm.ssh("date +%s").strip()
            vm.ssh(f"systemd-run --user --quiet --collect --unit=tecloid-benchmark-app "
                   f"--setenv=TECLOID_POPUPS={args.popups} "
                   f"--setenv=TECLOID_METRICS=$HOME/tecloid-e2e/metrics.csv "
                   f"--setenv=RUST_LOG=info,tecloid_core::text_input=debug /usr/bin/tecloid")
            guest("wait-app --timeout 45")
            guest("mapper")
            receiver = vm.ssh(f"systemd-run --user --quiet --wait --pipe --collect --unit=tecloid-benchmark-field "
                              f"--setenv=GDK_BACKEND={'x11' if args.x11 else 'wayland'} "
                              f"/usr/bin/python3 -u {work}/benchmark-receiver.py", background=True)
            events = queue.Queue()

            def collect(stream):
                for line in stream:
                    events.put((time.monotonic_ns(), line.strip()))

            threading.Thread(target=collect, args=(receiver.stdout,), daemon=True).start()

            def receive(predicate, timeout=5):
                deadline = time.monotonic() + timeout
                while True:
                    at, line = events.get(timeout=max(0, deadline - time.monotonic()))
                    if predicate(line):
                        return at, line

            receive(lambda line: line == "focused", 15)
            try:
                for trial in range(args.trials + 1):
                    before = int(guest("popup-count emoji t4_focused"))
                    vm.ssh("tecloid show emoji")
                    guest(f"popup-wait emoji t4_focused --after {before}")
                    start = time.monotonic_ns()
                    vm.qmp._keys(["q"], True)
                    vm.qmp._keys(["q"], False)
                    followup = trial > 0 and args.followup_ms is not None
                    if followup:
                        time.sleep(args.followup_ms / 1000)
                        vm.qmp._keys(["c"], True)
                        vm.qmp._keys(["c"], False)
                    if trial == 0 and mode in {"libeiText", "clipboardPaste", "libeiWithClipboardFallback"}:
                        time.sleep(0.5)
                        if guest("portal-visible").strip() == "yes":
                            allow_x = 0.75 if modern_kde else 0.58
                            point = json.loads(guest(f"portal-deny-point --x {allow_x} --y 0.86"))
                            vm.qmp.click(point["x"], point["y"])
                    at, line = receive(lambda line: line.startswith('"') and "🙂" in line, 15)
                    received = json.loads(line)
                    if followup and received == "🙂":
                        try:
                            _, line = receive(lambda line: line.startswith('"') and "c" in line, 0.5)
                            received = json.loads(line)
                        except queue.Empty:
                            pass
                    if received not in ({"🙂", "🙂c", "c🙂"} if followup else {"🙂"}):
                        raise RuntimeError(f"Unexpected receiver contents: {received!r}")
                    if trial:
                        rows.append({"mode": mode, "popups": args.popups, "trial": trial,
                                     "milliseconds": (at - start) / 1e6,
                                     "followup_ms": args.followup_ms, "received": received})
                        (vm.logs / "popup-text.json").write_text(json.dumps(rows, indent=2))
                    vm.qmp.tap("end", "backspace", "backspace")
                    receive(lambda line: line == '""')
            finally:
                log = vm.ssh(
                    f"journalctl --user -b -u tecloid-benchmark-app --since=@{since} "
                    "--no-pager -o short-monotonic", check=False)
                (vm.logs / f"{mode}.log").write_text(log)
                vm.ssh("systemctl --user stop tecloid-benchmark-field; tecloid quit", check=False)
                receiver.communicate(timeout=15)
    except Exception as error:
        print(f"Benchmark failed: {error}", file=sys.stderr, flush=True)
        if getattr(vm, "qmp", None):
            vm.qmp.screendump(vm.logs / "failure.png")
        raise
    finally:
        vm.stop()


if __name__ == "__main__":
    main()
