# Native tray menu fixture

This fixture connects the real `Tray.qml`, `QsMenuOpener`, `TrayMenu.qml`, and
`ShellPopup` to a StatusNotifierItem and DBusMenu service implemented with
Gio. It must run inside an already logged-in, explicitly disposable QEMU/KVM
guest. It does not start or control a VM, restart GDM, or manipulate the host
session. The external test driver owns real pointer and keyboard input.

Choose a new artifact directory for every run. Set the guest marker only in
the disposable guest:

```sh
BINGUX_DISPOSABLE_QEMU_GUEST=1 \
  python3 tests/tray-menu-native.py --artifacts /tmp/tray-menu-native-run
```

`--peer-count N` exports 0–24 independent peer providers (default: 1).
Each owns a separate bus connection and name; removing or replacing the
target preserves these peers. The target delays menu rows; peers answer
immediately. Use 24 peers for a many-item footprint comparison.

The service registers one menu item with the ID
`tray-menu-native-target`. It delays `GetLayout` by 1.2 seconds by default,
then supplies a real DBusMenu row. The controller name is
`dev.bingux.TrayMenuFixture`; its `dev.bingux.TrayMenuFixture1` interface at
`/dev/bingux/TrayMenuFixture` has `DelayLayout(u)` (0–10000 ms),
`ReleaseLayout()`, `ReplaceItem()`, `RemoveItem()`, and `Snapshot()`. Use these
session-bus calls from the main harness to control delayed delivery:

```sh
gdbus call --session --dest dev.bingux.TrayMenuFixture \
  --object-path /dev/bingux/TrayMenuFixture \
  --method dev.bingux.TrayMenuFixture1.DelayLayout 5000
gdbus call --session --dest dev.bingux.TrayMenuFixture \
  --object-path /dev/bingux/TrayMenuFixture \
  --method dev.bingux.TrayMenuFixture1.Snapshot
```

`Receipt(s,i)` signals a clicked DBusMenu item. `Snapshot()` and
`result.json` report methods, layout requests, menu events, receipts, and
lifecycle. Quickshell output is saved as `quickshell.log`.
The report includes `quickshellPid` and `qmlRoot`. Action receipts identify
the caller's bus name; resolve it with D-Bus `GetConnectionUnixProcessID`
to check that the fixture process received the click.

The runner copies the real shell components to `ARTIFACTS/qml` and places
the fixture beside them as `shell.qml`. It does not substitute menu components.
The QML root is a small native `PanelWindow` with a real SystemTray-backed
target `Tray` and a separate peer `Tray` holding all other discovered items.
Its `tray-menu-native.snapshot` IPC method reports panel-relative target
geometry, actual loaded menu rows, native popup geometry, focus, and peer IDs.
Its `tray-menu-native.openTarget` and `tray-menu-native.cancelTarget` methods
control a pending request. The IPC calls are available through the fixture
process, for example `qs -p /tmp/tray-menu-native-run/qml ipc call
tray-menu-native snapshot`. Use the main harness's QMP route for clicks and key
input. The provider's action receipt proves
that selecting the displayed row reached DBusMenu `Event(clicked)`.

The main harness should use its existing QMP or Gnoblin RemoteDesktop input
path to verify a cold first left click, right click, and Return/Space keyboard
activation after the delayed row appears. It should also test Escape and an
outside click while `GetLayout` is pending, then confirm that the menu remains
closed when the row arrives. Check the peer IDs before and after activating
the target. `ReplaceItem` and `RemoveItem` exercise provider lifecycle. Save
the result JSON and an actual guest screenshot in the caller's explicit
artifact directory; this fixture itself does not inject input or capture the
desktop.

Stop the process with SIGINT or SIGTERM. It terminates only the Quickshell
process it started, unregisters its own Gio objects, and writes final receipt
state. `--serve-only` exports the provider without launching the QML fixture.
