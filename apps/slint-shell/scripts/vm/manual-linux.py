#!/usr/bin/env python3
"""Start guided KDE or GNOME VM acceptance with the current release package."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


REPO = Path(__file__).resolve().parents[4]
STANDS = {"fedora": ("tecloid-fedora-kde", "tecloid-{version}-1.x86_64.rpm"),
          "debian": ("tecloid-debian13-kde", "tecloid_{version}_amd64.deb"),
          "gnome": ("tecloid-ubuntu2604-gnome", "tecloid-{version}-linux-x86_64.tar.gz")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stand", choices=STANDS)
    parser.add_argument("--vm-root", type=Path, help="directory containing the prepared Tecloid VM stands")
    parser.add_argument("--check", action="store_true", help="validate and print selected paths without booting")
    parser.add_argument("--build", action="store_true",
                        help="build all Linux release packages in an ubuntu:22.04 Docker container before booting")
    args = parser.parse_args()
    if args.build and args.check:
        parser.error("--build cannot be combined with --check")
    root = args.vm_root or os.environ.get("TECLOID_VM_ROOT")
    if not root:
        config = REPO / "dev-files/vm/manual.json"
        if config.is_file():
            root = json.loads(config.read_text())["vm_root"]
    if not root:
        parser.error("set --vm-root or TECLOID_VM_ROOT; see docs/testing.md")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"], cwd=REPO))
    version = next(p["version"] for p in metadata["packages"] if p["name"] == "slint-shell")
    directory, filename = STANDS[args.stand]
    vm = Path(root).expanduser().resolve() / directory
    package = Path(metadata["target_directory"]) / "release-artifacts" / filename.format(version=version)
    if not (vm / "vm.json").is_file():
        parser.error(f"{vm / 'vm.json'} missing; prepare the stand")
    config = json.loads((vm / "vm.json").read_text())
    base = vm / config.get("base", "base.qcow2")
    if not base.is_file():
        parser.error(f"{base} missing; prepare the stand")
    if not Path(config["ssh_key"]).expanduser().is_file():
        parser.error("VM SSH key is missing")
    if args.build:
        print("Building Linux release packages in ubuntu:22.04 (Docker)…", flush=True)
        subprocess.run([str(REPO / "apps/slint-shell/scripts/release/release-linux-docker.sh")],
                       cwd=REPO, check=True)
    if not package.is_file():
        parser.error(f"{package} missing; use --build or build packages as described in docs/release.md")
    runner = "run-gnome.py" if args.stand == "gnome" else "run-linux.py"
    command = [sys.executable, str(Path(__file__).with_name(runner)),
               "--manual", "--vm", str(vm), "--package", str(package)]
    if args.stand != "gnome":
        command += ["--popup-mode", "auto"]
    print(f"VM: {vm}\nPackage: {package}", flush=True)
    if not args.check:
        os.chdir(REPO)
        os.execv(sys.executable, command)


if __name__ == "__main__":
    main()
