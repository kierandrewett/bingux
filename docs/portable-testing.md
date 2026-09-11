# Testing outside Gnoblin

Bingux's UI is Quickshell/QML. It can run on a Wayland compositor with
`wlr-layer-shell`; it does not need Mutter to draw its panels. Stock GNOME does
not expose this protocol. Run a nested compositor there instead.

## Nested desktop

From the repository root:

```sh
scripts/test-desktop --check
scripts/test-desktop
scripts/test-desktop --size 1600x900
scripts/test-desktop --smoke
```

Install Sway, dbus-run-session, and Quickshell. Quickshell needs its matching Qt
runtime and the `Bingux.Text` QML plugin (`packages/bingux-text-layout`). Set
`QML_IMPORT_PATH` to the directory containing the `Bingux` module, or pass a
packaged runtime wrapper with `--quickshell /path/to/wrapper`. Optional sidebar
terminal and helper features need their corresponding Bingux packages too.
The dependency check checks executables; it does not validate QML plugins.

Super+Space opens search. Super+Shift+E exits the nested desktop. The smoke
check loads the real shell, queries its status, routes search open/close through
the shell IPC controller, and exits. Because portable mode has no Gnoblin
compositor socket, it does not prove compositor-mediated companion visibility,
physical keyboard or pointer handling.

The launcher uses temporary home/config/state/cache/runtime directories and a
private D-Bus session. It starts private bingux-searchd and bingux-statusd when
those executables are available. Search uses installed desktop entries with no
personal file roots or AI configuration. Settings disappear on exit. It does
not stop the host shell or install session services.

The isolated bus and runtime have no host PipeWire server or session tray
clients. Audio controls and screen recording are not validated in this mode;
missing-service warnings are expected. System-bus controls still refer to the
real machine: this is a UI test environment, not a security sandbox.

## Existing layer-shell desktop

```sh
BINGUX_COMPOSITOR=portable qs -p shell/bingux
```

Use a matching Quickshell/QML plugin runtime. This command uses your current
settings and services. An existing panel or notification daemon may conflict;
the nested launcher is preferable for isolated testing. Bind compositor keys to
`qs ipc -p /absolute/path/to/shell/bingux call -- search open` or other binguxctl
commands. Portable mode disables Gnoblin socket connection attempts and skips
global cursor requests without delaying app launch.

To exercise an installed or staged payload, point the same test at its shell
directory and QML plugin path:

```sh
QML_IMPORT_PATH=/path/to/root/usr/lib64/bingux/qml \
PATH=/path/to/root/usr/bin:$PATH \
scripts/test-desktop --shell /path/to/root/usr/share/bingux/shell --smoke
```

## Dependency map

| Area | Dependency outside Gnoblin |
| --- | --- |
| Bar, dock layout, menus, calendar, notes, animations | Quickshell, Qt, layer-shell; notes require Bingux.Text |
| Dock windows and activation | Standard foreign-toplevel protocol; availability depends on compositor |
| Notifications and app actions | freedesktop notifications D-Bus service and desktop entries |
| Tray, media, audio, network, Bluetooth | StatusNotifier, MPRIS, PipeWire, NetworkManager, BlueZ |
| Search and metrics | Bingux helper daemons; useful without Gnoblin |
| App launching | Desktop entries and GIO helper; global loading cursor is Gnoblin-only |
| Alt+Tab, live previews, modifier-release handling | Gnoblin private compositor socket; no portable replacement yet |
| Global shortcut grabs, input forwarding, emoji caret insertion | Gnoblin private compositor socket; bind panel commands externally |
| Privacy events, keyboard source changes, native OSD events | Gnoblin integration; unavailable events are not equivalent to no activity |
| Screenshot and recording | Existing portal/grim fallbacks; needs matching portal backend and PipeWire |
| Blur, custom shaders, compositor slide effects | Gnoblin rules; configure effects in the host compositor separately |
| Native desktop installation | Still explicitly requires Gnoblin; this launcher does not change production session wiring |

The dependence is concentrated in compositor integration and production session
packaging, rather than the general UI. Portable mode is for UI testing, not full
behavioural parity. In particular it does not emulate Alt+Tab or GNOME's focus
semantics for menus.
