# Bingux Settings

`bingux-settings` is the Settings application for the Bingux desktop. It is a
native Wayland window (Rust, Freya) with the shell's look: a header bar,
a page list on the left and boxed groups of rows on the right. Below 740 px of
width the page list becomes an overlay opened from the header button.

## Open it

- Search for "Settings" or open it from the dock and the Control Centre.
- Right-click the wallpaper for Change Background… or Display Settings.
- From a terminal: `bingux-settings [PANEL]` or `binguxctl settings [PANEL]`.

Panels: `network`, `bluetooth`, `background` (Appearance), `desktop`,
`search`, `display`, `sound`, `power`, `keyboard`, `mouse`, `datetime`,
`users`, `about`. An unknown or omitted panel opens the first page.

Set `BINGUX_REDUCED_MOTION=1` to disable animations.

## Where each setting lives

Settings has no store of its own. Every page reads and writes the system
that owns the setting, so changes made elsewhere show up here and the other
way round.

| Page            | Backend                                                                                              | Notes                                                                                                                                          |
| --------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Appearance      | `bingux-config desktop.wallpaper`, `appearance.font_family`, GSettings `org.gnome.desktop.interface` | The font is Bingux's own setting. Accent colour and colour scheme are the keys GTK apps and the portal read, and are hidden without gsettings. |
| Desktop, Search | `bingux-config` (`~/.config/bingux/bingux.toml`)                                                     | Same validation as `bingux-config set`. See [configuration](configuration.md).                                                                 |
| Displays        | `org.gnome.Mutter.DisplayConfig` over D-Bus, GSettings night light                                   | Apply writes a persistent layout (`~/.config/monitors.xml`).                                                                                   |
| Sound           | `wpctl` (WirePlumber)                                                                                | Default devices, volumes, mute, and a volume slider for each application stream.                                                               |
| Network         | `nmcli` (NetworkManager)                                                                             | Wi-Fi, wired status, airplane mode.                                                                                                            |
| Bluetooth       | `bluetoothctl` (BlueZ)                                                                               | Power, scan, pair, connect, remove.                                                                                                            |
| Power           | `powerprofilesctl`, `upower`, GSettings session and screensaver                                      | Blank screen and lock delays.                                                                                                                  |
| Keyboard, Mouse | GSettings `org.gnome.desktop.peripherals.*`, `gnoblinctl input`                                      | See the Gnoblin note below.                                                                                                                    |
| Date and Time   | `timedatectl`, GSettings clock format, `bingux-config desktop.clock_seconds`                         | Time zone and manual time may ask for authorisation.                                                                                           |
| Users           | AccountsService over D-Bus                                                                           | Full name changes may ask for authorisation.                                                                                                   |
| About           | `/etc/os-release`, DMI, `gnoblinctl --version`                                                       | Read-only.                                                                                                                                     |

Each page shows a readable error when its tool is missing or refuses the
request, and a Try again action where that makes sense.

## Window frame

Bingux Settings draws its own header bar with the page title and the
minimise, maximise and close buttons, because the sidebar and header form one
layout. It does not ask for server decorations, so Gnoblin's `auto` frame mode
adds no titlebar. Gnoblin still clips the window and draws the corners,
border and shadow, so the app window itself is square and opaque. See
[window frames](window-frames.md).

## Font

Bingux Settings and the shell use `appearance.font_family` from `bingux.toml`.
When it is empty they use Adwaita Sans, or the system sans-serif when Adwaita
Sans is not installed. GNOME's font setting is not read. Set it on the
Appearance page or with `bingux-config set appearance.font_family Cantarell`.
The shell follows the change at once; Bingux Settings reads it at start.

## Gnoblin and GSettings

Gnoblin's Lua configuration can set pointer, touchpad and keyboard options.
When a field is set there to anything other than `"inherit"`, Gnoblin ignores
the matching GSettings key. The Keyboard and Mouse pages read the committed
snapshot (`gnoblinctl --json config show`) and show such rows read-only with
"Set by Gnoblin config". Keyboard layouts come only from Gnoblin's
`input_sources`; the Keyboard page lists and selects them with
`gnoblinctl input`. Keyboard shortcuts are set in the same Lua file; the
Keyboard page's Open config action opens it with `xdg-open`, and
`gnoblinctl config reload` applies the edit. When Gnoblin is not running
these rows say so in one line.

GSettings needs a persistent backend (`dconf`) for values to survive logout.
The Bingux package requires it.

## Development

```sh
cargo build --manifest-path packages/bingux-settings/Cargo.toml
BINGUX_SETTINGS_BIN=target/debug/bingux-settings binguxctl settings sound
```

Pages find `bingux-config` through `BINGUX_CONFIG_BIN` when that is set,
else on `PATH`, else beside the `bingux-settings` binary, which is where
`make` and the package put it.

`make` builds the release binary with the other packages and
`scripts/install-shell.py` installs it with its desktop entry. The window is
built from [bingux-ui](bingux-ui.md), the design system crate that holds the
theme, icons, rows, controls and overlays; the app itself contains only the
pages and their backends.

To check every page without a compositor, build with the `snapshot` feature
and render the pages to PNG files. Each page reads the real system, so the
images show what the machine running the command reports:

```sh
cargo run --manifest-path packages/bingux-settings/Cargo.toml --features snapshot -- \
    --snapshot /tmp/bingux-settings-snapshots            # every page
cargo run --manifest-path packages/bingux-settings/Cargo.toml --features snapshot -- \
    --snapshot /tmp/bingux-settings-snapshots sound,about # some pages
```

Set `BINGUX_SETTINGS_SNAPSHOT_CLICK="x,y[;x,y]"` to click those window
points after the page loads and before the capture. This drives a real
write; for example clicking the Repeat keys switch on the Keyboard page
changes `org.gnome.desktop.peripherals.keyboard repeat`, which `gsettings
get` then shows. Set `BINGUX_SETTINGS_SNAPSHOT_SIZE="WxH"` to capture at
another window size, for example `600x640` for the overlay sidebar or
`1000x1500` to see a whole long page.
