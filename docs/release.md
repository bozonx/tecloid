# Release

## Flow

1. **While working:** every user-visible change adds an entry under
   `## [Unreleased]` in `CHANGELOG.md` in the same commit.
2. **Before tagging:** run L3 and L4 ([testing.md](testing.md#before-a-release)).
3. **Tag:** `cargo release <minor|patch>` previews; `--execute` bumps
   `workspace.package.version` and `Cargo.lock`, renames `Unreleased` to
   `## [X.Y.Z] - <UTC date>`, adds `<release>` to the metainfo, runs
   `release-metadata.py`, commits `Release X.Y.Z`, tags `vX.Y.Z` and pushes.
   Only from a clean `main`. Asking first is required (public release).
4. **CI:** the tag triggers `.github/workflows/release.yml`, which produces a
   **draft** release.
5. **Accept locally:** download the draft's artefacts, `sha256sum -c SHA256SUMS`,
   run L4 (`run-linux.py` / `run-linux-matrix.py`) on exactly these files.
   VM acceptance is local only; there is no CI job for it.
6. **Publish** the draft. Optionally paste the short L4 summary (commit, VMs,
   PASS/FAIL/SKIP) into the release notes; see [Acceptance results](#acceptance-results).

If `release.yml` fails after the tag was pushed, fix with a normal commit and
release the next patch. Never move a published tag; an unpublished tag may be
deleted and recreated.

Never edit the version in `Cargo.toml`, the CHANGELOG section header or the
metainfo `<release>` by hand. Configuration: `release.toml`,
`[package.metadata.release]` in `apps/slint-shell/Cargo.toml`
(`vendor/spell-framework` has `release = false`).

## Compatibility gate

Before every release check that nothing breaks existing users:

- `config.json` and keymap files of every released `version` still load
  (fixture tests), and no field, enum value or action id changed meaning;
- schemas in `schemas/` are current (`published_schemas` test);
- CLI commands and the IPC wire format are unchanged or only extended;
- an upgrade keeps `config.json` byte-identical (`run-linux.py --previous`).

See [config-format.md](config-format.md#compatibility-contract).

## CHANGELOG

[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Entries go under
`### Added`, `### Changed`, `### Fixed`, `### Removed`, in English, from the
user's point of view. Internal refactors, tests and CI are not listed. Behaviour
changes of defaults, removed or renamed features and format version bumps must
be called out explicitly.

## Artefacts

| File | Contents |
| --- | --- |
| `tecloid-<ver>-linux-x86_64.tar.gz` | binary, `install-linux.sh`, GNOME extension + installer, desktop file, metainfo, udev rule, icons, `LICENSE`, `CHANGELOG.md`, `README.md` (`packaging/linux/README.md`) |
| `tecloid_<ver>_amd64.deb` | Debian 12+, Ubuntu 22.04+ |
| `tecloid-<ver>-1.x86_64.rpm` | Fedora, openSUSE |
| `tecloid-<ver>-vendor.tar.xz` | `cargo vendor` of registry crates for offline builds and GPL sources |
| `SHA256SUMS` | Linux checksums |

Releases are Linux-only for now. The Windows ZIP (`tecloid.exe`, `LICENSE`,
`README.md`, plus `WINDOWS-SHA256SUMS`) is built only locally with
`package-windows.ps1`; `check-release-artifacts.py` checks it when present and
`--require-windows` makes it mandatory.

`vendor.tar.xz` contains only crates.io dependencies. The path crates
(`vendor/i-slint-core`, `vendor/spell-framework`) are part of the source tree, so
an offline build extracts the tag's source archive and then `vendor.tar.xz` over it.

User documentation is not shipped in packages; it lives on the documentation
site. Packages install the udev rule and `uinput` autoload but never add the user
to group `input`; removal never touches user configuration.

Linux device permissions use logind uaccess for keyboards, mice and touchpads
in the active local session, without adding users to input or requiring a new
login. This grants access to every active local user and does not prevent
keylogging by processes in that session. The input group remains an explicit
fallback for systems without logind and use outside an active local session.

## Building locally

```sh
apps/slint-shell/scripts/release/package-linux.sh   # tar.gz for this machine → target/packages/
apps/slint-shell/scripts/release/release-linux.sh   # all Linux artefacts → target/release-artifacts/ (needs nfpm)
```

Portable Linux artefacts must be built in `ubuntu:22.04` like the workflow:

```sh
docker run --rm -it -v "$PWD":/src -w /src ubuntu:22.04 bash
# inside: system dependencies, rustup stable, nfpm 2.47.0, then release-linux.sh
docker run --rm -v "$PWD":/src -w /src fedora:latest \
  apps/slint-shell/scripts/release/smoke-linux-package.sh target/release-artifacts/<file>.rpm
```

Smoke images: `debian:12`, `debian:13`, `ubuntu:22.04`, `ubuntu:24.04`,
`fedora:latest`, `opensuse/tumbleweed`, `archlinux` (tar.gz).

Windows (x64 MSVC):

```powershell
powershell -NoProfile -File apps/slint-shell/scripts/release/package-windows.ps1
powershell -NoProfile -File apps/slint-shell/scripts/windows/check-windows-package.ps1 -Archive target/release-artifacts-windows/tecloid-<ver>-windows-x86_64.zip -Desktop
```

PNG icons are generated by `apps/slint-shell/scripts/render-icons.sh` from
`assets/icon.svg`.

## release.yml

1. `verify` — calls `ci.yml` with `linux-only: true` (fmt, clippy, tests,
   non-Spell build, UI scenarios, MSRV, mapper e2e, cargo-deny; no macOS/Windows).
2. `metadata` — tag equals the workspace version; CHANGELOG has
   `## [<ver>] - <date>`; first metainfo `<release>` is this version.
3. `build-linux` (`ubuntu:22.04`) — `release-linux.sh`, `ldd`, lintian, checksums.
4. `rpmlint` and `smoke-packages` — install/remove in seven containers.
5. `verify-artifacts` — `check-release-artifacts.py --require-vendor`.
6. `draft-release` — checksum verification, build attestation, draft with notes
   from CHANGELOG. Only this job has write permissions.

A manual `workflow_dispatch` builds and checks without creating a release.

The Rust toolchain in every workflow is read from `rust-toolchain.toml`; bump it
only there. MSRV (`1.92` in `ci.yml`) follows `rust-version` in `Cargo.toml`.

## Acceptance results

L3/L4 run locally. Full logs, screenshots and VM runs stay outside Git
(`<vm>/runs/`, `target/test-results/`) and may be deleted once the release is
published or the defect is closed. The durable record is the short summary in
the GitHub release notes; it is not committed to the repository.
