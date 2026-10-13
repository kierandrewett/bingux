# Dock launch activation candidate

This fixture separates token transport from compositor acceptance. It does not
relax strict focus policy, activate windows by app ID, or install anything.
Keep the existing priority proof separate: scheduling rules can change nice after
launch. Native window/focus proof requires the main agent's VM session.

## Isolated checks

From the Bingux checkout:

```sh
python3 tests/launch-activation.py -v
python3 tests/dock-desktop-action.py
```

These use private D-Bus and desktop entries. They cover default/action/new-window
Exec tokens, stale inherited environment, the manager argv boundary and rejection
fallback, action identity, field codes, working directory, and real GIO D-Bus
`Activate`/`ActivateAction` startup data. The manager is a subprocess fixture,
not the session's real user manager. The extracted QML routing check is additional
coverage; it does not prove native input or focus.

## Build and prepare the native fixture

Run these steps in the VM, using the Qt development tools that match its
Quickshell runtime. GTK 4 development files, a C/C++ compiler, qmake, Python GI,
and the existing Bingux native build dependencies must be available.

```sh
activation_dir=/var/tmp/bingux-activation-proof
python3 tests/launch-activation.py --prepare-native "$activation_dir"
mkdir -p "$activation_dir/native" "$activation_dir/qml/Bingux/Wayland"
qmake6 "$PWD/packages/bingux-wayland-sync/sync.pro" -o "$activation_dir/native/Makefile"
make -C "$activation_dir/native" -j2
cp "$activation_dir/native/libbinguxwaylandsync.so" "$activation_dir/qml/Bingux/Wayland/"
cp packages/bingux-wayland-sync/qmldir "$activation_dir/qml/Bingux/Wayland/"
```

Preparation requires a new directory. It compiles an unclassified GTK 4 binary,
copies the current launcher, and creates a private desktop entry and QML config.
It does not start a window. The entry has action ID `proof`, label
`Open activation proof`, and no `StartupNotify` or `DBusActivatable` flags.
The GTK application is `NON_UNIQUE` with ID `dev.bingux.ActivationProbe`.

Select the actual matching Quickshell executable, rather than a wrapper that
replaces `QML_IMPORT_PATH`. For the managed Gnoblin layout:

```sh
qs_binary="${XDG_DATA_HOME:-$HOME/.local/share}/gnoblin/quickshell-fedora/bin/quickshell"
python3 tests/launch-activation.py --native-check "$qs_binary" "$activation_dir/native"
```

The offscreen check loads the compiled native plugin with private config/XDG
paths and a nonexistent bus address. It checks tokenless callbacks without
creating native windows. It deliberately excludes the native fixture's layer
surfaces, which cannot load on Qt's offscreen platform. The process is bounded
to ten seconds. This is not evidence of a real seat, serial, or focus grant.

## Native input and popup lifecycle

From a terminal inside the VM's existing Gnoblin session:

```sh
"$activation_dir/run-native.sh" "$qs_binary" "$activation_dir/qml" \
    2>&1 | tee "$activation_dir/fixture.log"
```

The fixture uses the real session's Wayland connection and bus, with private
application/config/cache paths. It does not import Bingux settings or change
session identity. A local `--host-launch` call isolates the focus lifecycle from
the real user manager. The ordinary dock must separately prove manager dispatch.

1. Keep another normal application open. Start main's existing API event capture
   for `gnoblin.window.activation-denied` before clicking.
2. Click **Action: held**. The source layer surface remains keyboard-interactive.
   Record the token-present reply, the window's stacking/attention state, and the
   compositor event. Then click **Release source** and observe focus again.
3. Close the proof window, reopen the gesture surface from the fixture controls,
   and click **Action: dismissed**. The fixture immediately removes keyboard
   interactivity and input, but retains the source surface, like `ShellPopup`.
4. Repeat both cases with **Default: held** and **Default: dismissed**.
5. Run **Tokenless control**. It launches the same action without requesting a
   token, even though a button was clicked. This represents the IPC boundary.
6. Close the fixture using its controls. Close any remaining proof windows.

`fixture.log` records requested action ID/label, source mode, token presence and
helper exit. It never prints token values. `windows.jsonl` records actual GTK
PID, nice, scheduler policy, monotonic time, environment presence in `main()`,
and `notify::is-active` transitions. GTK 4's GDK constructor can retain and unset
activation variables before `main()`; `token_present=false` in that record does
not prove missing transport or an absent token in GDK.

A returned token is not a focus grant; correlate those records with compositor
events and visible stacking. Record the
active shell PID and executable hash alongside the fixture results.

If held and dismissed differ, retain both event traces before changing the
compositor. No compositor patch or focus-policy workaround is part of this
candidate. An exclusive held source may itself retain keyboard focus; the
explicit release step distinguishes that from activation denial.

## Delayed dispatch and token replay (main's VM checks)

The fixture accepts two test-only environment flags, each an integer from `0`
to `600000` milliseconds. Invalid values produce a QML error.

- `BINGUX_ACTIVATION_DELAY_MS`: delay the first helper dispatch from the ready
  gesture handler. Default `0` dispatches as soon as the token reply arrives.
- `BINGUX_ACTIVATION_REPLAY_MS`: when positive, dispatch the same command with
  the exact same token a second time after this interval. Default `0` disables
  replay. An empty token skips replay and logs `BINGUX_ACTIVATION_REPLAY_SKIPPED`.

Readiness still gates every request. `activation.request()` captures the real
source/seat/serial synchronously in the gesture handler; only helper dispatch
is deferred. Each request owns a root-parented, one-shot Timer that survives
source dismissal, is destroyed after the final dispatch, and is cancelled when
the fixture exits. Independent clicks do not replace pending requests. No
synthetic gesture or additional token request is used for replay.

Preparation copies the QML file. If the directory was prepared before this
extension, main must refresh it with
`cp tests/launch-activation.qml "$activation_dir/shell.qml"` before restarting.

Restart the fixture for each case, with no leftover proof windows. Wait for
`BINGUX_ACTIVATION_FIXTURE_READY` with `ready:true`, then click **Action:
dismissed** once. Main captures compositor events and visible focus/attention
alongside `windows.jsonl`. Use a separate log per case:

```sh
# Grant expiry: allow 16 seconds without further seat input after the click.
env BINGUX_ACTIVATION_DELAY_MS=16000 BINGUX_ACTIVATION_REPLAY_MS=0 \
    "$activation_dir/run-native.sh" "$qs_binary" "$activation_dir/qml" \
    2>&1 | tee "$activation_dir/expiry.log"

# Later-input revocation: click/type in another normal app before dispatch.
env BINGUX_ACTIVATION_DELAY_MS=3000 BINGUX_ACTIVATION_REPLAY_MS=0 \
    "$activation_dir/run-native.sh" "$qs_binary" "$activation_dir/qml" \
    2>&1 | tee "$activation_dir/later-input.log"

# Lock/unlock: complete a real session lock/unlock before the delayed dispatch.
env BINGUX_ACTIVATION_DELAY_MS=10000 BINGUX_ACTIVATION_REPLAY_MS=0 \
    "$activation_dir/run-native.sh" "$qs_binary" "$activation_dir/qml" \
    2>&1 | tee "$activation_dir/lock-unlock.log"

# Replay: two helper dispatches, one captured token, 300 ms apart.
env BINGUX_ACTIVATION_DELAY_MS=0 BINGUX_ACTIVATION_REPLAY_MS=300 \
    "$activation_dir/run-native.sh" "$qs_binary" "$activation_dir/qml" \
    2>&1 | tee "$activation_dir/replay.log"
```

Run a `3000` ms delay control without later input to distinguish revocation from
ordinary delay. For lock/unlock, also run a `10000` ms no-input control; finish
unlocking before dispatch and retain the lock/unlock event times. If that is not
possible, record the case as inconclusive rather than attributing expiry to
locking. Do not weaken authentication or session policy to complete the case.

`BINGUX_ACTIVATION_REQUEST`, `REPLY`, and `DISPATCH` records share a request ID.
Dispatch records include the attempt number, elapsed milliseconds since the
gesture handler, and token presence, never token values. Timers may fire late;
use recorded elapsed time, not the configured delay alone. Replay should show
one reply and two dispatches for one request ID. Correlate each child PID/window
with compositor acceptance or denial; two helper exits alone do not establish
single-use enforcement, and a denied window may still be created. Establish that
the first activation consumed the token before interpreting the second as a
replay denial; helper dispatch order alone does not prove activation order. These are VM
procedures for main, not claims that the offscreen check proves policy behavior.

## Actual dock integration

Main applies the scoped delta to its clean delivery checkout, then builds only
`packages/bingux-wayland-sync/sync.pro` as above. Do not run a broad install from
the dirty shared checkout. This candidate's runtime payload is exactly:

- `shell/bingux/Dock.qml`
- `shell/bingux/DockAppMenu.qml`
- `shell/bingux/DockFolderPopup.qml`
- `shell/bingux/launch-application.py`
- The newly built `libbinguxwaylandsync.so`

For a managed install, `scripts/install-shell.py` maps shell files to
`$PREFIX/share/bingux/shell/` and the plugin to
`$QMLDIR/Bingux/Wayland/libbinguxwaylandsync.so`. Main must resolve the active
prefix/import path and any `BINGUX_APP_LAUNCHER_HELPER` override before replacing
only those files, retaining rollback copies. `qmldir` is unchanged. The private
import-directory steps above do not replace the installed plugin.

A new shell process must load the new plugin; an already-running process does
not acquire a rebuilt native type. Main owns the scoped installation and shell
lifecycle. Do not copy only the QML files into an old shell process:
`LaunchActivation` requires the matching native plugin.

Use an unclassified window-producing desktop entry to verify an actual dock
click, **Launch new...**, exact desktop action, folder launch and keyboard
activation through the installed shell. Capture manager/helper diagnostics,
compositor events and final focus/attention/priority state. Tokenless IPC must
still receive the compositor's normal strict policy.

## Protocol and GIO boundary

The native helper captures Qt's actual source `wl_surface`, last input `wl_seat`
and serial synchronously in the input handler. Each standard `xdg_activation_v1`
request is independent and falls back to a tokenless launch after 1.5 seconds.
No token is reused for later clicks. The helper uses Qt's connection directly:
Qt's generic token request delegates to the shell-surface implementation, while
Quickshell layer surfaces do not implement that method.

The Python helper passes an explicit token as `XDG_ACTIVATION_TOKEN` for Exec,
including entries without `StartupNotify`. GIO itself requests startup metadata
only when `StartupNotify=true`; in that case the launch context supplies
`desktop-startup-id` and `activation-token` for D-Bus activation as well. Entries
without that opt-in retain GIO's tokenless D-Bus semantics. Do not infer universal
D-Bus focus support or rewrite desktop metadata to hide this limitation.

Authoritative references:

- [xdg-activation-v1 protocol](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/xdg-activation/xdg-activation-v1.xml)
- [Qt 6.10.3 Wayland activation](https://github.com/qt/qtbase/blob/v6.10.3/src/plugins/platforms/wayland/plugins/shellintegration/xdg-shell/qwaylandxdgactivationv1.cpp)
- [GLib 2.86.5 desktop launch implementation](https://github.com/GNOME/glib/blob/2.86.5/gio/gdesktopappinfo.c)
