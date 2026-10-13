# Native media idle A/B driver

Main owns native execution. This driver starts no shell, compositor or desktop
service. It measures explicitly supplied PIDs and temporarily exports one synthetic
MPRIS player on their existing session bus. It never plays audio. Each variant uses
the exported installed QML, including its 12 differences from clean main; B changes
only `MediaControls.qml`. No shared dirty file is needed for that candidate.

## Prepare the copies

Requirements: the installation's Qt-compatible Quickshell and plugin environment,
QtTest QML, Python 3 with PyGObject (`gi.repository.Gio`), and readable `/proc/PID/stat`
for the shell and compositor. Keep the driver and `.inc.qml` beside each other.
The handoff includes the candidate as `MediaControls.qml`.

Run preparation where the complete installed shell assets are available. Replace
`/path/to/installed/shell` with the directory containing the installation's
`qmldir`, helpers and icons. Use a new output path.

```sh
python3 native-media-idle.py prepare \
  --snapshot /var/tmp/bingux-vm-installed-qml-20261013 \
  --assets /path/to/installed/shell \
  --candidate MediaControls.qml \
  --output /var/tmp/bingux-native-media-ab
```

This copies assets, overlays **all** exported QML/JS, then appends identical test
instrumentation to A and B. It verifies that only `MediaControls.qml` differs and
writes SHA-256 provenance to `manifest.json`. It does not alter the export, assets,
installed shell or settings. Native surfaces remain production `PanelWindow`s.

## Run A, then B

Use the same running compositor, libraries, configuration, monitor, applications
and environment for both runs. Do not change the Gaussian implementation or
recovery polling between A and B. Main stops its current shell and launches each
copy exclusively through its normal native launcher, selecting the staged path.
Launch a **fresh shell process for each run**, with these environment values:

```sh
BINGUX_LAYOUT_IMPORT=0 BINGUX_REDUCED_MOTION=0 BINGUX_LIVE_RELOAD=0 \
  "$MEDIA_QS" -p /var/tmp/bingux-native-media-ab/A --no-color
```

`MEDIA_QS` must identify the installation's Quickshell executable or wrapper.
Preserve the launcher's normal session bus, runtime and plugin environment. Start
this in a separate main-owned terminal/runner and obtain its actual Quickshell PID.
The driver will reject an unrelated shell path. Let startup settle for 10 seconds.

**Before measuring, visually confirm complete bar and dock contents in both
variants.** A mapped but blank or partly painted dock invalidates CPU comparisons.
Readiness, geometry and frame counters cannot certify visible pixels. Resolve that
independent condition first; do not repair the display by interacting during a
measurement. Keep the pointer on empty desktop space. Avoid other playback,
notifications, window changes, source writes and input during the run.

In the same session environment, set `MEDIA_SHELL_PID` and
`MEDIA_COMPOSITOR_PID` to the actual numeric PIDs and run:

```sh
python3 native-media-idle.py run \
  --source /var/tmp/bingux-native-media-ab/A \
  --shell-pid "$MEDIA_SHELL_PID" --compositor-pid "$MEDIA_COMPOSITOR_PID" \
  --quickshell "$MEDIA_QS" --expect baseline \
  --seconds 8 --settle 2 --repeats 3 \
  --output /var/tmp/bingux-native-media-A
```

Stop A through main's existing launcher. Start B with the same environment, update
only `MEDIA_SHELL_PID`, confirm complete contents again, and run:

```sh
python3 native-media-idle.py run \
  --source /var/tmp/bingux-native-media-ab/B \
  --shell-pid "$MEDIA_SHELL_PID" --compositor-pid "$MEDIA_COMPOSITOR_PID" \
  --quickshell "$MEDIA_QS" --expect fixed \
  --seconds 8 --settle 2 --repeats 3 \
  --output /var/tmp/bingux-native-media-B
python3 native-media-idle.py compare \
  /var/tmp/bingux-native-media-A/result.json \
  /var/tmp/bingux-native-media-B/result.json
```

Allow about three minutes per variant. Every accepted interval is eight seconds;
clock-rollover intervals are retained as excluded and repeated. The command is
bounded by interval/repeat limits and finite IPC/poll timeouts. It does not cap or
signal the native processes. The private validation harness separately caps CPU.

## Controlled phases and expected discriminator

The fixture selects `org.mpris.MediaPlayer2.bingux_idle_fixture` in the real media
widget, placed in the top bar. It uses a short title, a 300-second track, a 42-second
starting position and no periodic D-Bus signals. Position advances on demand.
Metrics are hidden and the minute clock remains active. The layout changes only
in memory and is restored afterward; the driver does not call settings writers.

| Phase            | State                            | Baseline                                | Candidate         |
| ---------------- | -------------------------------- | --------------------------------------- | ----------------- |
| paused-closed    | Fresh shell; detail never opened | No playhead ticks                       | No playhead ticks |
| playing-closed   | Playing; detail never opened     | Hidden playhead ticks                   | Zero hidden ticks |
| playing-open     | Artwork clicked; seek visible    | Visible progress                        | Visible progress  |
| playing-reclosed | Detail opened, then dismissed    | Hidden ticks; cached window may repaint | Zero hidden ticks |

Each phase has three accepted intervals. Before **each** reclosed interval, the
fixture opens and closes the detail again, then settles for two seconds. This
keeps its 30-second native-window cache from expiring midway through the samples.
The driver records and validates mapped/retained/seek states. The candidate should
remove the closed-state cadence; open-state progress must continue at its normal
rate. Bar/dock/detail frame counts are separate: do not assume hidden ticks damage
the bar, or that an uncreated detail window can submit frames.

After measurements, QtTest clicks the actual controls. The driver requires real
MPRIS Pause, Play, Next and Seek/SetPosition calls and resulting UI state. It also
closes a paused detail, moves the synthetic position to 123 seconds and requires
that position when reopening. These are Qt-dispatched mouse events, not physical
HID or compositor hit-test proof. Manual native interaction remains a separate
check. Notification, window-state and clock-update regressions are covered by
`tests/shell-idle.py`; this driver concentrates on real MPRIS and native surfaces.

## Read the evidence and restore

Each output contains `result.json`: source hashes, process identities, timestamps,
per-interval snapshots, CPU deltas, surface frames, playhead ticks, input counters,
interaction checks and median summaries. CPU is percent of **one core**, derived
from each process's own user+system ticks; child/helper CPU is excluded. Frame
counts are `QQuickWindow.frameSwapped`, not Wayland wire commits, compositor frame
counts, shader calls or pixel-damage regions. Native trace evidence can be aligned
with the recorded wall timestamps. The CPU and frame brackets differ slightly
because IPC snapshots surround the CPU sample; both elapsed durations are saved.

The comparator requires equal intervals, repeats, settling, shell instrumentation,
driver, compositor PID/start/executable, geometry and dock-model count. It cannot
verify library identity, rendered pixels, external workload or display damage.
Do not accept a pressure reduction from missing UI or a changed compositor.

Normal completion and Python exceptions restore the original live layout/player
selection and release only this driver's MPRIS name. Main owns stopping the test
shell and returning to its original launcher. If the driver is forcibly killed,
restore the in-memory layout before stopping the test shell if needed:

```sh
"$MEDIA_QS" ipc --pid "$MEDIA_SHELL_PID" call native-media-idle restore
```

The test has no disk-settings rollback because it never writes those settings.
It refuses to take over an existing fixture bus name and never stops other players.

No MPRIS name was active in the reported original idle VM state. This candidate
therefore does **not** explain that original compositor load; its claim is limited
to unnecessary work during inline playback and retained detail-window rendering.
