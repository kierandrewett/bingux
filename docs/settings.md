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

| Page | Backend | Notes |
| --- | --- | --- |
| Appearance | `bingux-config desktop.wallpaper`, GSettings `org.gnome.desktop.interface` | Accent colour and colour scheme are the keys GTK apps and the portal read. The window recolours at once. |
| Desktop, Search | `bingux-config` (`~/.config/bingux/bingux.toml`) | Same validation as `bingux-config set`. See [configuration](configuration.md). |
| Displays | `org.gnome.Mutter.DisplayConfig` over D-Bus, GSettings night light | Apply writes a persistent layout (`~/.config/monitors.xml`). |
| Sound | `wpctl` (WirePlumber) | Default devices, volumes, mute, application streams. |
| Network | `nmcli` (NetworkManager) | Wi-Fi, wired status, airplane mode. |
| Bluetooth | `bluetoothctl` (BlueZ) | Power, scan, pair, connect, remove. |
| Power | `powerprofilesctl`, `upower`, GSettings session and screensaver | Blank screen and lock delays. |
| Keyboard, Mouse | GSettings `org.gnome.desktop.peripherals.*`, `gnoblinctl input` | See the Gnoblin note below. |
| Date and Time | `timedatectl`, GSettings clock format, `bingux-config desktop.clock_seconds` | Time zone and manual time may ask for authorisation. |
| Users | AccountsService over D-Bus | Full name changes may ask for authorisation. |
| About | `/etc/os-release`, DMI, `gnoblinctl --version` | Read-only. |

Each page shows a readable error when its tool is missing or refuses the
request, and a Try again action where that makes sense.

## Gnoblin and GSettings

Gnoblin's Lua configuration can set pointer, touchpad and keyboard options.
When a field is set there to anything other than `"inherit"`, Gnoblin ignores
the matching GSettings key. The Keyboard and Mouse pages read the committed
snapshot (`gnoblinctl --json config show`) and show such rows read-only with
"Set by Gnoblin config". Keyboard layouts come only from Gnoblin's
`input_sources`; the Keyboard page lists and selects them with
`gnoblinctl input`.

GSettings needs a persistent backend (`dconf`) for values to survive logout.
The Bingux package requires it.

## Development

```sh
cargo build --manifest-path packages/bingux-settings/Cargo.toml
BINGUX_SETTINGS_BIN=target/debug/bingux-settings binguxctl settings sound
```

`make` builds the release binary with the other packages and
`scripts/install-shell.py` installs it with its desktop entry. The icons under
`packages/bingux-settings/icons/` are Adwaita symbolic icons with the
hard-coded fill removed so the app can recolour them; see `icons/COPYING`.
