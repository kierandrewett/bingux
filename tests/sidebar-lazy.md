# Retained sidebar regression

Run inside a native Wayland session with Bingux's Quickshell build and QML
plugins available:

```sh
python3 tests/sidebar-lazy.py --output /tmp/sidebar-lazy
python3 tests/sidebar-lazy.py --selection notes --restore
python3 tests/sidebar-lazy.py --selection monitor --restore --detached
python3 tests/sidebar-lazy.py --placement top-right
python3 tests/sidebar-lazy.py --placement control-centre
```

`QUICKSHELL_BIN` selects the executable. `--source` selects a shell source
directory, including an installed shell, for baseline/candidate comparisons.
`--selection` accepts terminal, notes, monitor, calendar, media or tasks.
`--detached` also covers a saved detached-but-closed sidebar without `--restore`.

The runner copies the actual shell and isolates preferences in a temporary
directory. The fixture checks absent closed content, saved restoration, first
rendering, source registration, live metrics binding, real keyboard input and
retained editor identity through hide, reopen, detach and dock. Placement cases
exercise first popup clicks and visible inline content with a small layout host.
They do not prove full desktop-editor drag placement or hardware rendering.

`--output` retains the runtime log and verdict. A baseline with eager notes and
monitor fails the closed-content assertion. First-use render durations in the
log include fixture scheduling; they are not login timings.
