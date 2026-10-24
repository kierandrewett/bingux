# Bingux Settings

`bingux-settings` is the Settings application for the Bingux desktop. It is a
native Wayland window (Rust, Freya) with the shell's look: a page list on the
left and, on the right, a rounded panel inset in the window. Each page starts
with a large title and one sentence that says what the page is for, then
boxed groups of rows. Below 740 px of
width the window is laid out as a phone. The page list is a screen of its own.
Choosing a page opens it, and the Back button in the header returns to the
list. A subpage's Back button returns to its page. The window buttons are
hidden, and rows, controls and buttons are bigger for a finger. Opening
Settings with no page name starts on the list; opening it
with a page name starts on that page.

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
audio streams while it plays or records, and a row for each kind of file it
can open, showing Default when it is the current choice. Pressing that row
opens the list for the kind. Every application row has a right-click menu
with Open, Show details and Uninstall.
Default applications, at the top of the page, is one row that shows the
default web browser. It opens the list of nine kinds: web links, mail,
calendar, files, text, documents, photos, music and video. Each kind shows
its current default, and has a search box. Pressing a kind opens the
applications that can open it, with a search box. Choosing one makes it the
default for every type in that kind. The choice shows at once and is written
with `gio mime` in the background, so GTK applications and `xdg-open` see it.
If the write fails, the previous default comes back and a message says why.
Each application in a kind's list has a right-click menu with Make default,
Open and Show details.

The Sound page starts with the output device, a dropdown of the devices that can play sound, and a Volume slider with its percentage and a mute button. Balance sits under the volume, with Left and Right labels beneath the slider. Test speakers plays a tone through each speaker to check the wiring. Allow volume above 100% lets the slider boost quiet audio past 100%, which can distort. The input device follows the same layout, then Applications lists one row for each application that plays or records sound, with its icon, name and percentage, a Playing or Recording state, the device it uses, a full-width volume slider and a mute button. Each application's icon comes from the icon the stream names when the icon theme has it, otherwise from the application's desktop entry.

Bluetooth uses subpages too. The list has two groups: My devices (paired or
remembered) and Nearby devices, which fill in while a scan runs. A scan runs in
25-second rounds while the page is open and Bluetooth is on. It stops when the
page closes or Bluetooth is turned off.
Each row shows an icon for the device type, its status and battery level when
known. Pressing a row opens Device details: the icon, name and status, then a
strip of actions (Pair or Connect, or Disconnect, and Forget once the device is
paired), the type, address, whether it is paired and connects automatically,
and its battery level. Forget asks first and returns to the list when the
device is gone. A device with no name shows as Unnamed device.

Users lists the accounts that AccountsService knows, with the signed-in person first. Each account is a row with its avatar, its full name, and its user name and type underneath (the type alone when the account has no full name). Pressing a row opens that account's page, titled with the account's name. The list and the account page show the data loaded last time at once, and a skeleton on first open.
The account page opens with a large initials tile, the full name and the account type. Account is one group: Full name, User name, Account type, Automatic login and Account locked. Full name is one row with the name on the right. Pressing it opens a sheet titled Change full name, with the name in a field and a Save button. A new name shows at once and is written in the background. If the write fails, the old name comes back and a toast says why.
Password appears only on the signed-in account. It is one row, Your password, with a Change password button. The button opens a sheet titled Change password, with Current, New and Retype fields and a Save button. A failed change keeps the fields and shows the reason in the sheet.
Each account row has a right-click menu with Open, and Change password for the signed-in account. If the list cannot load, the page shows the error with a Try again action.

Wi-Fi opens Network details from the settings button on a saved or connected
network, or from pressing the connected network. The page shows the signal
icon, the name and the state (Connected, Saved, Out of range), then a strip of
actions: Connect or Disconnect, and for a saved network Forget and Settings.
Forget asks first and returns to the list. Settings opens the connection
editor with the address, DNS and security tabs. Below the strip are the signal
strength in words, the frequency band, the security and the speed.
Date and Time opens the time zone as a searchable list of cities. The
zone list shows each UTC offset only for
the selected zone. Set date and time opens a sheet with Date (day, month and
year) and Time (hour and minute) rows and a Set button that stays disabled
until the values change.
The switches on the Date and time page and the clock format row also have a
right-click menu with the same change.

A subpage replaces the page. The header shows a Back button and the subpage
title is the large title at the top of the page. Back, Alt+Left and the mouse
Back button return to the page below; choosing another page in the sidebar
closes the subpage.

## Where each setting lives

Settings has no store of its own. Every page reads and writes the system
that owns the setting, so changes made elsewhere show up here and the other
way round.

| Page            | Backend                                                                                                                                                                                   | Notes                                                                                                                                                                                                                                                                                                                |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Appearance      | `bingux-config desktop.wallpaper`, `appearance.font_family`, GSettings `org.gnome.desktop.interface`                                                                                      | The font is picked from the installed families on a subpage. Accent colour and colour scheme are the keys GTK apps and the portal read, and are hidden without gsettings.                                                                                                                                            |
| Desktop, Search | `bingux-config` (`~/.config/bingux/bingux.toml`)                                                                                                                                          | Same validation as `bingux-config set`. See [configuration](configuration.md).                                                                                                                                                                                                                                       |
| Displays        | `org.gnome.Mutter.DisplayConfig` over D-Bus, GSettings night light                                                                                                                        | Apply tries the layout for 15 seconds and reverts unless kept; Keep writes a persistent layout (`~/.config/monitors.xml`). Each display is a row that opens its own page. Night light can follow a schedule.                                                                                                         |
| Sound           | `wpctl` (WirePlumber)                                                                                                                                                                     | Default devices (inline list, or a subpage with three or more), device kind icons from device properties, volumes, mute, and a row for each application.                                                                                                                                                             |
| Network         | `nmcli` (NetworkManager)                                                                                                                                                                  | Wi-Fi, airplane mode. Each wired connection and each VPN is a row that opens its details subpage, with the same connect and disconnect actions as its right-click menu.                                                                                                                                              |
| Bluetooth       | `bluetoothctl` (BlueZ)                                                                                                                                                                    | Power, visibility, scan, pair, connect, auto-connect, forget. Each device is a row that opens its Device details subpage.                                                                                                                                                                                            |
| Power           | `powerprofilesctl`, `upower`, GSettings session and screensaver                                                                                                                           | Blank screen and lock delays. The power mode cards apply at once and go back if the change is refused.                                                                                                                                                                                                               |
| Keyboard, Mouse | GSettings `org.gnome.desktop.peripherals.*`, `gnoblinctl input`                                                                                                                           | See the note below.                                                                                                                                                                                                                                                                                                  |
| Date and Time   | `timedatectl`, GSettings clock format, `bingux-config desktop.clock_seconds`                                                                                                              | Time zone and manual time may ask for authorisation.                                                                                                                                                                                                                                                                 |
| Users           | AccountsService over D-Bus                                                                                                                                                                | Full name changes may ask for authorisation.                                                                                                                                                                                                                                                                         |
| Applications    | Desktop entries in `XDG_DATA_DIRS`, `gio mime`, `wpctl`                                                                                                                                   | Installed applications with their icons, a page for each, and the default application for each kind of file.                                                                                                                                                                                                         |
| About           | `/etc/os-release`, DMI (`/sys/devices/virtual/dmi/id`), `/proc/cpuinfo`, `/proc/meminfo`, `lspci`, `/sys/class/drm`, `/proc/uptime`, `df`, `uname`, `gnoblinctl --version`, `hostnamectl` | Facts load in the background, and each tile shows a bar until its value arrives. The device card renames the device through `hostnamectl`, which may ask for authorisation, and copies a summary of every fact. Hardware and software values copy on right-click. Storage is the space used on the root file system. |

A page can replace the sentence under its title with its current state when
that helps (the connection on Wi-Fi and Network, the battery level on Power).
A failed change shows a toast instead of an inline message, and a page whose
tool is missing shows a readable error with a Try again action where that
makes sense.

On Wi-Fi, Network, Bluetooth and Displays, each item (a Wi-Fi network, a wired connection, a VPN, a Bluetooth device or a display) is a row that opens its own detail subpage, titled with the item's name. Right-clicking a network, device or display opens the same actions as its subpage.

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
it. The sidebar shows the signed-in person at the top (the full name from the
passwd entry); pressing it opens Users. The pages sit under group names
(Connections, Devices, Desktop, System). Search results appear under a
Settings name, and the person is hidden while a search is typed.

The Keyboard page has five groups. Key repeat holds Repeat keys, with Delay before repeat and Repeat speed nested under it and shown only while it is on, and a Try it box to test them. Typing holds Cursor blinking and Remember Num Lock state; Num Lock on at sign-in is nested under that switch and shown only while remembering is off. Special character entry holds Compose key and Alternate characters key. Input sources lists each layout or input method with its kind. Pressing a source opens its subpage, and Add an input source opens a sheet of the layouts and methods not yet in the list. Separate input per window sits under the list. Shortcuts has a summary row that shows how many shortcuts Gnoblin lists once it has answered, and opens a read-only list of every shortcut, grouped by what it acts on. Shortcuts are set in the Gnoblin Lua file, and the Configuration group opens it with Open file when its path is known. Each change shows at once and is written in the background. If the write fails, the control goes back to its old value and a toast says why.

## Mouse

The Mouse page has a Mouse group with Natural scrolling, Pointer speed, Left-handed buttons, Pointer acceleration and Double-click speed. Double-click speed is a slider shown in milliseconds. A Touchpad group appears only when a touchpad is found. It has Use touchpad, Tap to click, Natural scrolling, Secondary click, Scrolling, Pointer speed and Disable while typing. With Use touchpad off, the other touchpad rows are hidden. Secondary click and Scrolling are drawn cards: pick one and it applies at once. Test your settings has a box to check the double-click speed and a list to check scrolling. A setting that Gnoblin's Lua file sets shows its value and the subtitle Managed by your desktop configuration, and cannot be changed on the page. Each change shows at once and is written in the background. If the write fails, the control goes back to its old value and a toast says why; the page no longer shows an error line of its own.

## Sound

Sound lists the output and input devices as two dropdowns. Each device has a
headphone, headset or speaker icon before its name, so a device is easy to
find among similar names. Choosing a device makes it the default at once; if
the change is refused, the dropdown goes back and a toast says why.

Under the output's device, a slider sets its volume with a level meter below
it that shows what the output plays. The input has the same layout for what
it records. The meters run in the background and only while this page is
open.

When the output has left and right channels, Balance appears under the
volume. Its track has a centre mark, and the labels "Left" and "Right" sit at
its ends on one line. When it has front and rear channels, Fade appears too,
and a Subwoofer slider appears when it has a low-frequency channel. Setting a
slider back to its centre snaps it onto the mark.

If "Allow volume above 100%" is on, the volume slider runs to 150% with a
tick at 100%, the point where the sound starts to clip.

"Test speakers" opens a sheet with one button for each speaker of the output.
Each button plays a short tone through that speaker only, so a wrong wire or a
silent channel shows at once.

Each application that plays or records sound is a row with its own volume
slider and meter. Right-clicking the row offers Mute and Reset to 100%.

Volume and mute changes apply at once. The backend applies them in the
background, and a failure puts the slider back and shows a toast.

## Window frame

Bingux Settings draws its own header bar with the minimise, maximise and
close buttons, because the sidebar and the page panel form one
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

## Desktop and dock

The Desktop and dock page starts with a preview of the desktop: the top bar, the dock and the sidebar as they are set below. Right-click the dock or the sidebar in the preview for a menu that shows or hides it, then aligns the dock or chooses the sidebar's edge. Each choice shows at once and is written in the background, the same as the matching row.

Every setting is inline. No setting opens a subpage, because each one is a switch, a slider or a short fixed list.

- Dock: a switch. When it is on, Icon size (a slider from 32 to 80 px, written when the pointer is released) and Alignment (Left, Centre or Right) show beneath it.
- Sidebar: a switch. When it is on, Side of the screen (Automatic, Left, Right or Top) shows beneath it.
- Dock mouse actions: Click, Middle click, Scroll and Scroll direction.
- Top bar: Show system usage and Show seconds in the clock.
- Control Centre: one switch for each tile (VPN, Do not disturb, Night light, Power mode, Keep awake). All five are saved together.
- Privacy: Show my public IP address. It looks the address up with an online service.

A write that fails puts the previous value back and a toast says why.

## Appearance

The Appearance page starts with a preview of the wallpaper on the desktop. Right-click the preview for Choose picture and Use system wallpaper.

Wallpaper is one group. The first row shows the file name, with a small thumbnail, and Choose picture opens a file picker. If no picture is chosen, the row says the system wallpaper is in use. The second row, System wallpaper, goes back to the picture that came with the system. The picture is decoded in the background.

Font is one summary row that shows the current font on the right. Pressing it opens the Font subpage, which has the full list and search (see Font below). The font is not also shown inline.

Accent colour is a row of swatches. Colour scheme is three cards: Default, Light and Dark. Applications follow it; the desktop shell stays dark. Both use GSettings and are left out when it cannot be read, and a line then says they are not available on this system.

Accent colour and colour scheme show at once and are written with GSettings in the background. If the write fails, the previous choice comes back. The page shows the data it loaded last time at once, and a skeleton on first open.

## Font

Bingux Settings and the shell use `appearance.font_family` from `bingux.toml`.
When it is empty they use Adwaita Sans, or the system sans-serif when Adwaita
Sans is not installed. GNOME's font setting is not read.

The Font page has one toolbar row and one group of rows. The search field fills
the width; the category dropdown at its right filters the list (All, Sans
serif, Serif, Monospace, Other). The group's description is the font count.
Each row shows the family name in its own face, with a muted "The quick brown
fox" sample beneath it in the same face. The chosen font has a tick. Right-click
a row to use it or copy its name. The list fills in chunks, and `fc-list` runs
in the background, so the page never blocks while it loads. Choosing a font
applies it at once; if the write fails the page reverts and a toast says so.

The shell follows the change at once; Bingux Settings reads it at start. Set it
from the command line with `bingux-config set appearance.font_family Cantarell`.
"System default" clears it.

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
the subtitle "Managed by your desktop configuration". Keyboard layouts and input
methods come only from Gnoblin's `input_sources` table in the Lua file. The
Keyboard page lists them under Input sources with their names from the XKB
database (`evdev.xml`) and IBus. Its Add sheet chooses one from those lists;
pressing a source opens its subpage, where Move up, Move down and Remove act
on it; the same three are in its right-click menu. Each change rewrites only
that table, keeps the file's comments, and asks Gnoblin to reload it; the file
is restored if Gnoblin refuses it. A table in a form the
page cannot rewrite is left alone and the page says so. Keyboard shortcuts are
set in the same Lua file. When the file path is known, the Keyboard page's "Open
file" button opens it with `xdg-open`. When Gnoblin is not running, the page
says so in one line.

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
another window size, for example `390x780` for the phone layout (set
`BINGUX_SETTINGS_SNAPSHOT_LIST=1` to start on the list of pages) or
`1000x1500` to see a whole long page.
