#!/usr/bin/env python3
"""Guest side of the Linux VM e2e stand. `run-linux.py` copies this file into
the VM and calls one subcommand per step over ssh as the autologin user.
Only the Python standard library and tools of a KDE Plasma, GNOME or
Cinnamon (X11, `xdotool`/`xprop`) install are used.
"""

import argparse
import ast
import csv
import errno
import fcntl
import glob
import grp
import json
import os
from pathlib import Path
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import time

from text_corpus import TEXT_CORPUS, TEXT_UNICODE

APP_ID = "dev.bozonx.tecloid"
MAPPER_DEVICE = "Tecloid Virtual Keyboard"
TRAY_ID = "tecloid-slint-shell"
HOME = Path.home()
WORK = HOME / "tecloid-e2e"
APP_LOG = WORK / "app.log"
METRICS = WORK / "metrics.csv"
CONFIG_DIR = HOME / ".config" / APP_ID
DATA_DIR = HOME / ".local/share" / APP_ID

FAKE_GAME = "tecloid-fake-game"

# Every rule maps a physical key to a result the host can read back from the
# receiver window. Keys not listed here (c, h, j, k, o, q, s...) pass through.
E2E_LAYOUT = f'''{{
  "version": 1,
  "meta": {{"description": "VM e2e"}},
  "layers": [
    {{
      "id": "nav",
      "keys": {{
        "KeyJ": "KeyK"
      }}
    }}
  ],
  "rules": [
    {{
      "key": "KeyA",
      "tap": "KeyB"
    }},
    {{
      "key": "KeyD",
      "tap": "Shift+KeyX"
    }},
    {{
      "key": "KeyT",
      "tap": "text:tecloid-ok"
    }},
    {{
      "key": "KeyF",
      "tap": "KeyG",
      "hold": {{
        "layer": "nav"
      }}
    }},
    {{
      "key": "KeyY",
      "tap": "text:{TEXT_UNICODE}"
    }},
    {{
      "key": "KeyM",
      "tap": "macro:e2e"
    }},
    {{
      "key": "KeyN",
      "tap": "cmd:touch"
    }},
    {{
      "key": "KeyE",
      "tap": "app:showQuickMenu1"
    }},
    {{
      "key": "KeyG",
      "when": {{
        "gameMode": "on"
      }},
      "tap": "KeyH"
    }},
    {{
      "key": "KeyG",
      "when": {{
        "gameMode": "off"
      }},
      "tap": "KeyJ"
    }},
    {{
      "key": "Digit1",
      "tap": "sys:switchToDesktop2"
    }},
    {{
      "key": "Digit2",
      "tap": "kde:kwin/Switch to Desktop 1"
    }},
    {{
      "key": "Digit3",
      "tap": "sys:switchToKeyboardLayout2"
    }},
    {{
      "key": "Digit4",
      "tap": "sys:switchToKeyboardLayout1"
    }}
  ],
  "macros": [
    {{
      "id": "e2e",
      "steps": [
        "KeyN",
        "pause:300",
        "text:mac",
        "Shift+KeyO"
      ]
    }}
  ],
  "commands": [
    {{
      "id": "touch",
      "sh": "touch tecloid-e2e/cmd-ran"
    }}
  ],
  "emojiMenuPages": [
    {{
      "name": "E2E",
      "cells": {{
        "KeyQ": "🙂"
      }}
    }}
  ],
  "quickMenuPages": [
    {{
      "name": "E2E",
      "cells": {{
        "KeyQ": "text:quick-ok"
      }}
    }}
  ]
}}'''


def fail(message):
    print(message, file=sys.stderr)
    sys.exit(1)


def wait(what, timeout, probe, interval=0.5):
    deadline = time.monotonic() + timeout
    while True:
        value = probe()
        if value:
            return value
        if time.monotonic() >= deadline:
            fail(f"timed out after {timeout}s waiting for {what}")
        time.sleep(interval)


def binary():
    path = shutil.which("tecloid") or shutil.which(
        "tecloid", path=str(HOME / ".local/bin"))
    if not path:
        fail("tecloid is not installed")
    return path


def app(*arguments, timeout=10):
    return subprocess.run([binary(), *arguments], capture_output=True, text=True,
                          timeout=timeout)


def session_pid(group=None):
    """PID of this user's desktop shell, optionally with `group` in its groups."""
    gid = grp.getgrnam(group).gr_gid if group else None
    for status in glob.glob("/proc/[0-9]*/status"):
        try:
            text = Path(status).read_text()
            if os.stat(status).st_uid != os.getuid():
                continue
        except OSError:
            continue
        fields = dict(line.split(":\t", 1) for line in text.splitlines() if ":\t" in line)
        if fields.get("Name", "").strip() not in ("plasmashell", "gnome-shell", "cinnamon"):
            continue
        if fields.get("Name", "").strip() == "gnome-shell" and "WAYLAND_DISPLAY" not in user_environment():
            continue
        if gid is None or str(gid) in fields.get("Groups", "").split():
            return int(Path(status).parent.name)
    return None


def user_environment():
    output = subprocess.run(["systemctl", "--user", "show-environment"],
                            capture_output=True, text=True).stdout
    return dict(line.split("=", 1) for line in output.splitlines() if "=" in line)


def desktop():
    """`kde`, `gnome` or `cinnamon` from the session's XDG_CURRENT_DESKTOP."""
    current = user_environment().get("XDG_CURRENT_DESKTOP", "").upper()
    if "CINNAMON" in current:
        return "cinnamon"
    return "gnome" if "GNOME" in current else "kde"


# Variables the stand's systemd units need to reach an X11 session.
X11_SESSION_VARIABLES = ("DISPLAY", "XAUTHORITY", "XDG_CURRENT_DESKTOP",
                         "XDG_SESSION_TYPE", "XDG_SESSION_DESKTOP", "DESKTOP_SESSION")


def import_x11_environment():
    """Copy the X11 session variables of the running `cinnamon` process into
    the user manager. cinnamon-session does not export DISPLAY to systemd in
    every release, but units started by the stand need it."""
    pid = session_pid()
    if pid is None:
        return False
    try:
        raw = Path(f"/proc/{pid}/environ").read_bytes()
    except OSError:
        return False
    found = dict(item.split("=", 1) for item in raw.decode(errors="replace").split("\0") if "=" in item)
    if "DISPLAY" not in found:
        return False
    if "CINNAMON" not in found.get("XDG_CURRENT_DESKTOP", "").upper():
        return False
    assignments = [f"{key}={found[key]}" for key in X11_SESSION_VARIABLES if key in found]
    subprocess.run(["systemctl", "--user", "set-environment", *assignments], check=True)
    return True


def x11_environment():
    return {**os.environ, **user_environment()}


def xprop_root(atom):
    result = subprocess.run(["xprop", "-root", atom], capture_output=True, text=True,
                            env=x11_environment())
    if result.returncode:
        fail(f"xprop -root {atom}: {result.stderr.strip()}")
    return result.stdout.split("=", 1)[1].strip() if "=" in result.stdout else ""


def xkb_group():
    """Effective XKB group of the core keyboard through libX11 (no extra tools)."""
    import ctypes
    import ctypes.util

    class XkbState(ctypes.Structure):
        _fields_ = [("group", ctypes.c_ubyte), ("locked_group", ctypes.c_ubyte),
                    ("base_group", ctypes.c_ushort), ("latched_group", ctypes.c_short),
                    ("mods", ctypes.c_ubyte), ("base_mods", ctypes.c_ubyte),
                    ("latched_mods", ctypes.c_ubyte), ("locked_mods", ctypes.c_ubyte),
                    ("compat_state", ctypes.c_ubyte), ("grab_mods", ctypes.c_ubyte),
                    ("compat_grab_mods", ctypes.c_ubyte), ("lookup_mods", ctypes.c_ubyte),
                    ("compat_lookup_mods", ctypes.c_ubyte), ("ptr_buttons", ctypes.c_ushort)]

    library = ctypes.util.find_library("X11")
    if not library:
        fail("libX11 not found")
    x11 = ctypes.CDLL(library)
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XkbGetState.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.POINTER(XkbState)]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = x11.XOpenDisplay(x11_environment().get("DISPLAY", "").encode())
    if not display:
        fail("cannot open the X11 display")
    try:
        state = XkbState()
        if x11.XkbGetState(display, 0x100, ctypes.byref(state)):
            fail("XkbGetState failed")
        return state.group
    finally:
        x11.XCloseDisplay(display)


def x11_layouts():
    """Layouts from `_XKB_RULES_NAMES = "evdev", "pc105", "us,ru", ",", ""`."""
    value = xprop_root("_XKB_RULES_NAMES")
    fields = [field.strip('"').strip() for field in value.split('", "')] if value else []
    return fields[2] if len(fields) > 2 else ""


def cinnamon_layout_setting(env):
    schemas = subprocess.check_output(["gsettings", "list-schemas"], text=True, env=env).split()
    if "org.cinnamon.desktop.input-sources" in schemas:
        return ("org.cinnamon.desktop.input-sources", "sources",
                "[('xkb', 'us'), ('xkb', 'ru')]")
    return ("org.gnome.libgnomekbd.keyboard", "layouts", "['us', 'ru']")


def active_window_title():
    result = subprocess.run(["xdotool", "getactivewindow", "getwindowname"],
                            capture_output=True, text=True, env=x11_environment())
    return result.stdout.strip() if result.returncode == 0 else ""


def systemd_run(unit, command, output, extra_env=(), errors=None):
    subprocess.run(["systemctl", "--user", "reset-failed", unit],
                   capture_output=True)
    subprocess.run(
        ["systemd-run", "--user", "--quiet", "--collect", f"--unit={unit}",
         f"--property=StandardOutput=append:{output}",
         f"--property=StandardError=append:{errors or output}",
         *(f"--setenv={item}" for item in extra_env), *command],
        check=True,
    )


def unit_active(unit):
    return subprocess.run(["systemctl", "--user", "is-active", "--quiet", unit]).returncode == 0


def mapper_running():
    try:
        return f'N: Name="{MAPPER_DEVICE}"' in Path("/proc/bus/input/devices").read_text()
    except OSError:
        return False


# --- subcommands ---------------------------------------------------------

def cmd_wait_session(args):
    wait(f"a desktop session{' with group ' + args.group if args.group else ''}",
         args.timeout, lambda: session_pid(args.group), interval=2)
    if args.x11:
        wait("the Cinnamon X11 session environment", args.timeout,
             lambda: import_x11_environment() and "DISPLAY" in user_environment(), interval=2)
    else:
        wait("WAYLAND_DISPLAY in the user manager", args.timeout,
             lambda: "WAYLAND_DISPLAY" in user_environment())
    WORK.mkdir(exist_ok=True)
    print(user_environment().get("XDG_SESSION_TYPE", "x11" if args.x11 else "wayland"))


def cmd_check_install(args):
    path = binary()
    result = app("--version")
    if result.returncode:
        fail(f"installed executable cannot run ({result.returncode}): {result.stderr.strip()}")
    version = result.stdout.strip()
    prefixes = [Path("/usr/share"), HOME / ".local/share"]
    problems = []
    def present(relative):
        return any((prefix / relative).exists() for prefix in prefixes)
    for relative in (f"applications/{APP_ID}.desktop", f"metainfo/{APP_ID}.metainfo.xml",
                     f"icons/hicolor/scalable/apps/{APP_ID}.svg",
                     f"icons/hicolor/64x64/apps/{APP_ID}.png"):
        if not present(relative):
            problems.append(f"missing {relative}")
    if shutil.which("desktop-file-validate"):
        for prefix in prefixes:
            entry = prefix / f"applications/{APP_ID}.desktop"
            if entry.exists():
                result = subprocess.run(["desktop-file-validate", str(entry)],
                                        capture_output=True, text=True)
                if result.returncode:
                    problems.append(f"desktop-file-validate: {result.stdout}{result.stderr}")
    if args.packaged:
        if not Path("/usr/lib/udev/rules.d/60-tecloid.rules").exists():
            problems.append("udev rule is not installed")
        if "input" in subprocess.check_output(["id", "-nG"], text=True).split():
            problems.append("VM user must not belong to input: ACL coverage would be hidden")
        # The uaccess rule gives the active session's user write access.
        if not os.access("/dev/uinput", os.W_OK):
            problems.append("/dev/uinput is not writable for the session user")
    if problems:
        fail("\n".join(problems))
    print(f"{path}: {version}")


def cmd_probe_keyboard(args):
    """Report the keyboard node that receives the host's QMP key events.
    Needs read access to /dev/input (run through sudo before the user has it)."""
    candidates = {}
    for node in sorted(glob.glob("/dev/input/event*")):
        caps = Path(f"/sys/class/input/{Path(node).name}/device/capabilities/key")
        try:
            words = caps.read_text().split()
        except OSError:
            continue
        bits = int("".join(f"{int(word, 16):064b}" for word in words) or "0", 2)
        if bits >> 30 & 1 and bits >> 42 & 1:  # KEY_A and KEY_LEFTSHIFT
            try:
                candidates[os.open(node, os.O_RDONLY | os.O_NONBLOCK)] = node
            except OSError:
                pass
    if not candidates:
        fail("no readable keyboards")
    size = struct.calcsize("llHHi")
    deadline = time.monotonic() + args.timeout
    while time.monotonic() < deadline:
        ready, _, _ = select.select(list(candidates), [], [], 0.5)
        for fd in ready:
            data = os.read(fd, size * 64)
            for offset in range(0, len(data) - size + 1, size):
                _, _, kind, code, _ = struct.unpack_from("llHHi", data, offset)
                if kind == 1 and code == 42:
                    node = candidates[fd]
                    for link in glob.glob("/dev/input/by-id/*-event-kbd"):
                        if os.path.realpath(link) == node:
                            node = link
                    print(node)
                    return
    fail("no keyboard received the probe keys; is usb-kbd attached?")


def cmd_write_config(args):
    (DATA_DIR / "keymaps").mkdir(parents=True, exist_ok=True)
    layout = E2E_LAYOUT
    if desktop() != "kde":
        layout = layout.replace("kde:kwin/Switch to Desktop 1", "sys:switchToDesktop1")
    parsed = json.loads(layout)
    parsed["rules"].append({"key": "F9", "tap": "sys:closeWindow"})
    parsed["rules"].append({"key": "KeyI", "tap": "text:Привет 👩🏽‍💻"})
    parsed["rules"].append({"key": "KeyU", "tap": "text:" + TEXT_CORPUS})
    (DATA_DIR / "keymaps/E2E.json").write_text(json.dumps(parsed, ensure_ascii=False), encoding="utf-8")
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    environment = HOME / '.config/environment.d'
    environment.mkdir(parents=True, exist_ok=True)
    (environment / '90-tecloid-e2e.conf').write_text(f'TECLOID_POPUPS={args.popup_mode}\nTECLOID_METRICS={METRICS}\nRUST_LOG=info\n')
    settings = {
        "experimental": True,
        "linux": {"textMode": {"libei": "libeiWithClipboardFallback", "clipboard": "clipboardPaste"}.get(args.text_backend, args.text_backend)},
        "devices": {"keyboards": [args.device]},
        "keymaps": {"mode": "manual", "manualKeymap": "user:E2E", "commandsAllowed": ["user:E2E"]},
        "startMapperOnLaunch": True,
        "launchAtLogin": True,
        "interface": {"locale": "en"},
        "gameMode": {
            "processes": [{"name": FAKE_GAME, "foregroundOnly": False}],
        },
    }
    if args.capture_tablet:
        settings["devices"]["mice"] = [test_tablet()]
    (CONFIG_DIR / "config.json").write_text(
        json.dumps({"version": 1, "settings": settings}, indent=2),
        encoding="utf-8")
    if desktop() == "cinnamon":
        # Read by Cinnamon in this session and the next login.
        env = x11_environment()
        for setting in (("org.cinnamon.desktop.wm.preferences", "num-workspaces", "2"),
                        cinnamon_layout_setting(env)):
            subprocess.run(["gsettings", "set", *setting], env=env, check=True)
        return
    if desktop() == "gnome":
        env = dict(os.environ, **user_environment())
        for schema, key, value in (
            ("org.gnome.desktop.input-sources", "sources", "[('xkb', 'us'), ('xkb', 'ru')]"),
            ("org.gnome.mutter", "dynamic-workspaces", "false"),
            ("org.gnome.desktop.wm.preferences", "num-workspaces", "2"),
        ):
            subprocess.run(["gsettings", "set", schema, key, value], env=env, check=True)
        return
    # Read by Plasma at the next login: two keyboard layouts and two desktops
    # for the `sys:` and `kde:` actions.
    config = HOME / ".config"
    (config / "kxkbrc").write_text(
        "[Layout]\nDisplayNames=,\nLayoutList=us,ru\nUse=true\nVariantList=,\n",
        encoding="utf-8")
    subprocess.run(["kwriteconfig6", "--file", "kwinrc", "--group", "Desktops",
                    "--key", "Number", "2"], check=True)
    subprocess.run(["kwriteconfig6", "--file", "kwinrc", "--group", "Desktops",
                    "--key", "Rows", "1"], check=True)


def cmd_check_keymap(args):
    settings = json.loads((CONFIG_DIR / "config.json").read_text(encoding="utf-8")).get("settings", {})
    if (settings.get("devices", {}).get("keyboards") != [args.device]
            or settings.get("keymaps", {}).get("manualKeymap") != "user:E2E"
            # Defaults are not written: a missing mode is manual.
            or settings.get("keymaps", {}).get("mode", "manual") != "manual"):
        fail("application did not retain the test device/keymap; package and fixture may use different config formats")
    print("test keyboard and active keymap retained by the application")


def cmd_set_text_backend(args):
    path = CONFIG_DIR / "config.json"
    config = json.loads(path.read_text(encoding="utf-8"))
    linux = config["settings"]["linux"]
    linux["textMode"] = args.mode
    if getattr(args, "ydotool_path", None):
        linux["ydotoolPath"] = args.ydotool_path
    path.write_text(json.dumps(config, ensure_ascii=False), encoding="utf-8")


def cmd_start_app(args):
    WORK.mkdir(exist_ok=True)
    env = ["RUST_LOG=info", f"TECLOID_POPUPS={args.popups}", f"TECLOID_METRICS={METRICS}"]
    if args.ydotool_socket:
        env.append(f"YDOTOOL_SOCKET={args.ydotool_socket}")
    systemd_run("tecloid-e2e-app", [binary()], APP_LOG, env)
    cmd_wait_app(args)


def cmd_wait_app(args):
    if getattr(args, "absent", False):
        deadline = time.monotonic() + args.timeout
        while time.monotonic() < deadline:
            if processes() or app("ping").returncode == 0:
                fail("Tecloid started with autostart disabled")
            time.sleep(1)
        return
    wait("an answer to `ping`", args.timeout, lambda: app("ping").returncode == 0)


def cmd_quit_app(args):
    app("quit")
    wait("the process to exit", 15,
         lambda: subprocess.run(["pgrep", "-u", str(os.getuid()), "-x", "tecloid"],
                                capture_output=True).returncode != 0)


def cmd_mapper(args):
    if args.absent:
        # A failing start is retried for a few seconds after launch.
        time.sleep(args.timeout)
        if mapper_running():
            fail("the mapper virtual keyboard exists")
        return
    wait("the mapper virtual keyboard", args.timeout, mapper_running)


def cmd_toggle_mapper(args):
    before = mapper_running()
    if app("toggle-mapper").returncode:
        fail("toggle-mapper failed")
    wait(f"the mapper to {'stop' if before else 'start'}", 15,
         lambda: mapper_running() != before)


def cmd_set_mapper(args):
    enabled = args.state == "on"
    if mapper_running() != enabled and app("toggle-mapper").returncode:
        fail("toggle-mapper failed")
    wait(f"the mapper to be {args.state}", 15, lambda: mapper_running() == enabled)
    # uinput disappears on the worker before the shell processes its completion
    # and clears mapper_busy. A back-to-back toggle can otherwise be ignored.
    time.sleep(1)
    if mapper_running() != enabled:
        fail(f"the mapper did not remain {args.state}")


def cmd_tray(args):
    def items():
        result = subprocess.run(
            ["busctl", "--user", "--json=short", "get-property",
             "org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher",
             "org.kde.StatusNotifierWatcher", "RegisteredStatusNotifierItems"],
            capture_output=True, text=True)
        return json.loads(result.stdout)["data"] if result.returncode == 0 else []

    def ours():
        for item in items():
            if "@" in item:
                service, path = item.split("@", 1)
            elif "/" in item:
                service, suffix = item.split("/", 1)
                path = "/" + suffix
            else:
                service, path = item, "/StatusNotifierItem"
            result = subprocess.run(
                ["busctl", "--user", "--json=short", "get-property", service, path,
                 "org.kde.StatusNotifierItem", "Id"], capture_output=True, text=True)
            if result.returncode == 0 and json.loads(result.stdout)["data"] == TRAY_ID:
                return item
        return None
    print(wait("the tray item", args.timeout, ours))


def cmd_autostart(args):
    entry = HOME / ".config/autostart" / f"{APP_ID}.desktop"
    wait(str(entry), 10, entry.exists)
    exec_line = next((line for line in entry.read_text().splitlines()
                      if line.startswith("Exec=")), "")
    target = os.path.realpath(binary())
    if target not in exec_line and binary() not in exec_line:
        fail(f"{entry}: {exec_line!r} does not start {target}")
    print(exec_line)


def receiver_path(name):
    return WORK / f"receiver-{name}.json"


def cmd_receive(args):
    """Capture the dialog's exit status and text atomically, including empty input."""
    title = receiver_title(args.title_suffix)
    command = (["zenity", "--entry", f"--title={title}", "--text=Mapper output"]
               if desktop() in ("gnome", "cinnamon")
               else ["kdialog", "--title", title, "--inputbox", "Mapper output"])
    result = subprocess.run(command, capture_output=True)
    # kdialog/zenity append one framing newline; preserve all user data before it.
    text = result.stdout.decode("utf-8").removesuffix("\n")
    value = {"status": "accepted" if result.returncode == 0 else "failed",
             "text": text, "exit_code": result.returncode}
    path = receiver_path(args.name)
    pending = path.with_suffix(".tmp")
    pending.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
    pending.replace(path)


def receiver_title(suffix):
    return f"Tecloid e2e receiver {suffix}" if suffix else "Tecloid e2e receiver"


def receiver_focused(title="Tecloid e2e receiver"):
    if desktop() == "cinnamon":
        return active_window_title() == title
    if desktop() == "gnome":
        result = subprocess.run([
            "gdbus", "call", "--session", "--dest", "org.gnome.Shell",
            "--object-path", "/dev/bozonx/Tecloid/Gnome", "--method",
            "dev.bozonx.Tecloid.Gnome.Snapshot", "true"],
            capture_output=True, text=True, check=True, env={**os.environ, **user_environment()})
        window = json.loads(ast.literal_eval(result.stdout.strip())[0])["window"]
        return bool(window and window["title"] == title)
    return kwin_value("receiver-focus", """
var w = workspace.activeWindow;
console.warn(MARKER + JSON.stringify(Boolean(w && String(w.caption) === TITLE)));
""".replace("TITLE", json.dumps(title)))


def cmd_start_receiver(args):
    if not args.keep_existing:
        stopped = subprocess.run(
            ["systemctl", "--user", "stop", "tecloid-e2e-receiver-*.service"],
            capture_output=True, text=True, timeout=10)
        # A completed transient receiver can unload between tests.
        if stopped.returncode not in (0, 5):
            fail(f"stop previous receivers: {stopped.stderr.strip()}")
    receiver_path(args.name).unlink(missing_ok=True)
    systemd_run(f"tecloid-e2e-receiver-{args.name}",
                [sys.executable, str(Path(__file__).resolve()), "receive", "--name", args.name,
                 "--title-suffix", args.name if args.tagged else ""],
                WORK / "receiver.log",
                ["GDK_BACKEND=x11"] if args.xwayland else [])
    wait("receiver window focus", 15, lambda: receiver_focused(
        receiver_title(args.name if args.tagged else "")))


TERMINAL_TITLE = "Tecloid e2e terminal"


def cmd_terminal_receive(args):
    """Read one line from Konsole's tty: a paste arrives as typed text, a
    plain Ctrl+V becomes the tty's literal-next character instead."""
    raw = sys.stdin.buffer.readline()
    value = {"status": "accepted", "exit_code": 0,
             "text": raw.decode("utf-8", "replace").removesuffix("\n")}
    path = receiver_path(args.name)
    pending = path.with_suffix(".tmp")
    pending.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
    pending.replace(path)


def terminal_focused():
    return kwin_value("terminal-focus", """
var w = workspace.activeWindow;
console.warn(MARKER + JSON.stringify(Boolean(w && String(w.resourceClass).indexOf("konsole") >= 0 &&
    String(w.caption).indexOf(TITLE) >= 0)));
""".replace("TITLE", json.dumps(TERMINAL_TITLE)))


def cmd_start_terminal_receiver(args):
    if not shutil.which("konsole"):
        fail("konsole is not installed in the guest")
    subprocess.run(["systemctl", "--user", "stop", "tecloid-e2e-receiver-*.service"],
                   capture_output=True, text=True, timeout=10)
    receiver_path(args.name).unlink(missing_ok=True)
    systemd_run(f"tecloid-e2e-receiver-{args.name}",
                ["konsole", "--separate", "--nofork", "--hide-menubar", "--hide-tabbar",
                 "-p", f"tabtitle={TERMINAL_TITLE}",
                 "-e", sys.executable, str(Path(__file__).resolve()), "terminal-receive", "--name", args.name],
                WORK / "receiver.log")
    wait("terminal receiver focus", 20, terminal_focused)


def cmd_focus_terminal_receiver(args):
    test_window_point("konsole", TERMINAL_TITLE, 0.5, 0.5, activate=True)


def cmd_receiver_state(args):
    if unit_active(f"tecloid-e2e-receiver-{args.name}"):
        print("open")
        return
    path = receiver_path(args.name)
    print(json.loads(path.read_text(encoding="utf-8"))["status"] if path.exists() else "missing")


def cmd_receiver_result(args):
    path = receiver_path(args.name)
    deadline = time.monotonic() + args.timeout
    while not path.exists():
        if time.monotonic() >= deadline:
            subprocess.run(["systemctl", "--user", "stop", "--no-block", f"tecloid-e2e-receiver-{args.name}"],
                           capture_output=True, timeout=5)
            fail("receiver timed out without an accepted result")
        if not unit_active(f"tecloid-e2e-receiver-{args.name}"):
            # A completed wrapper has already renamed its result before exiting.
            if not path.exists():
                fail("receiver exited without a result")
        time.sleep(0.1)
    result = json.loads(path.read_text(encoding="utf-8"))
    if (not isinstance(result, dict) or result.get("status") != "accepted"
            or type(result.get("exit_code")) is not int or result["exit_code"] != 0
            or not isinstance(result.get("text"), str)):
        fail(f"receiver failed: {result!r}")
    if args.expect is not None and result["text"] != args.expect:
        fail(f"expected {args.expect!r}, received {result['text']!r}")
    print(json.dumps(result, ensure_ascii=False))


def metric_count(popup, event):
    count = 0
    for path in (METRICS, Path(str(METRICS) + ".spell.csv")):
        if path.exists():
            with path.open() as stream:
                count += sum(row.get("window") == popup and row.get("event") == event
                             for row in csv.DictReader(stream))
    return count


def cmd_popup_count(args):
    print(metric_count(args.popup, args.event))


def cmd_popup_wait(args):
    wait(f"{args.popup} {args.event}", args.timeout,
         lambda: metric_count(args.popup, args.event) > args.after)


def wait_x11_popup(popup, *, hidden=False):
    title = {"quick": "Quick actions", "emoji": "Emoji"}[popup]
    if hidden:
        wait(f"{popup} to hide", 15, lambda: subprocess.run(
            ["xdotool", "search", "--onlyvisible", "--name", f"^{title}$"],
            capture_output=True, env=x11_environment()).returncode == 1)
    else:
        wait(f"{popup} to take X11 focus", 15, lambda: active_window_title() == title)


def cmd_x11_popup(args):
    wait_x11_popup(args.popup)


def cmd_popups(args):
    x11 = desktop() == "cinnamon"
    for popup in ("quick", "emoji"):
        before = {event: metric_count(popup, event)
                  for event in ("t3_first_frame", "t4_focused", "hidden")} if not x11 else {}
        try:
            result = app("show", popup)
            if result.returncode:
                fail(f"show {popup}: {result.stdout}{result.stderr}")
            if x11:
                wait_x11_popup(popup)
            else:
                for event in ("t3_first_frame", "t4_focused"):
                    wait(f"{popup} {event}", 15,
                         lambda: metric_count(popup, event) > before[event])
        finally:
            result = app("hide")
            if result.returncode:
                fail(f"hide: {result.stdout}{result.stderr}")
        if x11:
            wait_x11_popup(popup, hidden=True)
        else:
            wait(f"{popup} hidden", 15, lambda: metric_count(popup, "hidden") > before["hidden"])
    if app("ping").returncode:
        fail("the instance stopped answering after the popups")


def busctl(*arguments):
    result = subprocess.run(["busctl", "--user", *arguments], capture_output=True, text=True)
    if result.returncode:
        fail(f"busctl {' '.join(arguments)}: {result.stderr.strip()}")
    return result.stdout.split()


def run_kwin_test_script(name, body):
    script = WORK / f"{name}.js"
    script.write_text(body, encoding="utf-8")
    try:
        result = busctl("call", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting",
                        "loadScript", "ss", str(script), name)
        if int(result[1]) < 0:
            fail(f"KWin refused {name}")
        busctl("call", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "start")
        time.sleep(1)
    finally:
        busctl("call", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting",
               "unloadScript", "s", name)


def kwin_value(name, body):
    marker = f"TECLOID_E2E_{time.time_ns()}="
    run_kwin_test_script("tecloid-e2e-" + name, body.replace("MARKER", json.dumps(marker)))
    log = subprocess.run(["journalctl", "--user", "-b", "--no-pager", "-o", "cat", "--grep", marker, "-n", "300"],
                         capture_output=True, text=True, check=True).stdout
    values = [line.split(marker, 1)[1] for line in log.splitlines() if marker in line]
    if len(values) != 1:
        fail(f"{name} probe did not return one result")
    return json.loads(values[0])


def test_window_point(class_hint, title, x, y, activate=False):
    # Read the real frame instead of moving a Wayland client under the pointer.
    # KWin debug/print output can be disabled; warnings reach the user journal.
    marker = f"TECLOID_E2E_POINT_{time.time_ns()}="
    body = """
var list = workspace.windowList ? workspace.windowList() : workspace.clientList();
for (var i = 0; i < list.length; i++) {
    var w = list[i];
    if (String(w.resourceClass).indexOf(CLASS_HINT) < 0 ||
        String(w.caption).toLowerCase().indexOf(TITLE.toLowerCase()) < 0 || !w.output) continue;
    var g = w.frameGeometry, o = w.output.geometry;
    ACTIVATE
    console.warn(MARKER + JSON.stringify({
        x: (g.x + g.width * BUTTON_X - o.x) / o.width,
        y: (g.y + g.height * BUTTON_Y - o.y) / o.height
    }));
}
"""
    for key, value in {
        "CLASS_HINT": json.dumps(class_hint), "TITLE": json.dumps(title),
        "BUTTON_X": str(x), "BUTTON_Y": str(y), "MARKER": json.dumps(marker),
        "ACTIVATE": "workspace.activeWindow = w;" if activate else "",
    }.items():
        body = body.replace(key, value)
    run_kwin_test_script("tecloid-e2e-window-point", body)
    log = subprocess.run(["journalctl", "--user", "-b", "--no-pager", "-o", "cat",
                          "--grep", marker, "-n", "300"], capture_output=True, text=True, check=True).stdout
    matches = [line.split(marker, 1)[1] for line in log.splitlines() if marker in line]
    if len(matches) != 1:
        fail(f"expected one {title} window geometry, received {len(matches)}")
    point = json.loads(matches[0])
    if not all(0 < point[axis] < 1 for axis in ("x", "y")):
        fail(f"test window point is outside the output: {point}")
    print(json.dumps(point))


def cmd_portal_visible(args):
    marker = f"TECLOID_E2E_PORTAL_{time.time_ns()}="
    body = """
var list = workspace.windowList ? workspace.windowList() : workspace.clientList();
var count = 0;
for (var i = 0; i < list.length; i++) {
    if (String(list[i].resourceClass).indexOf('portal') >= 0 &&
        String(list[i].caption).toLowerCase().indexOf('remote control') >= 0) count++;
}
console.warn(MARKER + count);
""".replace('MARKER', json.dumps(marker))
    run_kwin_test_script('tecloid-e2e-portal-visible', body)
    log = subprocess.run(['journalctl', '--user', '-b', '--no-pager', '-o', 'cat', '--grep', marker, '-n', '300'],
                         capture_output=True, text=True, check=True).stdout
    values = [line.split(marker, 1)[1] for line in log.splitlines() if marker in line]
    if len(values) != 1:
        fail('portal visibility probe did not return one result')
    print('yes' if int(values[0]) else 'no')


def cmd_portal_deny_point(args):
    if not (0 < args.x < 1 and 0 < args.y < 1):
        fail("portal button coordinates must be fractions inside the window")
    test_window_point("portal", "Remote Control", args.x, args.y)


def cmd_focus_receiver(args):
    test_window_point("kdialog", "Tecloid e2e receiver", 0.5, 0.535, activate=True)


def cmd_kde_state(args):
    """Current virtual desktop (1-based) and keyboard layout index."""
    desktop = busctl("call", "org.kde.KWin", "/KWin", "org.kde.KWin", "currentDesktop")[1]
    layout = busctl("call", "org.kde.keyboard", "/Layouts", "org.kde.KeyboardLayouts",
                    "getLayout")[1]
    print(f"desktop={desktop} layout={layout}")


def cmd_x11_state(args):
    """Desktop, XKB group and layouts of the X11 session as JSON."""
    def cardinal(atom):
        value = xprop_root(atom)
        return int(value) if value.isdigit() else None
    print(json.dumps({"desktop": cardinal("_NET_CURRENT_DESKTOP"),
                      "desktops": cardinal("_NET_NUMBER_OF_DESKTOPS"),
                      "group": xkb_group(), "layouts": x11_layouts(),
                      "active": active_window_title()}, ensure_ascii=False))


def cmd_x11_layouts(args):
    env = x11_environment()
    setting = cinnamon_layout_setting(env)
    if args.configure:
        subprocess.run(["gsettings", "set", *setting], check=True, env=env)
    # Cinnamon 6.6 controls XKB through Muffin and can append a fallback
    # group. Replacing its keymap with setxkbmap breaks its source switching.
    if setting[0] == "org.cinnamon.desktop.input-sources":
        def configured():
            layouts = x11_layouts()
            return layouts if layouts.split(",")[:2] == ["us", "ru"] else None

        print(wait("Cinnamon to apply the us,ru input sources", 20, configured))
    else:
        if x11_layouts() != "us,ru":
            subprocess.run(["setxkbmap", "-layout", "us,ru"], check=True, env=env)
        print(x11_layouts())


def test_tablet():
    paths = sorted(Path("/dev/input/by-id").glob("*QEMU*Tablet*-event-*"))
    if len(paths) != 1:
        fail(f"expected one QEMU tablet event device, found {paths}")
    return str(paths[0])


def cmd_tablet_grabbed(args):
    with open(test_tablet(), "rb") as device:
        try:
            fcntl.ioctl(device, 0x40044590, 1)  # EVIOCGRAB
        except OSError as error:
            if error.errno == errno.EBUSY:
                print("yes")
                return
            raise
        fcntl.ioctl(device, 0x40044590, 0)
        print("no")


def cmd_screen_locked(args):
    service, path = {
        "gnome": ("org.gnome.ScreenSaver", "/org/gnome/ScreenSaver"),
        "cinnamon": ("org.cinnamon.ScreenSaver", "/org/cinnamon/ScreenSaver"),
        "kde": ("org.freedesktop.ScreenSaver", "/ScreenSaver"),
    }[desktop()]
    print("yes" if busctl("call", service, path, service, "GetActive")[1] == "true" else "no")


def cmd_lock_screen(args):
    busctl("call", "org.freedesktop.ScreenSaver", "/ScreenSaver",
           "org.freedesktop.ScreenSaver", "Lock")


def cmd_game(args):
    result = app("game", args.mode)
    if result.returncode:
        fail(f"game {args.mode}: {result.stdout}{result.stderr}")


def cmd_fake_game(args):
    """A process named like a configured game, for game mode Auto."""
    unit = "tecloid-e2e-fake-game"
    if args.action == "stop":
        subprocess.run(["systemctl", "--user", "stop", unit], capture_output=True)
        return
    path = WORK / FAKE_GAME
    # Multicall coreutils dispatch by argv[0] and cannot run renamed as a game.
    shutil.copy2(sys.executable, path)
    systemd_run(unit, [str(path), "-c", "import time; time.sleep(600)"],
                WORK / "fake-game.log")


def cmd_command_marker(args):
    marker = WORK / "cmd-ran"
    if args.action == "reset":
        marker.unlink(missing_ok=True)
    else:
        wait(f"{marker} from cmd:touch", 10, marker.exists)


def cmd_text_failures(args):
    """Number of `text:` failures with `reason` in the application logs: an
    instance started by the stand writes APP_LOG, an autostarted one the journal."""
    text = APP_LOG.read_text(errors="replace") if APP_LOG.exists() else ""
    text += subprocess.run(["journalctl", "--user", "-b", "--no-pager", "-o", "cat"],
                           capture_output=True, text=True).stdout
    print(sum(1 for line in text.splitlines()
              if "text not typed" in line and args.reason in line))


def cmd_kill_app(args):
    subprocess.run(["pkill", "-KILL", "-u", str(os.getuid()), "-x", "tecloid"])
    wait("the killed process to exit", 10,
         lambda: subprocess.run(["pgrep", "-u", str(os.getuid()), "-x", "tecloid"],
                                capture_output=True).returncode != 0)


def cmd_resources(args):
    owners = processes()
    roots = [pid for pid in owners if b'--spell-worker' not in (Path('/proc') / pid / 'cmdline').read_bytes()]
    if len(roots) != 1:
        fail(f"expected one settings owner, found {len(roots)}")
    values = []
    for pid in owners:
        process = Path('/proc') / pid
        status = dict(line.split(':', 1) for line in (process / 'status').read_text().splitlines())
        values.append({"rss_kib": int(status['VmRSS'].split()[0]),
                       "fds": len(list((process / 'fd').iterdir())),
                       "threads": int(status['Threads'])})
    environment = dict(item.split('=', 1) for item in (Path('/proc') / roots[0] / 'environ').read_text().split('\0') if '=' in item)
    print(json.dumps({"pid": int(roots[0]), "pids": [int(pid) for pid in owners],
                      "popup_mode": environment.get('TECLOID_POPUPS', 'winit'),
                      **{field: sum(value[field] for value in values) for field in ('rss_kib', 'fds', 'threads')}}))


def cmd_clipboard(args):
    environment = {**os.environ, **user_environment()}
    if args.value is not None:
        subprocess.run(["wl-copy", "--type", "text/plain;charset=utf-8"],
                       input=args.value.encode('utf-8'), stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, env=environment, check=True)
    else:
        result = subprocess.run(["wl-paste", "--no-newline"], env=environment,
                                check=True, capture_output=True, text=True)
        print(result.stdout)


def cmd_suspend_count(args):
    """Completed suspend cycles in this boot's kernel log."""
    # Debian lets only root and the adm group read the system journal.
    log = subprocess.run(["sudo", "-n", "journalctl", "-k", "-b", "--no-pager", "-o", "cat"],
                         capture_output=True, text=True).stdout
    print(log.count("PM: suspend exit"))


def processes():
    return subprocess.run(["pgrep", "-u", str(os.getuid()), "-x", "tecloid"],
                          capture_output=True, text=True).stdout.split()


def virtual_keyboards():
    try:
        return Path("/proc/bus/input/devices").read_text().count(f'N: Name="{MAPPER_DEVICE}"')
    except OSError:
        return 0


def cmd_cli(args):
    """A second launch hands over to the running instance; client commands
    and --version/--help answer without starting another one."""
    before = processes()
    second = subprocess.run([binary()], capture_output=True, text=True, timeout=20)
    if second.returncode:
        fail(f"a second launch exited with {second.returncode}: {second.stderr}")
    time.sleep(2)
    if processes() != before:
        fail(f"a second launch left processes {processes()} (before {before})")
    for arguments in (("hide",), ("ping",), ("--version",), ("--help",)):
        result = app(*arguments)
        if result.returncode:
            fail(f"{' '.join(arguments)}: {result.stdout}{result.stderr}")
    print(app("--version").stdout.strip())


def cmd_second_instance(args):
    """Another instance (own IPC socket) must not take the keyboard while
    this one's mapper holds the cross-process lock."""
    unit = "tecloid-e2e-second"
    log = WORK / "second.log"
    log.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="second-", dir=WORK) as directory:
        home = Path(directory) / "linux"
        settings = home / ".config" / APP_ID
        keymaps = home / ".local/share" / APP_ID / "keymaps"
        settings.mkdir(parents=True)
        shutil.copyfile(CONFIG_DIR / "config.json", settings / "config.json")
        shutil.copytree(DATA_DIR / "keymaps", keymaps)
        systemd_run(unit, [binary()], log, [
            "TECLOID_SOCKET=tecloid-e2e-second",
            f"TECLOID_DEV_DIR={directory}",
            "TECLOID_HOTKEYS=off",
            "RUST_LOG=info",
        ])
        try:
            os.environ["TECLOID_SOCKET"] = "tecloid-e2e-second"
            wait("the second instance to answer", 30, lambda: app("ping").returncode == 0)
            time.sleep(5)
            if virtual_keyboards() != 1:
                fail(f"{virtual_keyboards()} mapper virtual keyboards with two instances")
            if "already active in another process" not in log.read_text(errors="replace"):
                fail("the second instance did not report the mapper lock; see second.log")
        finally:
            app("quit")
            del os.environ["TECLOID_SOCKET"]
            time.sleep(2)
            subprocess.run(["systemctl", "--user", "stop", unit], capture_output=True)
    if app("ping").returncode or not mapper_running():
        fail("the first instance or its mapper stopped")


def cmd_check_removed(args):
    if shutil.which("tecloid") or (HOME / ".local/bin/tecloid").exists():
        fail("the binary is still installed")
    if not (CONFIG_DIR / "config.json").exists():
        fail("the user configuration was removed with the package")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("wait-session")
    p.add_argument("--group")
    p.add_argument("--x11", action="store_true", help="Cinnamon X11 session instead of Wayland")
    p.add_argument("--timeout", type=float, default=300)
    p = sub.add_parser("check-install")
    p.add_argument("--packaged", action="store_true")
    p = sub.add_parser("probe-keyboard")
    p.add_argument("--timeout", type=float, default=20)
    sub.add_parser("tablet-grabbed")
    p = sub.add_parser("write-config")
    p.add_argument("--capture-tablet", action="store_true")
    p.add_argument("--device", required=True)
    p.add_argument("--text-backend", choices=("libei", "clipboard", "xdotool", "gnomeCommitText"), default="libei")
    p.add_argument("--popup-mode", choices=("winit", "auto", "spell"), default="winit")
    p = sub.add_parser("check-keymap")
    p.add_argument("--device", required=True)
    p = sub.add_parser("start-app")
    p.add_argument("--ydotool-socket")
    p.add_argument("--popups", default="auto")
    p.add_argument("--timeout", type=float, default=30)
    p = sub.add_parser("wait-app")
    p.add_argument("--absent", action="store_true")
    p.add_argument("--timeout", type=float, default=60)
    sub.add_parser("quit-app")
    sub.add_parser("resources")
    p = sub.add_parser("clipboard")
    p.add_argument("--value")
    p = sub.add_parser("mapper")
    p.add_argument("--absent", action="store_true")
    p.add_argument("--timeout", type=float, default=20)
    p = sub.add_parser("set-text-backend")
    p.add_argument("--ydotool-path")
    p.add_argument("mode", choices=("libeiOnly", "libeiText", "portalKeycode", "clipboardPaste",
                                    "gnomeCommitText", "ydotool", "xdotool"))
    sub.add_parser("toggle-mapper")
    p = sub.add_parser("set-mapper")
    p.add_argument("state", choices=["on", "off"])
    p = sub.add_parser("tray")
    p.add_argument("--timeout", type=float, default=20)
    sub.add_parser("autostart")
    p = sub.add_parser("start-receiver")
    p.add_argument("--name", default="main")
    p.add_argument("--xwayland", action="store_true")
    p.add_argument("--keep-existing", action="store_true")
    p.add_argument("--tagged", action="store_true")
    sub.add_parser("receiver-state").add_argument("--name", required=True)
    sub.add_parser("start-terminal-receiver").add_argument("--name", default="terminal")
    sub.add_parser("terminal-receive").add_argument("--name", required=True)
    sub.add_parser("focus-terminal-receiver")
    p = sub.add_parser("receive")
    p.add_argument("--title-suffix", default="")
    p.add_argument("--name", required=True)
    p = sub.add_parser("receiver-result")
    p.add_argument("--name", default="main")
    p.add_argument("--expect")
    p.add_argument("--timeout", type=float, default=10)
    sub.add_parser("popups")
    p = sub.add_parser("popup-count")
    p.add_argument("popup", choices=("quick", "emoji"))
    p.add_argument("event", choices=("t3_first_frame", "t4_focused", "hidden"))
    p = sub.add_parser("popup-wait")
    p.add_argument("popup", choices=("quick", "emoji"))
    p.add_argument("event", choices=("t3_first_frame", "t4_focused", "hidden"))
    p.add_argument("--after", type=int, required=True)
    p.add_argument("--timeout", type=float, default=15)
    sub.add_parser("kde-state")
    sub.add_parser("focus-receiver")
    sub.add_parser("portal-visible")
    p = sub.add_parser("portal-deny-point")
    p.add_argument("--x", type=float, required=True)
    p.add_argument("--y", type=float, required=True)
    sub.add_parser("x11-state")
    sub.add_parser("x11-layouts").add_argument("--configure", action="store_true")
    sub.add_parser("x11-popup").add_argument("popup", choices=("quick", "emoji"))
    sub.add_parser("screen-locked")
    sub.add_parser("lock-screen")
    p = sub.add_parser("game")
    p.add_argument("mode", choices=["auto", "on", "off"])
    p = sub.add_parser("fake-game")
    p.add_argument("action", choices=["start", "stop"])
    p = sub.add_parser("command-marker")
    p.add_argument("action", choices=["reset", "check"])
    p = sub.add_parser("text-failures")
    p.add_argument("--reason", default="")
    sub.add_parser("kill-app")
    sub.add_parser("suspend-count")
    sub.add_parser("cli")
    sub.add_parser("second-instance")
    sub.add_parser("check-removed")
    args = parser.parse_args()
    globals()["cmd_" + args.command.replace("-", "_")](args)


if __name__ == "__main__":
    main()
