# Native popup loading and reload regression

Use a disposable QEMU run already logged into the installed native Gnoblin and
Bingux session. The VM must permit its test user to restart GDM with `sudo -n`.
The runner restarts guest GDM, temporarily adds a test-only IPC observer to the
installed main QML, and restores the original bytes in `finally`. It never
restarts host GDM. Keep other GUI tests out of the same VM during the run.

```sh
python3 tests/popups-load-native.py \
  --run "$HOME/.local/state/gnoblin-qemu-e2e/RUN" \
  --source shell/bingux/shell.qml \
  --output /tmp/popup-native-candidate
```

Replace `RUN` with an existing run directory. `--gnoblin-repo` selects the
checkout containing `scripts/qemu-e2e`; its default is Bingux's sibling
`gnoblin`. `--shell-target` overrides the guest's configured main QML path; the
default is `~/.local/share/bingux/share/bingux/shell/shell.qml`. Other installed
QML and plugins remain in place, so install changed dependencies first.

The input fixture requires one untransformed output at scale 1. It derives the
guest user, runtime directory, output dimensions and actual widget coordinates.
It uses the saved overflow placement instead of assuming System is in the bar.

The checks cover absent cold content, closing unused panels without loading
them, first pointer opening, live anchors and model references, outside-down
dismissal, Escape, retained identity on reopening, and nine open/closing/hidden
reload cases. A test-only generation stamp distinguishes engine reloads from
stale IPC replies. Layer IDs verify old popup disposal before reopening. System
and its overflow host can legitimately map two distinct popup surfaces.

Keyboard assertions follow the compositor's exclusive-focus handoff. They do
not prove input delivery before mapping, credential-click login timing, native
graphics-cache expiry, multi-monitor behavior or high-DPI rendering. The saved
frames, generation records and layer samples support manual review.

## Cold compilation evidence

Record a Qt 6.10.3 `qmlprofiler` XML trace with Compiling, Creating and SceneGraph
events, without opening these panels. The profiler changes normal timing; use
an uninstrumented capture for latency measurements.

```sh
python3 tests/qml-cold-trace.py /tmp/main.qmltrace
python3 tests/qml-cold-trace.py /tmp/main.qmltrace --until-first-scenegraph
python3 tests/qml-cold-trace.test.py
```

The verifier correlates actual ranges with event definitions. Exit 0 proves
absence of the three popup sources in the selected interval; 1 reports forbidden
activity; 2 rejects malformed or inadequate evidence. `--forbid BASENAME` is
repeatable and replaces the defaults. `--require-source BASENAME` defaults to
`shell.qml` and requires both compilation and creation coverage for that entry
source. A bounded check permits deliberate first use after the first SceneGraph
marker. Do not sum inclusive profiler durations as wall-clock savings.
