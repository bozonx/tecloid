# Emoji popup insertion latency

Measured on 2026-11-27 in disposable overlays of the repository's KVM stands,
with Tecloid 0.4.7 and the default Emoji page. Each main series uses one
unmeasured warm-up and 20 measured insertions of the exact string `🙂`.

| VM | Popup | Text method | Samples | Min | Median | p95 | Max | Median to confirmed target |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Fedora 44, KWin 6.7.5, Wayland | winit | `clipboardPaste` | 20 | 129.59 ms | 139.60 ms | 154.18 ms | 157.71 ms | 5.52 ms |
| Linux Mint 22.3, Cinnamon 6.6.4, X11 | winit | `xdotool` | 20 | 29.39 ms | 74.05 ms | 104.45 ms | 123.18 ms | 8.42 ms |
| Ubuntu 26.04, GNOME Shell 50.1, Wayland | winit | `gnomeCommitText` | 20 | 23.18 ms | 30.07 ms | 35.74 ms | 47.38 ms | 12.02 ms |

The host timestamps the QMP key-down request for the Emoji cell and the arrival
of the GTK entry's change notification over an already-open SSH stream. This
includes virtual keyboard delivery, guest scheduling, and notification
transport. The confirmed-target column comes from Tecloid's guest log and starts
at the popup's selection callback. p95 uses the nearest-rank method. The
warm-up excludes first-use RemoteDesktop consent from the timings.

On KDE and Cinnamon the median to confirmed focus was under 10 ms.
The Fedora clipboard series still took about 140 ms end to end. These samples
describe the current build. A matching before-change run would be needed to
quantify the improvement.

## Same-VM comparison with immediate follow-up typing

On Fedora 44 with KWin 6.8.3 and libei 1.6.0, the winit popup ran two
20-sample series with a physical `c` synthesized by QEMU 25 ms after
selection. Both modes used the same guest, receiver, and executable.

| Text method | Min | Median | p95 | Max | Median to confirmed target | Final field contents |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `libeiText` | 12.23 ms | 29.03 ms | 52.04 ms | 53.23 ms | 5.85 ms | `🙂c` in 20/20 |
| `clipboardPaste` | 131.29 ms | 137.05 ms | 162.68 ms | 174.83 ms | 6.49 ms | `c🙂` in 20/20 |

The median from the guest's popup choice to EIS send was 6.15 ms.
The direct mode reduced the measured median by 108.02 ms compared with
clipboard insertion in this run. The order result applies to this delay
and fixture. It does not guarantee order at shorter intervals or in other apps.
Artifacts are in the Fedora 6.8 overlay run `20261127-102246`.

In a separate Fedora 6.7.5 Spell run (`20261127-100540`), sending `c` after
25 ms produced `c🙂` in the one completed measured trial. The runner then
timed out waiting for the receiver's focus notification on the next opening.
The sample confirms that a key can overtake clipboard insertion. It does not
establish a frequency. The runner now waits for Tecloid's popup-focus metric.

Windows and macOS were outside these Linux runs.

## Reproducing the measurement

Use the package for the VM's integration files and the portable binary for the
revision being tested. For example, from the repo root:

```sh
python3 apps/slint-shell/scripts/vm/bench-popup-text.py \
  --vm /mnt/disk2/vm/tecloid-plasma6-fedora44 \
  --package target/release-artifacts/tecloid-0.4.7-1.x86_64.rpm \
  --binary target/release-container/target/release/tecloid \
  --mode clipboardPaste --trials 20
```

The runner writes `popup-text.json`, the guest environment, the binary SHA-256,
and the Tecloid journal under the VM's `runs/<timestamp>/logs/`. The runs
above were `20261127-100315`, `20261127-100417`, and `20261127-100802`, with binary SHA-256
`bc9c3ec7872601dd34af1cd8f67a355b9612c63f3e8df7df0248c15aa326e6a1`.

`--popups spell` selects the layer-shell popup. `--followup-ms 25` sends a
`c` 25 ms after sending the cell selection and records the resulting field
contents. `--x11` selects the X11 guest session and receiver. For a Fedora
stand with older KDE packages, `--upgrade-kde` updates KDE and libei inside
the disposable overlay before booting back into the desktop.
