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
`users`, `applications`, `about`. An unknown or omitted panel opens the
first page. `PANEL/SUBPAGE` opens a subpage: `applications/default` opens
Default applications, and `applications/` followed by an application's
desktop id, for example `applications/org.gnome.Nautilus`, opens that
application.

Set `BINGUX_REDUCED_MOTION=1` to disable animations.

While Settings is open, right-click its dock icon to open Network,
Bluetooth, Displays, Sound, Appearance or Power. These are the desktop
entry's actions; each one runs `bingux-settings` with that panel, and the
dock shows an application's actions only while it has a window.

## App identity

The dock, the window switcher, search results and notifications find the
application icon from the desktop entry. `bingux-settings.desktop` sets
`Icon=bingux-settings` and `StartupWMClass=bingux-settings`, and the window's
app id is `bingux-settings`, so a running window groups under its dock icon.
The icon is `packages/bingux-settings/bingux-settings.svg`, a full-colour
128 x 128 icon in the Adwaita app-icon style the dock shows for other
applications. `scripts/install-shell.py` and the RPM install it as
`share/icons/hicolor/scalable/apps/bingux-settings.svg`; GTK finds it through
`XDG_DATA_DIRS`, which the shell's launcher sets to include the install
prefix. The window itself sets no icon, because Wayland takes it from the
desktop entry.

## Applications and subpages

Applications lists every installed application with its full-colour icon and, under its name, its first category in plain words (for example Media or Office).
Search filters the list by name. Pressing a row opens that application's
page, with its icon and name, an Open button, a volume slider for each of its
audio streams while it plays or records, and the kinds of file it can open
with a Make default button for each one it is not yet the default for.
Default applications, at the top of the page, sets the default application
for web links, mail, calendar, files, text, documents, photos, music and
video; each choice shows the application icons. Changes are written with
`gio mime`, so GTK applications and `xdg-open` see them.

The Sound page shows one row for each application that plays or records sound,
with its icon, a Playing or Recording state, the device it uses, a volume
slider and a mute button. An application's icon comes from the icon the stream
names when the icon theme has it, otherwise from the application's desktop entry.

Bluetooth uses subpages too. The list has two groups: My devices (paired or
remembered) and Nearby devices, which fill in while a scan runs. A scan runs in
25-second rounds while the page is open and Bluetooth is on. It stops when the
page closes or Bluetooth is turned off.
Each row shows an icon for the device type, its status and battery level when
known. Pressing a row opens the device's page: the icon, name and status, the
one action it allows (Pair or Connect, or Disconnect), the type, address,
whether it is paired and connects automatically, its battery level, and Forget
device. Forget device asks first and returns to the list when the device is
gone. A device with no name shows as Unnamed device.

Users opens an account's page the same way: full name, user name, account
type and Change password. The account page opens with a large initials tile, the
full name and the account type.

Network shows an information button on each Wi-Fi row. It opens the network's
page: its state, Connect or Disconnect (with a password row when the network
needs one), signal, security, band and speed, and Forget network for a saved
network. Date and Time opens the time zone as a searchable list of cities, and
Set date and time as steppers for day, month, year, hour and minute.

A subpage replaces the page and puts a Back button and the subpage title in
the header. Back, Alt+Left and the mouse Back button return to the page
below; choosing another page in the sidebar closes the subpage.

## Where each setting lives

Settings has no store of its own. Every page reads and writes the system
that owns the setting, so changes made elsewhere show up here and the other
way round.

| Page            | Backend                                                                                              | Notes                                                                                                                                                                     |
| --------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Appearance      | `bingux-config desktop.wallpaper`, `appearance.font_family`, GSettings `org.gnome.desktop.interface` | The font is picked from the installed families on a subpage. Accent colour and colour scheme are the keys GTK apps and the portal read, and are hidden without gsettings. |
| Desktop, Search | `bingux-config` (`~/.config/bingux/bingux.toml`)                                                     | Same validation as `bingux-config set`. See [configuration](configuration.md).                                                                                            |
| Displays        | `org.gnome.Mutter.DisplayConfig` over D-Bus, GSettings night light                                   | Apply tries the layout for 15 seconds and reverts unless kept; Keep writes a persistent layout (`~/.config/monitors.xml`). Night light can follow a schedule.             |
| Sound           | `wpctl` (WirePlumber)                                                                                | Default devices (inline list, or a subpage with three or more), device kind icons from device properties, volumes, mute, and a row for each application.                  |
| Network         | `nmcli` (NetworkManager)                                                                             | Wi-Fi, wired status, airplane mode.                                                                                                                                       |
| Bluetooth       | `bluetoothctl` (BlueZ)                                                                               | Power, visibility, scan, pair, connect, auto-connect, forget.                                                                                                             |
| Power           | `powerprofilesctl`, `upower`, GSettings session and screensaver                                      | Blank screen and lock delays.                                                                                                                                             |
| Keyboard, Mouse | GSettings `org.gnome.desktop.peripherals.*`, `gnoblinctl input`                                      | See the note below.                                                                                                                                                       |
| Date and Time   | `timedatectl`, GSettings clock format, `bingux-config desktop.clock_seconds`                         | Time zone and manual time may ask for authorisation.                                                                                                                      |
| Users           | AccountsService over D-Bus                                                                           | Full name changes may ask for authorisation.                                                                                                                              |
| Applications    | Desktop entries in `XDG_DATA_DIRS`, `gio mime`, `wpctl`                                              | Installed applications with their icons, a page for each, and the default application for each kind of file.                                                              |
| About           | `/etc/os-release`, DMI, `gnoblinctl --version`                                                       | Read-only.                                                                                                                                                                |

Each page shows a readable error when its tool is missing or refuses the
request, and a Try again action where that makes sense.

## Search

The sidebar search finds pages first, then settings. Each settings row shows
the page it lives on under its title. Up and Down move through the results,
Enter opens the highlighted one, and Esc clears the search. At most 20 results
show. Ctrl+F moves focus to the search field. The list of searchable settings
is `src/settings_index.rs`; add a row there when a page gains a setting, with
plain words a person might type in the keywords column.

## Related pages

Each top-level page ends with a Related group of two other pages that usually
go with it. The pairs come from `PageId::related()` in `src/pages/mod.rs`.
Choosing one opens that page at its top level. Subpages do not show the group.

## Keyboard

Ctrl+Q and Ctrl+W close the window. Ctrl+F focuses the search, and Esc clears
it. The sidebar has no group headings: a hairline divides the groups instead.

## Window frame

Bingux Settings draws its own header bar with the page title and the
minimise, maximise and close buttons, because the sidebar and header form one
layout. It does not ask for server decorations, so Gnoblin's `auto` frame mode
adds no titlebar. Gnoblin still clips the window and draws the corners,
border and shadow, so the app window itself is square and opaque. See
[window frames](window-frames.md).

## Scrolling

A wheel notch scrolls 64 px. The motion follows Firefox's smooth scrolling: it
starts quickly, glides softly while the wheel keeps turning and settles fast
once it stops. Turning the wheel the other way, or stopping, takes effect at
once. Touchpad movement follows the fingers 1 to 1. Dragging the page
does not scroll it. To tune the feel without a rebuild, set `BINGUX_WHEEL_STEP`
(pixels per notch), `BINGUX_TOUCHPAD_FACTOR` (touchpad speed, 1 is direct) or
`BINGUX_SCROLL_SPEED` (1 is the default, 2 is twice as quick, 0.5 twice as
soft; 0.25 to 8) before starting Settings.
These come from a small patch to Freya kept in `packages/vendor`.

## Font

Bingux Settings and the shell use `appearance.font_family` from `bingux.toml`.
When it is empty they use Adwaita Sans, or the system sans-serif when Adwaita
Sans is not installed. GNOME's font setting is not read. Pick it from Font on
the Appearance page, which lists the families `fc-list` reports, or set it with
`bingux-config set appearance.font_family Cantarell`. "System default" clears it.
The shell follows the change at once; Bingux Settings reads it at start.

## Where a setting is stored

A setting that GNOME already defines stays in GSettings, because GTK
applications, the portal and Gnoblin read that key: accent colour, colour
scheme, night light, key repeat, pointer options, power timeouts and the clock
format. Bingux Settings writes the key and does not copy it anywhere else. A
setting that GNOME does not define goes in `bingux.toml`, written through
`bingux-config`: the wallpaper, dock, sidebar, search and the font. A new
setting follows the same rule. Without gsettings the pages that need it say so
and the rest keep working.

## Gnoblin and GSettings

Gnoblin's Lua configuration can set pointer, touchpad and keyboard options.
When a field is set there to anything other than `"inherit"`, Gnoblin ignores
the matching GSettings key. The Keyboard and Mouse pages read the committed
snapshot (`gnoblinctl --json config show`) and show such rows read-only with
the subtitle "Managed by your desktop configuration". Keyboard layouts come
only from Gnoblin's `input_sources`. The Keyboard page shows each layout by its
name from `/usr/share/X11/xkb/rules/base.lst` and selects them with
`gnoblinctl input`. Keyboard shortcuts are set in the same Lua file. When the
file path is known, the Keyboard page's "Open file" button opens it with
`xdg-open`. When Gnoblin is not running, the page says so in one line.

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
