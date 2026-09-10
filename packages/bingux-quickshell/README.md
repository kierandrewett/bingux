# Bingux Quickshell runtime

A layer surface must record the settings sent when it is created. Quickshell
0.2.1 through 0.3.1 leave that snapshot at its default values. If a setting
returns to its default before the next commit, the compositor keeps the old
value. In Customise UI, this can leave the sidebar accepting keyboard focus,
so a mouse press takes focus from the editor and the return of focus cancels
the button click.

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
