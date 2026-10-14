# Bingux Quickshell runtime

A layer surface must record the settings sent when it is created. Quickshell
0.2.1 through 0.3.1 leave that snapshot at its default values. If a setting
returns to its default before the next commit, the compositor keeps the old
value.

`layer-initial-state.patch` records the initial settings in the existing
committed-state snapshot. Downstream package builders can apply it when they
build the matching Quickshell release.

`clipping-window.patch` gives the rounded clipping texture source a visual
parent. It then releases its old window reference when the widget moves. Without
that parent, moving a populated media sidebar into a detached window produces
cross-window warnings; closing or reloading it can crash Quickshell. The clipping
shader and its appearance remain the same.

The runtime disables Quickshell's built-in crash reporter and minidump writer.
The service also sets `LimitCORE=0` to prevent kernel memory dumps. Systemd
handles service restarts, and normal diagnostic messages remain in the journal.
Private UI tests set the same core limit so failed tests cannot fill a checkout.

Run `tests/layershell-focus-live.py` as `GNOBLIN_TEST_DBUS_CLIENT` in Gnoblin's
private session, with `QS_TEST_BIN` pointing to the matching Quickshell runtime.
The test reads actual Wayland requests. Before the patch it receives only
`OnDemand` (2); after the patch it also receives `None` (0).

`local-files.patch` loads a config folder as ordinary files when every directory
in it ships its own qmldir. Quickshell otherwise serves every file through its
`qs:` network scheme, and Qt keeps its compiled-unit disk cache only for local
files, so the whole shell recompiled on each start.

Bingux ships a qmldir, so the
shell loads from the cache after its first start. Set `QS_NO_LOCAL_FILES=1` to
use the old behaviour.

In the test VM the main shell loaded about 0.3 to 0.5 s
faster; VM noise is about 1 s, so treat that as an estimate.

`child-pdeathsig.patch` starts every Process child with PR_SET_PDEATHSIG, so helpers die
with the shell. Without it, a killed or restarted shell left its audio meter,
idle tracker, camera and microphone watchers and the appearance monitor
running, and each restart added another set. Execute-and-detach commands are
not affected.

`ipc-connection-lifetime.patch` releases each server-side IPC connection after
its local socket disconnects. Status clients open short-lived sockets, so
retaining their parented connection objects also retains their socket buffers
until Quickshell exits. The patch includes a bounded QTest that connects one
local socket, disconnects it, and waits for the connection object to be
destroyed.

Apply the patch to the pinned Quickshell source before rebuilding the runtime:

```sh
git -C /path/to/quickshell apply /path/to/bingux/packages/bingux-quickshell/ipc-connection-lifetime.patch
```

Applying it changes the source only. Rebuild and install Quickshell, then
restart Bingux to use it. With `BUILD_TESTING=ON`, run the `ipcconnection` CTest
to verify that disconnecting destroys the server-side connection and its socket.

## Reclaim unused memory

`allocator-background-reclaim.patch` enables jemalloc's asynchronous page
reclamation after the graphical application initializes. Without background
reclamation, retired rendering threads can leave unused dirty pages resident
in idle arenas after popup windows close.

The patch keeps the allocator's
existing decay intervals and skips this default when `MALLOC_CONF` is set.
Builds with `USE_JEMALLOC=OFF` keep their existing allocator behavior. IPC and
other command-only clients do not start background workers.

Apply it to the pinned Quickshell source:

```sh
git -C /path/to/quickshell apply /path/to/bingux/packages/bingux-quickshell/allocator-background-reclaim.patch
```

Rebuild Quickshell and restart Bingux to use it. Do not enable `background_thread` through a startup environment
override: jemalloc recommends dynamic control to avoid initialization
dependencies. See the [jemalloc manual](https://jemalloc.net/jemalloc.3.html#background_thread).

For an installed desktop in a disposable VM, check repeated popup expiry with:

```sh
python3 tests/panel-lifecycle-live.py --panels controls calendar notifications \
  --cycles 4 --warmup-cycles 1 --settle-seconds 35 --max-growth-mib 30 \
  --report /path/to/popup-memory.json
```

This measures the shell and its owned helpers after idle expiry. It does not
attribute GPU memory or prove a long session is free of leaks.
