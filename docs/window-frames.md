# GTK window frames

Right-click window menus are rendered by Bingux's shared Quickshell `TrayMenu`
with compact rows matching the keyboard layout selector. The integration drop-in
sets `shell["window-menu"] = { "binguxctl", "ipc", "shell", "windowMenu" }`.
Gnoblin supplies the original window ID and actions; `gnoblinctl window menu ID`
also opens it, suitable for a configurable shortcut. Reload via `gnoblinctl config reload`.

`bingux-frame` draws **actual GTK4/libadwaita widgets**, not a lookalike. It snapshots
an `AdwHeaderBar`, `AdwWindowTitle` and GTK's title buttons into Gnoblin's private
frame surface. GTK supplies typography, symbolic icons, theme colors, button
layout and state styling. Gnoblin still owns geometry, radius, input and recovery.

Snapshots are rendered by the GTK root's `GskRenderer`, then downloaded to the
shared-memory frame buffer. Do not replace this with `gsk_render_node_draw()`:
its Cairo fallback changes text rasterization even at 100% display scale.
`bash packages/bingux-frame/test-title-rendering.sh OUTPUT_DIR` compares the
actual painter's title pixels with GTK's renderer at odd/even widths and in
focused/unfocused states. Run it on the nested session's display and settings;
it also writes the old Cairo path for visual comparison. This test covers 1x
rasterization, not mixed-DPI buffer negotiation.

Build just this renderer: `make build/bingux-frame`. Normal builds/package installs
include it at `PREFIX/libexec/bingux/bingux-frame`. It requires GTK4, libadwaita and
Wayland, but no JavaScript, QML or running Bingux panels. The vendored MIT transport
comes from Gnoblin's `src/tools/frame-renderer` and its private protocol XML.

It is not automatically enabled. Add to your Gnoblin Lua config (adjust PREFIX):

```lua
["frame-renderers"] = {
    bingux = { "bingux-frame", "--compact" },
},
["window-rules"] = {
    { match = { ["app-id"] = "^spotify$" },
      frame = { mode = "prefer-server", renderer = "bingux",
                extents = { 36, 0, 0, 0 } },
      corners = { mode = "force", radius = 12 } },
},
```

Service registration reloads with `gnoblinctl config reload`. After rebuilding
`bingux-frame`, use the same `gnoblinctl config reload` command. Watched config
edits also restart the renderer; windows remain open during the
renderer replacement. Updating Gnoblin's compositor code itself still needs one
new session. Follow GTK's configured
decoration layout, or pass `--button-layout=:minimize,maximize,close` explicitly.
The renderer uses an ordinary `AdwHeaderBar` with GTK's `titlebar` class.
Its natural height is 46px with the installed libadwaita theme, matching the
header in a normal GTK4/libadwaita application. Theme/version/font changes can alter
that minimum. Existing native border/radius rules are separate.
For the shorter title-only header, pass `--compact` and use 36px top extents.
This adds GTK's `default-decoration` class; it keeps the same GTK frame-clock
animations. The user's current desktop selects this compact variant.

These metrics come from libadwaita's own
[headerbar stylesheet](https://github.com/GNOME/libadwaita/blob/1.8.7/src/stylesheet/widgets/_header-bar.scss),
not a Bingux CSS recreation. The compositor rounds the bottom content hole using
the outer radius minus the side/bottom extents, so the inner frame corners do not
remain square.

The GTK root is a snapshot host: it never presents an application window. Its
show/map paths bypass GtkWindow presentation; per-frame state must not call
GtkWindow APIs that present a toplevel. Tests reject extra managed windows.
The native surface has a working `GdkFrameClock` even without being presented.
The widget tree remains alive so GTK retains its CSS transition history. GTK's
main context and the frame protocol connection share one poll; after-paint
requests a redraw of that window. Hover, press, leave and backdrop transitions
are GTK's own colors and timing, and follow GTK's animation setting. There are
no custom tint overlays or fixed animation timers. Accessibility and mixed-scale
buffer negotiation are not implemented by this transport.

Run `bash packages/bingux-frame/test-gtk-comparison.sh OUTPUT_DIR` on the
normal-config nested display. It presents a normal GTK window with an
`AdwHeaderBar` and compares its rendered header with the actual SSD painter.
All six settled states must match byte-for-byte; animated reference transitions
must also produce multiple SSD frames. `render-gtk-comparison.py OUTPUT_DIR`
assembles the captures into a labelled PNG and animation.
Pass `--compact` after OUTPUT_DIR to compare the compact variant against GTK's
own compact decoration. Both variants must meet the same pixel/transition checks.
