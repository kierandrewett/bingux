# Background blur protocol

The dock, `ShellPopup`, search/preview panels and window switcher prefer the Wayland `ext-background-effect-v1` staging
protocol when its manager advertises blur support. `BackgroundEffect.qml` is an
optional wrapper around the native `Bingux.Effects` type. The native plugin now
requires the Qt Wayland Client development module when building.

Each wrapper supplies an item and its corner radius. The plugin combines all
items in a `QQuickWindow` into one surface-local `wl_region`. It sends changed
regions during scene synchronisation so the compositor applies them with the
matching buffer. The compositor selects blur strength. Gnoblin's existing rules
still supply radius 48 for the dock and 24 for shared popups.

The dock's private blur-region request remains the fallback when the standard
capability is absent. Both paths use the visible dock's bounds. Transparent
input padding above and below the dock must not request blur. Shared popups retain their existing
rule-based blur fallback. The standard only specifies background regions;
`SurfaceFade` still supplies per-item fade metadata where several animated items
share a buffer. Other shell surfaces retain their existing effects.

Run `tests/standard-background.py` through Gnoblin's `scripts/run-gnome-shell.sh`
with `MONITOR=1920x1080`, `QS_TEST_BIN` set to a compatible Quickshell, and
`GNOBLIN_TEST_DBUS_CLIENT` set to the test's absolute path. Steam and LocalSend
must be installed for the two real icon samples. The test opens the actual dock
menu with a right-click, checks the selected effect and its radius, and compares
blurred checkerboard pixels against a flat reference background.

For fallback coverage, set `EXPECT_STANDARD=0` and use `GNOBLIN_CONFIG` at startup
to disable `ext-background-effect-v1`, retaining the dock and popup blur rules.
All state and preferences used by the test belong to the isolated session.

`tests/popup-shadow-blur.py` uses the same private compositor harness. It checks
that black shadows only darken background pixels while the panel interiors keep
blur. Alpha-based shadow exclusion is disabled in this test, so success requires
correct standard regions. Shadows remain client-rendered; moving them into
Gnoblin requires geometry and shadow ownership for individual shell panels.

Search, search previews, the window switcher and the emoji picker use
`Theme.overlaySurface` with 55% tint opacity. Text and icons retain their normal
opacity. Other popup materials keep `Theme.popupSurface` at 80%.
