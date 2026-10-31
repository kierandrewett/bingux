# Testing the desktop

Bingux has one test framework for the desktop. It starts the real stack (Gnoblin, the Bingux shell and its
services) in a private sandbox, drives it with keyboard and pointer input, and reads the result from the screen,
from Gnoblin's control socket and from the logs. It does not touch the session you are logged in to.

```sh
make test-desktop                       # everything that is not marked slow
make test-desktop-fast                  # only what needs no compositor and no built program
make test-desktop ARGS="session -k dock"
make test-desktop ARGS="-m slow"        # the probes that need a quiet machine
```

`make check` runs `check-static` and `test-desktop-fast` first, then builds and tests the rest. `make check-static`
runs the checks that build nothing. The framework lives in `tests/desktop/`.

## What it proves and what it does not

It proves that, with the Gnoblin, Quickshell and Bingux builds it found (the run writes them to
`stack.json`), a session boots, the shell draws its bar, wallpaper and dock, a client window gets a dock icon,
Settings opens a page and a subpage, a notification is drawn, and a test can type and click. It proves that none of
this reaches your desktop, session bus, home directory or autostart programs.

It does not prove anything that needs hardware or a service the sandbox does not have:

- Rendering is in software on one virtual 1280x720 monitor. Animation smoothness, GPU paths, multiple monitors,
  scaling and monitor hot-plug are not tested. (`--gpu` exposes `/dev/dri` to the session; no test depends on it.)
- There is no PipeWire, NetworkManager, BlueZ, UPower, logind or colord. Audio, networking, Bluetooth, battery,
  suspend and screen recording are not tested here. Settings pages that need them run on the fakes the app
  provides (`BINGUX_SETTINGS_FAKE_*`).
- Input is synthetic: Mutter's RemoteDesktop interface injects key and pointer events. Touch, tablets, libinput
  configuration and real keyboards are not tested.
- The login manager, the greeter and bsm (the session manager) are not started. Their tests need a VM.
- The sandbox shares the host kernel, `/usr` and `/etc` (read-only). It keeps a test from reaching your session;
  it is not a defence against hostile code.

## How a session is isolated

`Session` (`tests/desktop/bxdesktop/session.py`) runs everything inside one bubblewrap sandbox
(`bxdesktop/sandbox.py`, a pure function covered by `fast/test_sandbox.py`):

| Host thing                                 | What the session gets                                                            |
| ------------------------------------------ | -------------------------------------------------------------------------------- |
| Namespaces                                 | New user, mount, PID, IPC, UTS, cgroup and network namespaces (`--unshare-all`)  |
| Network                                    | A loopback only. Abstract sockets, X11 sockets and host services are unreachable |
| Environment                                | Cleared. The session sets every variable itself; nothing is inherited            |
| `WAYLAND_DISPLAY`, `DISPLAY`               | Not set for the compositor. Clients get `bx-0`, Gnoblin's own display            |
| `DBUS_SESSION_BUS_ADDRESS`                 | A private `dbus-daemon` inside the sandbox, with a short list of services        |
| `XDG_RUNTIME_DIR`                          | A directory of the run's temporary root, mounted at `/run/user/UID`              |
| `HOME`                                     | A directory of the run's temporary root. Nothing of your home is visible         |
| `/etc/xdg` (autostart entries)             | An empty tmpfs: no host autostart program can start                              |
| `/usr`, `/etc`, the checkout, the prefixes | Read-only at their own paths                                                     |
| Everything else                            | Not present. `/tmp`, `/var`, `/run` are tmpfs or the run's own directories       |
| Processes                                  | `--die-with-parent`: killing the test run kills the sandbox                      |

The temporary root is a directory under `$BINGUX_TEST_TMP` (default `$TMPDIR`). `stop()` removes it. Logs and
screenshots go to the artefact directory, `build/desktop-test/<run>/desktop/` (or `--artifacts DIR` /
`$BINGUX_TEST_ARTIFACTS`), which holds `bwrap-command.txt` (the exact sandbox command), `stack.json` (which
Gnoblin, Quickshell and Bingux builds ran), `logs/` and `screenshots/`. A test that fails also saves a
`FAILED-<test>.png`.

`session/test_isolation.py` holds the proof. From inside the sandbox it shows that the environment contains only
what the harness set; that decoy values for `WAYLAND_DISPLAY`, `DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`,
`SSH_AUTH_SOCK`, `SWAYSOCK`, `LD_PRELOAD` and others set in the test process do not arrive; that the host Wayland
display and session bus are not reachable (a different device and inode for the runtime directory and bus
socket, a different bus ID, no host services on the bus); that the host home is not visible; that `/etc/xdg` is
empty and no host autostart program is running while Gnoblin's own autostart list is; that the only network
interface is a loopback; and that writes land only in the run's temporary root. A whole run is bracketed by a
snapshot of the host's desktop processes, which must be unchanged afterwards, with every process of the
session's PID namespace gone and its root removed. `session/test_lifecycle.py` shows that stopping ends every
process, that a failed start leaves nothing behind, and that `SIGKILL` of the test run takes the sandbox with it.

## How Gnoblin runs without a parent display

`gnoblin` starts a compositor only as `gnoblin --devkit --wayland-display NAME`. In that mode Mutter launches a
preview window (`mutter-devkit`, a GTK application) on a parent Wayland display and ends the session when the
window exits. The sandbox has no parent display, so the window fails to open (`Gtk-WARNING Failed to open
display`). Mutter checks `GNOBLIN_DEVKIT_KEEP_SESSION=1` when the window exits (`src/core/meta-mdk.c`) and keeps
the session running. The compositor keeps its virtual 1280x720 monitor, `grim` reads it, and no second
compositor is needed. `GNOME_DEVKIT_HEADLESS`, which two Gnoblin scripts mention, is not read by any code in
Mutter or Gnoblin. The two log lines this produces are in the allowed list of `bxdesktop/logs.py`, with that
reason.

Gnoblin loads `~/.config/gnoblin/init.lua` from the run's home. The harness writes one line there that loads this
checkout's `gnoblin/bingux.lua`, so Gnoblin's own autostart list starts the wallpaper, the shell and the popouts
from the checkout's `shell/bingux`. The commands that file names are small launchers the harness writes to the
sandbox's `/tmp/bin`, as the packaged ones would.

## What it needs

| Piece                                              | Used for                                                                     | Set by                                                                                     |
| -------------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Gnoblin prefix                                     | `bin/gnoblin`, `bin/gnoblinctl`, `libexec/gnoblin-env.sh`                    | `GNOBLIN_PREFIX` (default `/usr/local/lib/gnoblin`, `/usr/lib/gnoblin`)                    |
| Quickshell                                         | The shell                                                                    | `BINGUX_QUICKSHELL`, else the prefix's `bingux-shell`, else `quickshell` or `qs` on `PATH` |
| A Bingux prefix                                    | The native QML plugins (`lib/bingux/qml`), `bingux-statusd`, `bingux-config` | `BINGUX_TEST_PREFIX` (default `build/test-prefix`, `~/.local/share/bingux`)                |
| Cargo target directory                             | Settings, Files, statusd, config                                             | `BINGUX_TEST_CARGO_TARGET` or `CARGO_TARGET_DIR`                                           |
| `bwrap`, `nsenter`, `dbus-daemon`, `grim`, `gdbus` | The sandbox, commands in it, screenshots                                     | `PATH`                                                                                     |
| PyGObject                                          | The input helper (`bxdesktop/inputd.py`)                                     | the system Python                                                                          |
| Pillow, numpy, pytest                              | Screenshots and the runner                                                   | `pip install pytest pillow numpy`                                                          |
| Tesseract (optional)                               | Reading text from the screen (clock, Settings, toasts)                       | `PATH`; tests that need it skip without it                                                 |

The shell's QML and Gnoblin module come from the checkout, so a test exercises the change under review.
Compiled pieces (Quickshell, the QML plugins, the Rust programs) come from whatever build was found, which may be
older than the checkout: read `stack.json`. A missing piece skips the desktop tests and names itself and the
variable that sets it; `--require-desktop` (CI sets it) turns the skip into a failure. A session that does not
start with a complete stack is always a failure.

A busy machine needs longer waits, not different tests: `BINGUX_TEST_TIMEOUT_SCALE=3` triples every
`wait_until`.

## Layout and markers

```text
tests/desktop/
  bxdesktop/            the harness: stack.py, sandbox.py, session.py, inputd.py, imaging.py, logs.py,
                        hostguard.py, keysyms.py, legacy.py
  fast/                 tests of the harness itself        marker: fast
  apps/                 headless app probes                markers: fast, app, slow
  session/              the isolated desktop               marker: needs_gnoblin
  legacy.toml           where every older script goes (see below)
  conftest.py  pytest.ini
```

| Marker           | Meaning                                                                                |
| ---------------- | -------------------------------------------------------------------------------------- |
| `fast`           | No compositor and no built Bingux program. Runs anywhere with Python, Pillow and numpy |
| `app`            | Runs a built Bingux app in its headless `--snapshot` mode. Needs the Rust binary       |
| `needs_gnoblin`  | Starts the isolated desktop                                                            |
| `needs_hardware` | Can only be verified on a real machine; skipped unless `--hardware` is given           |
| `slow`           | Needs a quiet machine or more than about a minute; deselected unless `-m slow`         |

The session tests share one desktop (starting it takes a few seconds). The `desktop` fixture hands it over idle:
no client windows, shell panels closed, the first-run card dismissed, the pointer over bare wallpaper. A test that
leaves an error line in the session logs fails at teardown with the line.

## Writing a test

```python
import pytest

pytestmark = pytest.mark.needs_gnoblin


def test_a_terminal_gets_a_dock_icon(desktop):
    client = desktop.launch(["foot", "sleep", "600"], title="foot")   # waits for the window
    apps = desktop.wait_until(lambda: desktop.shell_json("actions", "dock", "list", "", "0")["apps"], 15, "the dock")
    assert apps[0]["id"] == "foot"
    desktop.screenshot("dock")                                         # kept in the artefact directory
```

| Need                    | Call                                                                                                                                     |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Start a program         | `desktop.launch(argv, title=..., env=...)` (waits for its window); `spawn` does not wait; `run` finishes first                           |
| List and manage windows | `desktop.windows()`, `desktop.gnoblinctl("window", "move", id, "20", "60")`; also `layers()`                                             |
| Ask the shell           | `desktop.shell_json("shell", "panel", "search", "status")` (Quickshell IPC, `binguxctl list` names the targets)                          |
| Keys and pointer        | `key("Alt_L", "Tab")`, `type("text")`, `click(x, y)`, `move`, `scroll`, `hover`, `wait_focused`                                          |
| Look                    | `screenshot(name)` keeps a PNG, `capture()` does not; `Screenshot.changed_fraction`, `is_blank`, `distinct_colours`, `find_text`, `text` |
| Wait                    | `wait_until(check, timeout, what)`, `settled(region)`; never `sleep` for a result                                                        |
| Logs                    | `log_report(since=cursor).assert_clean()`; add expected lines to `bxdesktop/logs.py` with a reason                                       |

Assert on what a person would see or what the shell reports, not on pixel values of a particular wallpaper.
Compare a region with a screenshot taken before (`changed_fraction`), read text with `find_text`, and keep
coordinates out of tests where a text or a layer geometry can name the place.

Two things the harness has learned the hard way, both handled in `Session` and worth knowing:

- Mutter drops the first few key events of a new RemoteDesktop session. The harness sends four harmless Shift taps
  before it hands over.
- A toolkit that is still starting takes the first pointer presses and drops them. `hover(x, y, region)` waits
  until the screen reacts to the pointer; use it before a click on a freshly opened app.

## Log policy

`bxdesktop/logs.py` sorts every log line that looks wrong into one of three classes. `error` fails the test
that was running. `allowed` is expected in this sandbox and carries its reason (no GPU, no PipeWire, no parent
display); it is counted, not dropped. `known` is a real defect that is understood: it does not fail the run, every
run lists it, and an entry that stops matching is reported as stale. There is no rule without a reason.

## Coverage

Status is what the suites do today. "Covered" names the tests; "partly" says what is missing; "not yet" is work
that can be done with this framework; "hardware" can only be verified on a real machine or in a VM.

| Area                          | Assertion style                                                                | Fixture                             | Status             | Where / what is missing                                                                                                                                                                                                |
| ----------------------------- | ------------------------------------------------------------------------------ | ----------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Boot: bar, wallpaper, dock    | Layer geometry (`gnoblinctl layer list`), screenshot regions, OCR of the clock | `desktop`                           | Covered            | `session/test_boot.py`                                                                                                                                                                                                 |
| Shell panels (bar popups)     | IPC status, screenshot change                                                  | `desktop`                           | Partly             | `session/test_panels.py` opens and closes search, calendar, Control Centre and notification centre. Their contents are not asserted; metrics, keyboard menu, tray, sidebar not yet                                     |
| Dock                          | IPC list, screenshot of the dock strip                                         | `desktop`, `launch`                 | Partly             | `session/test_dock.py`: icon appears, windows counted, icon leaves. Pinning, folders, menus, media, badges, launch errors not yet (legacy `dock-*`)                                                                    |
| Search                        | IPC status, keys                                                               | `desktop`                           | Partly             | `session/test_input.py`: Super opens it, Escape closes it. Results need `bingux-searchd` with a config (the overlay shows "Offline"); not started                                                                      |
| Bar and dock over fullscreen  | Layer names, bar screenshot, clicks logged by foot                             | `desktop`, `launch`                 | Partly             | `session/test_fullscreen_chrome.py`: a click into the window, or activating another window, ends the reveal after Super hides search; the panels stay clickable; a windowed app gets its first click                   |
| Settings, in the desktop      | OCR of page text, click, screenshot diff                                       | `desktop`, `launch`, fakes          | Partly             | `session/test_settings.py`: opens on Network, a row opens the Wi-Fi subpage, Back returns. Other pages, values and saving not yet                                                                                      |
| Settings, headless            | Frame sequences, layout probes                                                 | `settings_binary` (`--snapshot`)    | Partly             | `apps/`: scroll continuity runs by default; layout stability and page slides are `slow`; the rest of `scripts/` (layout-lint, overflow, state, pixel, copy, focus ring, tooltip) is run by `make check`, not by pytest |
| Files, Taxonomy               | Same probes as Settings                                                        | `--snapshot` binaries               | Not yet            | Covered only by `make check` through `scripts/`                                                                                                                                                                        |
| Notifications                 | D-Bus `Notify`, OCR of the toast                                               | `desktop`                           | Partly             | `session/test_notifications.py`: server identity, toast drawn and closed. History, gestures, do-not-disturb, actions, images not yet (legacy `notification-*`)                                                         |
| OSD (volume, brightness)      | Screenshot after a compositor event                                            | needs a compositor peer             | Not yet            | Legacy `osd-native-event.py` and `osd-event-bridge.py` send the event from a test peer                                                                                                                                 |
| Audio                         | —                                                                              | needs PipeWire                      | Hardware           | No PipeWire in the sandbox. `audio-meter*`, `audio-osd-feedback` are legacy                                                                                                                                            |
| Network, Bluetooth, battery   | —                                                                              | needs NetworkManager, BlueZ, UPower | Hardware           | Settings pages run on fakes (headless and in the desktop); real daemons are not reachable by design                                                                                                                    |
| Capture (screenshot, record)  | —                                                                              | needs PipeWire and portals          | Not yet / hardware | The capture UI could be driven; recording needs PipeWire. Legacy `capture-*`                                                                                                                                           |
| Window management             | `gnoblinctl window`, geometry, focus                                           | `desktop`, `launch`                 | Partly             | `session/test_input.py`: click focus between two windows, `window move/resize/close`. Snapping, edge tiling, switcher, menus not yet                                                                                   |
| Keyboard and pointer input    | Typed text read back from a client                                             | `desktop`                           | Covered            | `session/test_input.py`. Real devices, touch and tablets: hardware                                                                                                                                                     |
| Isolation of the framework    | Environment, mounts, namespaces, host process list                             | `desktop`, own sessions             | Covered            | `session/test_isolation.py`, `session/test_lifecycle.py`, `fast/test_sandbox.py`, `fast/test_hostguard.py`                                                                                                             |
| Lock screen, greeter          | Headless Sway today                                                            | `make check-session`                | Not yet            | Run under Sway by `make check-session`; not on Gnoblin and not in pytest                                                                                                                                               |
| bsm (session manager)         | VM boot                                                                        | QEMU                                | Hardware           | Legacy `bsm-*`, `bsm-qemu*`                                                                                                                                                                                            |
| Installer, distribution image | VM boot                                                                        | QEMU                                | Hardware           | Legacy `distro-*`, `installer-*`                                                                                                                                                                                       |
| Packaging and install tree    | File lists, staged install                                                     | none                                | Legacy             | `install-dependencies.py`, `standalone-install.py`, `rpm-check`: run by `make check`                                                                                                                                   |
| Rendering quality, motion     | Frame timing on a GPU                                                          | a real machine                      | Hardware           | Software rendering only here                                                                                                                                                                                           |
| Multi-monitor, HiDPI          | —                                                                              | needs more than one output          | Not yet            | One virtual monitor, fixed 1280x720                                                                                                                                                                                    |

## Existing scripts

Every tracked file under `tests/` and `scripts/` is placed in an area of `tests/desktop/legacy.toml` with a fate:
`keep` (a tool, a fixture or a check that is not a desktop test), `fold` (a check with no compositor, to be
collected by pytest), `port` (drives the shell in a nested compositor today, to be rewritten on `Session`) or
`hardware` (needs a VM or a seat). `fast/test_legacy_map.py` fails when a file has no place, so a new loose script
has to be placed, or written as a framework test. `python3 -m bxdesktop.legacy` prints the table; `--list` names
every file. Nothing was deleted: migrating a script means writing its test as a `needs_gnoblin` or `fast` test,
then removing the script and its line here.

`scripts/test-desktop` stays as the way to look at the shell by hand in a nested Sway window. It is not a test and
it uses your Wayland display; automated checks go through `make test-desktop`.

## Observations from the first runs

The first isolated boot and the scenarios found these. None was investigated further here.

- `shell/bingux/TerminalSidebar.qml:973` reads `sidebarCustomise`, an id that no longer exists
  (`ReferenceError`, logged on every boot). The menu falls back to its 272 px minimum width. Listed in `KNOWN_DEFECTS`.
- Closing a window with `gnoblinctl window close` makes Gnoblin log
  `meta_gnoblin_window_effects_set_window_outline: assertion 'CLUTTER_IS_ACTOR(parent)' failed`. Listed in `KNOWN_DEFECTS`.
- The notification centre, opened empty, drew only "No notifications" and "Clear all" over the wallpaper, with no
  panel background (`session/test_panels.py`, notifications; screenshot `notifications-open`).
- `scripts/layout-stability.py` (Settings `mouse` page at 390x800) and `scripts/nav-slide.py` gave different
  verdicts between runs on a machine at load average 45, against a build older than the checkout. They are marked
  `slow`.
- `make lint` with no arguments fails on files nobody touched (ruff, shellcheck, clang-format, shfmt, prettier,
  qmlformat, stylua and rustfmt each report findings), so CI lints the changed lines only.

## CI

`.github/workflows/ci.yml` runs four jobs. `lint` runs the pre-commit hooks on the changed files, `static` runs
`make check-static` and `make test-desktop-fast`, `snapshot` builds Settings with `--features snapshot` and runs
`make test-desktop ARGS=apps`, and `desktop` runs the isolated suite on a self-hosted runner labelled
`bingux-desktop` when the repository variable `BINGUX_DESKTOP_RUNNER` is `true`. GitHub's hosted runners have no
Gnoblin, no patched Quickshell, and on Ubuntu 24.04 block the unprivileged user namespaces bubblewrap needs.

What has been run: the commands of `lint` (with `--from-ref origin/main --to-ref HEAD`) and `static`, locally, and
the workflow file through `actionlint`. What has not: any job on a GitHub runner, the `snapshot` job's package list,
and the `desktop` job. `act` is not installed on the machine that wrote this.
