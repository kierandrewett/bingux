# Check shell repaint cadence

Run the production bar, dock and media QML with controlled inputs:

```sh
QS_TEST_BIN=/path/to/quickshell python3 tests/shell-idle.py
QS_TEST_BIN=/path/to/quickshell python3 tests/shell-idle.py --renderer opengl
BINGUX_REDUCED_MOTION=1 QS_TEST_BIN=/path/to/quickshell python3 tests/shell-idle.py
```

Use the same Qt-compatible Quickshell and QML plugin paths as the installation.
The default renderer is Qt Quick software on the offscreen platform. The OpenGL
option starts a private headless Weston with llvmpipe. Both processes are pinned
to one CPU; the shell has a 55-second wall timeout and a 20-second CPU limit.
The fixture uses private home, runtime, configuration and D-Bus directories. Its
bus cannot activate desktop services. All fixture process groups are stopped.

The fixture retains production content, grouping, bindings and animations.
It replaces layer windows with floating windows and supplies controlled window,
notification, audio and player inputs. Appearance monitors and delayed background
helpers are disabled. Native placement, compositor blur and real device services
are outside this check.

The assertions cover:

- No playhead sampling or frames while inline music has a short title and its
  seek panel is hidden; visible seeking retains its normal cadence.
- Play, pause, next-track and seek delivery, followed by an immediate position
  refresh when reopening a paused player.
- Stable dock delegates and no repainting from equivalent snapshots; live
  focus, minimize, notification and audio badge changes still reach the UI.
- Minute label changes and the real seconds timer and setting transition.
  The minute timer is stopped during idle measurements to avoid a random
  wall-clock rollover; its production scheduling code is unchanged.

Usage graphs are deselected for the zero-frame assertions. An updating graph
or a deliberately scrolling long song title is not a quiescent surface.
The test does not disable those behaviors in production.

## Compare an installed source snapshot

`--source` accepts a shell directory or an export containing only QML and JS.
Missing module declarations and bundled icons come from this checkout. An overlay
changes only the private copy; it does not modify the export or installed shell.

```sh
QS_TEST_BIN=/path/to/quickshell python3 tests/shell-idle.py \
  --source /path/to/installed-qml --output /tmp/bingux-idle-before
QS_TEST_BIN=/path/to/quickshell python3 tests/shell-idle.py \
  --source /path/to/installed-qml --overlay shell/bingux/MediaControls.qml \
  --output /tmp/bingux-idle-after
```

Before the visibility fix, the standalone playing inline control samples its
hidden seek bar about 60 times per second and submits frames despite unchanged
visible content.
The expected regression failures are the hidden-playhead and idle-frame checks.
After the fix, both hidden sampling and those frames stop. The visible seek panel
continues to animate, or updates once per second with reduced motion.

## Native follow-up

The repeatable real-MPRIS driver and native commands are in
[native-media-idle.md](native-media-idle.md).

Use the installed source snapshot as A and overlay only `MediaControls.qml` as B.
Keep the same compositor build, configuration, monitor, applications and player.
Wait for startup and transitions to settle, then collect equal-duration samples
of client surface commits and compositor CPU for each case:

1. The original idle state, with no playback or open panels.
2. A media widget placed in the top bar, playing a short title that fits, with
   its details closed and the pointer away from the widget.
3. The same player with its details open, then closed again.

Case 2 should lose hidden playhead sampling. Whether frames also disappear depends
on whether the detail window has been created and remains mapped. Case 3 must keep
visible progress and responsive seeking, then stop hidden sampling after closing.
Check notification delivery, dock focus and
minimize, a minute rollover, and track/playback changes after swapping the file.

A blur shader appearing in every compositor frame does not establish the client's
commit rate. If the original idle state has no active inline media, this fix does
not explain its compositor CPU usage. Native CPU and damage behavior must be
measured separately; the private fixture proves QML behavior only.
