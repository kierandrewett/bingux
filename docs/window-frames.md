# Bingux window frames

Bingux enables the `bingux-frame` renderer for ordinary app windows. Its
`auto` mode keeps an application's client-side decorations and supplies a
compositor-drawn titlebar only when the application requests one.

The frame helper uses GTK4/libadwaita widgets and follows GTK's theme, fonts,
and title-button layout. Its GTK background also supplies the rounded bottom
frame corners; Gnoblin owns the frame geometry and clipping.

The `frame.mode` values are:

| Value           | Effect                                                                      |
| --------------- | --------------------------------------------------------------------------- |
| `off`           | Leave the titlebar to the application.                                      |
| `auto`          | Draw a Bingux frame only after the application requests server decorations. |
| `prefer-server` | Request a Bingux frame from apps that can negotiate decorations.            |
| `replace`       | Replace the matching application's titlebar with a Bingux frame.            |

See [Gnoblin's frame mode reference](https://gnoblin.org/guides/window_frames#choose-a-mode) for
details. Apps that cannot negotiate server decorations keep their own titlebar.
Gnoblin clips buffer pixels outside an app's declared visible bounds and draws
Bingux's configured border and shadow around those bounds. This removes
invisible CSD shadow margins without guessing their size or hiding controls.

Bingux also enables `remove_csd` on app windows. This handles rounded corner
cutouts inside the visible bounds: Gnoblin fills those pixels when it can
sample a safe background, then applies Bingux's shared rounded shape and
border. If corner detection is uncertain, it preserves the app's original
corner.

## Configure a frame in Gnoblin

To use Bingux's renderer for a selected app, add this after the
`gnoblin.load(...)` lines in `~/.config/gnoblin/init.lua`:

```lua
gnoblin.configure {
    frame_renderers = {
        bingux = {"bingux-frame", "--compact"},
    },
}

gnoblin.window_rule {
    match = {app_id = "^org%.gnome%.Nautilus$"},
    frame = {
        mode = "auto",
        renderer = "bingux",
        extents = {36, 0, 0, 0},
    },
    corners = {
        mode = "force",
        radius = 12,
        border_width = 1,
        border_color = "#505050bf",
        remove_csd = true,
    },
}
```

`app_id` is a regular expression matched against the app ID; use
`gnoblinctl window list` to find it. The example matches Files.
The `bingux` renderer name points to the `bingux-frame` command.

`extents` are logical pixels in top, right, bottom, left order. This example
reserves 36 pixels above the client area. Each extent accepts an integer from
0 to 256. See [Gnoblin's frame options](https://gnoblin.org/guides/window_frames#frame-options)
for defaults and the other rule fields.

The `--compact` renderer option selects GTK's short titlebar. It is the Bingux
default and pairs with 36-pixel top extents. Omitting it selects GTK's regular
header, whose height depends on the installed theme, fonts, and GTK version.

## Apply changes

Run `gnoblinctl config reload` after editing renderer registration or window
rules. Reload restarts the frame renderer while keeping app windows open.
Apps must update their Wayland surface to apply decoration changes; reopen
an app if its titlebar stays unchanged.

An update to Gnoblin's compositor code requires a new session.

Source builds can build the renderer with `make build/bingux-frame`. It requires
GTK4, libadwaita, and Wayland development packages. Bingux packages include the
renderer and its default Gnoblin integration.

## Renderer readback

For opaque GTK snapshots with supported frame extents, the helper reads the
decoration strips and bottom-corner guards. The guards preserve pixels inside
Gnoblin's rounded mask, and filtering margins include adjacent body pixels.

Translucent snapshots, clamped extents, and extreme unequal side extents use
full readback to preserve their pixels. The guard size covers Gnoblin's maximum
corner radius. For renderer development and coverage checks, see the
[frame helper guide](https://github.com/kierandrewett/bingux/blob/main/packages/bingux-frame/README.md).
