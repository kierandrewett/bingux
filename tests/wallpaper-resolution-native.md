# Native wallpaper resolution regression

`wallpaper-resolution-native.py` compares the real native wallpaper surface
with a deterministic RGB PNG at the guest's physical output size. Run it only
inside a disposable QEMU/KVM guest with Bingux and Gnoblin already running.
The runner stops the single managed `WallpaperShell.qml` process, starts one
fixture Quickshell process, captures both wallpaper choices, and restarts guest
GDM during cleanup so the normal session returns.

The guest must have `grim`, Pillow, `gnoblinctl`, `systemd-detect-virt`, and
passwordless `sudo systemctl restart gdm`. The runner checks that exact sudo
command during preflight. The shell process must inherit its
active `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY`, and Wayland socket environment.
Set `BINGUX_DISPOSABLE_QEMU_GUEST=1` in the guest to opt in. The runner also
checks QEMU/KVM virtualization and guest DMI identity before stopping any
process or restarting GDM.

Pass the absolute Quickshell executable path used by the managed wallpaper. By default, the runner reads the
worktree source; `--source` can select another `Wallpaper.qml` file. Supply an
artifact directory on the guest; captures, fixture PNGs, the child log, and
`result.json` are retained there, including on failure.

```sh
cd /path/to/bingux
BINGUX_DISPOSABLE_QEMU_GUEST=1 \
  python3 tests/wallpaper-resolution-native.py \
  --quickshell /absolute/path/to/quickshell \
  --artifact-dir /path/to/retained/wallpaper-evidence
```

The runner records the source SHA-256, logical monitor dimensions and scale,
derived physical dimensions, actual `grim` dimensions, both process
identities, comparison region, and maximum per-channel RGB error. It fails if
logical dimensions multiplied by scale disagree with the initial `grim` capture,
the stable region is too small, the capture is blank, or either image exceeds
the two-level RGB tolerance. Its comparison excludes at least the top 240 pixels and bottom 160 pixels.
These margins grow with monitor scale to cover 160 logical pixels at the top
and 80 at the bottom; it also excludes the rightmost 32 logical pixels. This
avoids shell UI and the native recovery notice shown after stopping the managed
wallpaper process.

The runner verifies only the guest's current native configuration and does
not configure display scaling. Repeat it at the display scales under review.
Use `--expected-scale 1.25`, for example, to reject a session that has returned
to 100% scale after restarting GDM.
It checks settled image pixels after the initial reveal and after changing the
wallpaper through IPC; it does not measure animation pacing or first paint.

On Wayland, Qt's screen device-pixel ratio can round a fractional monitor scale
up to an integer. `Wallpaper.qml` uses the window's device-pixel ratio instead,
so the decode size follows that surface's fractional scale. A native 125%
trace showed `preferred_scale(150)` and `preferred_buffer_scale(2)` on the
wallpaper surface, with window ratio 1.25 and screen ratio 2. The screen ratio
produced a 2048×1280 decode for a 1280×800 output; the window ratio produced
1280×800. Both settled wallpaper choices matched the source at 100%, 125%, and 200%
in the disposable guest. These checks do not prove other outputs or host
behavior.

The private fixture directory is removed after guest GDM restarts. The
explicit artifact directory is retained for review.
