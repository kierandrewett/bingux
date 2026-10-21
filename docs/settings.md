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
`users`, `privacy`, `about`. An unknown or omitted panel opens the first page.

Set `BINGUX_REDUCED_MOTION=1` to disable animations.

## Where each setting lives

Settings has no store of its own. Every page reads and writes the system
that owns the setting, so changes made elsewhere show up here and the other
way round.

| Page            | Backend                                                                      | Notes                                                                                                    |
| --------------- | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Appearance      | `bingux-config desktop.wallpaper`, GSettings `org.gnome.desktop.interface`   | Accent colour and colour scheme are the keys GTK apps and the portal read. The window recolours at once. |
| Desktop, Search | `bingux-config` (`~/.config/bingux/bingux.toml`)                             | Same validation as `bingux-config set`. See [configuration](configuration.md).                           |
| Displays        | `org.gnome.Mutter.DisplayConfig` over D-Bus, GSettings night light           | Apply writes a persistent layout (`~/.config/monitors.xml`).                                             |
| Sound           | `wpctl` (WirePlumber)                                                        | Default devices, volumes, mute, and a volume slider for each application stream.                         |
| Network         | `nmcli` (NetworkManager)                                                     | Wi-Fi, wired status, airplane mode.                                                                      |
| Bluetooth       | `bluetoothctl` (BlueZ)                                                       | Power, scan, pair, connect, remove.                                                                      |
| Power           | `powerprofilesctl`, `upower`, GSettings session and screensaver              | Blank screen and lock delays.                                                                            |
| Keyboard, Mouse | GSettings `org.gnome.desktop.peripherals.*`, `gnoblinctl input`              | See the Gnoblin note below.                                                                              |
| Date and Time   | `timedatectl`, GSettings clock format, `bingux-config desktop.clock_seconds` | Time zone and manual time may ask for authorisation.                                                     |
| Users           | AccountsService over D-Bus                                                   | Full name changes may ask for authorisation.                                                             |
| Privacy         | `~/.config/bingux/portal-permissions.toml` (`bingux-portal` store)           | Rules that allow, ask or deny portal requests. Read and written atomically. See [Privacy](#privacy).     |
| About           | `/etc/os-release`, DMI, `gnoblinctl --version`                               | Read-only.                                                                                               |

Each page shows a readable error when its tool is missing or refuses the
request, and a Try again action where that makes sense.

Desktop has an Introduction group with a **Show introduction** action. It asks the
running shell to reopen the first-login guide (`binguxctl ipc introduction show`),
so it needs a running Bingux shell and reports an error otherwise.

The Displays page has an Identify group with an **Identify** action. It asks the running shell to label each display with its number and connector, using `binguxctl identify show`. Each display group shows the same number in its Number row. Like Show introduction, it needs a running Bingux shell and reports an error otherwise.

## Privacy

The Privacy page edits the portal permission rules in
`$XDG_CONFIG_HOME/bingux/portal-permissions.toml` (default
`~/.config/bingux/portal-permissions.toml`). The `bingux-portal` crate reads
and writes the file, so the page and the portal backend share one schema.

- **Default** sets the level for requests that no rule decides: Ask or Deny.
  Allow is not offered as a default.
- **Rules** lists each rule with its selectors, capabilities and level. Add,
  edit, delete, and move rules up or down. Order matters.
- A matching deny always wins. Otherwise the last matching rule wins.
- Capabilities are Screen cast, Remote desktop, Input capture, Screenshot and
  Access. Monitors apply only to Screen cast and Remote desktop. Devices and
  Clipboard apply only to Remote desktop.
- Each rule needs a name, at least one capability, and an app ID, a program
  path (absolute) or a pattern. Errors show inline and nothing is saved.
- A missing file means the default applies. Saving the first rule creates it.
- A file that cannot be read is shown as an error with a Try again action. The
  page does not overwrite it, and portal requests stay denied until it is
  fixed.

Saves write a temporary file and rename it over the old one, so a crash does
not leave a half-written file. The file is private to the user.

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
