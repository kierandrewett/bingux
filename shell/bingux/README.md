# Quickshell desktop

For other desktops, see [portable testing](../../docs/portable-testing.md).
Use `scripts/test-desktop` for an isolated nested Sway session.

Run this directory with `qs --path shell/bingux`. Use a Quickshell package built
against its runtime Qt version. Mixing distro Qt updates with an older
Quickshell binary can cause crashes.

`Theme.qml` defines shared colours, spacing, icon sizes and typography.
Nested rectangular backgrounds use `Theme.insetRadius(outerRadius, padding)`:
`max(0, outerRadius - padding)`. Bind this to the actual parent radius and inset.
For example, a 20px popup with 8px padding gives its menu rows a 12px radius.
Circular indicators, slider handles and pills retain their circular geometry.
Top bar controls use full-height hit targets and hover-only backgrounds.
Volume, mute, microphone, display brightness, and keyboard brightness requests
show a temporary OSD above the dock. It uses the shared shell styling, displays
the device name when available, eases the level bar, and fades away without
scaling. Bingux disables Gnoblin's native OSD and installs a compatibility event
bridge for running compositors that predate the native OSD signal.
Dock buttons are square, with icons centred independently of their indicators.
Pinned apps come first, followed by unpinned running apps after a divider.
The divider appears only when both sections contain apps. Drag an icon to reorder
it, or focus it and press Ctrl+Left / Ctrl+Right to reorder within its section.
Dragging a running app into the pinned section shows a pin badge; dropping saves
the pin and position. Dropping a pinned app into the running section unpins it.
The order is saved through Qt Settings in `~/.config/gnoblin/dock.ini`.
App menus offer Pin to dock / Unpin from dock. Pins use the desktop-entry ID and
share that settings file, so they survive reloads and restarts. Unpinning keeps
running windows in the dock and can override a profile's configured pin. Apps
without a desktop entry cannot be pinned because they have no saved launcher.

Menus use `ShellPopup.qml`, a transparent layer surface with a rounded content
card. This avoids the native xdg-popup grab ordering failure in older Gnoblin
builds. Outside clicks and Escape dismiss menus. Opening scales the shared card from 90%
to full size over 160 ms. Closing fades it to zero over 120 ms without changing
its scale, including when an opening animation is interrupted. Pointer and
keyboard input are released at the start of closing. Reduced motion makes both
transitions immediate. Reopening during a fade resumes from the current state.
Gnoblin routes keyboard input for the `gnoblin-shell-popup` namespace separately
from application activation, keeping the underlying window and dock indicator
active while menus receive keys. This requires the compositor's menu keyboard
routing support; installing a new compositor build takes effect at next login.
The outside-click surface excludes the card so pointer re-entry remains usable.

Use `ShellPopup` for new shell context menus and popovers. Put the contents in
its default slot and set `popupWidth`, `popupHeight`, and `preferredX/Y` as needed.
Set `visible` to request opening or closing; the component owns window lifetime,
input handling, surface styling, and motion. Top-bar popovers open from their top
edge. Dock menus set `revealOriginY: popupHeight` to open from their bottom edge.
Use `MenuNavigator` for action navigation and `contentRadius` for nested controls.
Keep data-specific menu entries in their caller; do not create another window or
animation for each menu. Tray, keyboard, dock, calendar, and control-centre menus
all use this component.

Run `QUICKSHELL_BIN=/path/to/matching/quickshell tests/popup-motion.sh` for real
window, pointer, keyboard, interruption, and reopening checks. Repeat with
`BINGUX_REDUCED_MOTION=1` for immediate transitions. For unattended checks, run
the script through Gnoblin's `GNOBLIN_TEST_DBUS_CLIENT` launcher so desktop input
cannot dismiss the test menus.

Tray submenus use the same surface and provide a Back action. `DockTooltip.qml` provides a non-interactive
application tooltip after a 500 ms initial hover delay. Moving to another tooltip target
skips the delay until one second after the last tooltip closes. Every open and close
keeps the same 160 ms fade and scale animation, including immediate target switches.
All tooltips share the dock bubble styling, and surfaces stay mapped until the exit finishes.

Click the clock for the calendar, the system indicators for the control centre,
and the search icon for the floating search panel. Search needs bingux-searchd;
application launching also needs an applicationLauncher command in search.json.
The control centre uses PipeWire for volume and the existing system settings
applications for network and display configuration.

Win+Period opens the emoji picker through Gnoblin's persistent shortcut bridge.
The desktop module leaves IBus's inline picker on Win+Semicolon so it cannot
intercept Win+Period; Ctrl+Shift+U Unicode entry is unchanged.
Search by name, category or aliases such as `lol`; arrows select, Enter/click
copies, and Escape/outside-click dismisses. The last 32 choices are stored locally.
`qs ipc -p shell/bingux call emoji open` is also available. The bundled Unicode
17.0 catalogue works offline; regenerate it with `scripts/generate-emoji-data.py`
using Unicode's `emoji-test.txt`. Its licence is `emoji-data.LICENSE`.

## Dock media controls and badges

Apps with a matching MPRIS player show artwork, title, artist, previous,
play/pause, next, and a seek bar inside the existing dock menu. Controls follow
the player's capabilities and keep the menu open. Hover previews the seek time
and fills the track in grey without changing playback. Clicking or dragging
seeks on release; a track change during a drag cancels it. Scrolling accumulates
five seconds per notch and eases the bar toward the target, coalescing rapid
seek requests. The time button toggles elapsed and remaining time. Keyboard
activation and arrow-key seeking work too.

Click the artwork to animate it into a full-width square above the controls;
click again to collapse it. Covers crossfade, play/pause morphs, and long titles
use the shared marquee with fading edges. Hover colours change immediately.
Reduced-motion mode disables the motion and keeps long titles elided.

`MediaControls.qml` is shared by the dock and control centre; `DockMediaControls`
is its compatibility name. `AlbumArtCache` retains at most twelve 512px covers
in Qt's in-memory image cache, with matching decode settings across consumers.
This avoids repeat downloads when a menu is reopened. The OS icon renderer
samples each app icon's palette for its player accent and speaker badge;
accent buttons derive lighter hover/pressed colours and a contrasting icon.

`MediaMatch.js` matches the player's `DesktopEntry` against the dock group's
desktop entry, startup class, or window app ID. Missing desktop IDs fall back to
an exact MPRIS bus identity. Controls never default to an unrelated player.
Notification badges count that app's outstanding shell notifications and clear
when cleared in the control centre or dock preview, or withdrawn by the app;
toast expiry preserves both the notification and its count. They do not invent an app's internal unread
count. Senders should supply the standard `desktop-entry` hint. Legacy senders
can match by exact app name.

The dock menu shows that app's retained notifications as a flat, scrollable list.
`NotificationStack.qml` owns the cards in the dock, desktop toasts and control centre:
app icons, timestamps, text, actions, hover controls and dismissal motion share one
implementation.
Incoming desktop notifications arrive as separate cards. Notifications group by
app only in the notification centre. Every notification view uses the same
110ms eased wheel scrolling with 160px per notch. Touchpad pixel deltas retain
their distance, rapid inputs accumulate, and reduced motion scrolls immediately. Group expansion
uses a 180ms transition and renders only cards near the viewport, so large groups
do not animate every card through the same small area. Card models and actions
remain available when their rendering is outside the viewport.

Collapsed groups show at most four cards, including the front card. Extra
notifications add no layers or spacing; the count and expanded list include them all.

The desktop notification scene excludes the sidebar on its screen. Toasts slide
from the edge of that remaining desktop area. The whole notification centre,
including its footer, slides in and out together at the right edge with 300ms
cubic easing. Its closing fade stays visible through the slide.
Its animation and dismissal input stay within the same clipped area.
Sidebar resizing moves the scene without replacing its cards.

The dock uses full-width cards and the menu's existing spacing, without stacked-card padding.
Widgets inside dock menus use `Theme.menuWidgetBackground` and `Theme.menuWidgetRadius`,
with no card borders or shadows. Dock notifications appear immediately. Their X
control fades and collapses a card vertically while the remaining cards and menu
height follow its size. Drag dismissal is disabled in the dock. Reduced motion
removes this animation. Desktop and control-centre swipe behaviour stays enabled.
Notification application icons come from the matching desktop entry, including
exact window-class matches for Electron variants such as Discord Canary. Badge numerals roll up and down, with immediate updates under reduced motion.

Speaker badges use `bingux-audio-meter`, which monitors per-stream peak levels
through the PulseAudio protocol supported by PipeWire. It retains no audio.
A stream must exceed -50 dBFS for 250ms to activate; an active badge holds above
-60 dBFS and through 1.2 seconds of silence, then fades out over two seconds.
Muted, paused and silent streams do not activate it. Process or desktop identity
outranks generic runtime names, avoiding Chromium/Electron misattribution.
The standalone install supplies `BINGUX_AUDIO_METER` through the compiled helper
on PATH. Source sessions can set the variable to an explicit helper path. The
helper reconnects after an audio-server restart.

Validation:

- `node --test tests/media-match.test.mjs` checks app attribution.
- Run `tests/dock-media.py` through Gnoblin's `GNOBLIN_TEST_DBUS_CLIENT` launcher
  with `QS_TEST_BIN` pointing to a matching Quickshell runtime. Set
  `GNOBLIN_TEST_DISABLE_NOTIFICATIONS=1` and
  `GNOBLIN_TEST_GSETTINGS_BACKEND=keyfile` to give the fixture its notification
  bus name. It verifies real MPRIS/notification calls, artwork download reuse,
  badge motion, expanding artwork and keyboard/pointer controls.
- `python3 tests/audio-meter.py /path/to/bingux-audio-meter` creates and removes
  an owned null sink to verify silence, short pulses, debounce and release
  timing without playing test audio through the speakers.

## Sidebar

Text selections use `Theme.textSelection`, the accent colour at 28% opacity,
across notes, search and filter fields, tasks, capture settings and the terminal.

The compact header dropdown selects Terminal, Notes, System, Calendar, Media or Tasks.
Its opaque menu also contains the left/top/right position controls. Notes format Markdown in place as you type: headings, lists, quotes, bold,
italic, strikethrough, links and code. Use Ctrl+B/Ctrl+I for bold/italic.
Right-click or press Shift+F10 for edit actions, six heading levels and block/text formatting.
Heading shortcuts preserve neighbouring blocks; one undo reverses a complete
formatting operation, including its internal fragment edits.
A dim syntax hint for the active line appears at the bottom of the editor without
adding a gutter or inserting markers into the saved text.
There is one editable view; notes save as Markdown locally in
`~/.config/bingux/sidebar-notes.ini`; System uses the same metrics feed as the
bar. Calendar shows connected calendar events, Media stacks each player with its own artwork and
playback controls, and Tasks saves a local checklist in `~/.config/bingux/sidebar-tasks.ini`.
Switching views preserves the terminal session. Only opening Terminal starts a shell. Open/closed state, selected view, placement and size
are saved immediately and restored when Quickshell starts. Notes remain saved
across restarts. A restored Terminal panel starts a new shell process.
System shares the top bar's CPU, memory and network history graphs, with
hover inspection and a saved one- or five-minute range. CPU and memory share
a percentage graph; network readouts sit together, with a compact grid for
the extra System readings.
Run `tests/sidebar-notes.sh` for Markdown and immediate-save checks, and
`tests/sidebar-notes.sh sidebar-system` for responsive layout and range restoration.
`tests/sidebar-notes.sh sidebar-panels` checks checklist persistence, calendar navigation,
media playback and panel widths.
Choose **Pop out window** in the header menu to move the existing panel into a
resizable window; **Dock in sidebar** brings it back. This preserves the editor
and running terminal process. Closing the window hides it and restores the edge handle.
`tests/sidebar-notes.sh notes-context` checks menu actions and keyboard dismissal.
`tests/sidebar-notes.sh notes-headings` checks all six heading levels, changes to
existing headings, neighbouring formatting and undo/redo.
With the terminal plugin on `QML_IMPORT_PATH`, `tests/sidebar-notes.sh sidebar-popout`
checks native window input, reparenting and terminal process continuity.
The header aligns with the top bar and uses shared controls. Content has 16px
horizontal padding and 8px vertical padding. Square sidebar edges and inward desktop corners join it to the bar.
The desktop-facing divider follows the growing curve continuously; there is no
rectangular frame or internal border across the join.
The terminal uses a transparent background over the top bar's surface colour;
older already-loaded terminal plugins use that colour until Quickshell restarts.

Move the pointer to the right screen edge. A small swipe-style handle appears next to the
pointer. Press and drag inward to open: the terminal, edge handle, and reserved space follow the pointer.
Release to keep that size; releasing below 150px closes the sidebar. The selected
width and height are saved for subsequent openings. Side panels are capped at 30% of monitor width. The open resize edge has a 24px grab area. The pill stays visible while
open, eases toward the pointer along its resize edge after an initial 96px threshold, then tracks continuously until you leave the edge; it follows both axes while dragging; drag it again to resize, or shrink below 150px to close. Clicking only pulses
the pill as a hint to drag; it does not change the terminal. The pill smoothly fades out over
the bar and dock reservation, then returns in the usable area.
Use the Left, Top, and Right placement icons to move it
to the left, top (drop-down console), or right. The selection is saved in
`~/.config/bingux/sidebar.ini`. The sidebar uses the same screen as the top bar.

Side panels fill the entire screen height. The top bar and dock shrink and move
with the sidebar, then return to their original geometry when it closes.
The open terminal reserves space through layer-shell. Hide releases that space. The shell and running commands stay
alive when hidden or moved. Drag below 150px or use Ctrl+Shift+F12 to hide it. Ctrl+Shift+C/V
copy and paste; ordinary terminal shortcuts, including Escape, reach the shell.
After a shell exits, New shell starts another. Restarting Quickshell ends the
terminal session. The panel slides from the selected edge. Set
`BINGUX_REDUCED_MOTION=1` to show and hide it without motion.

Keyboard shortcuts and external controls can use `qs -c bingux ipc call sidebar
toggle` (also `open`, `hide`, `select terminal|notes|monitor`, and `edge top|left|right`). For a source-path session,
use `qs -p shell/bingux ipc call sidebar toggle` instead.

The terminal renders inside Quickshell through the Qt 6 QMLTermWidget plugin.
Its controls use Bingux's shared theme and ActionButton component. The terminal
plugin is optional; without it, the rest of the sidebar remains available.
Source-path runs need QMLTermWidget 2.0 built for Qt 6 on `QML_IMPORT_PATH`.
The terminal component loads only after a valid opening drag is released (or an explicit open command). Aborted drags never start a shell. Existing sessions resize and rewrap their text during subsequent drags. The terminal uses a custom Bingux palette and the same background as its panel; sidebar, dock, and top bar share their surface, outline, and corner treatment. A missing plugin shows a retry
message inside the panel and does not prevent the rest of the shell from loading.

Gnoblin compositor effects are configured in `~/.config/gnoblin/gnoblin.toml`.
The rebuilt compositor supports alpha-masked layer blur and per-window rules.
Dock icon rectangles are published through foreign-toplevel management so
minimise animations can target each icon. Those native changes need one new
login after installation. QML edits reload in the running Quickshell process.

## Verification

On the live Gnoblin session, verify:

- Calendar opens from the full-height clock target and closes outside or with Escape.
- Tray right-click opens a menu, submenus can return, and the shell stays connected.
- Dock drag ordering persists across a Quickshell restart.
- Tooltips fit their text and do not capture pointer input.
- Search shows application icons without a dim background.
- Control centre reports the current volume and device state.
- The journal has no QML load errors or Qt version mismatch warning.

The compositor repository contains isolated native tests for dock rectangles,
layer entrance animations, live rules, and pixel verification of masked blur.
Run those against the same installed Mutter and Shell prefix.

The sidebar's isolated integration test uses Quickshell, the Qt 6 terminal plugin,
and `grim`. Run it through Gnoblin's test launcher (it rejects live sessions):

```sh
QML_IMPORT_PATH=/path/to/qmltermwidget \
GNOBLIN_TEST_DBUS_CLIENT="$PWD/tests/sidebar-integration.py" \
/path/to/gnoblin/scripts/run-gnome-shell.sh
```

It checks pointer activation, keyboard input, all three exclusive work areas,
maximised-window resizing, terminal-session persistence, shell exit, and restart.

The Qt integration window resizes correctly. A separate GTK 4 test window kept
its previous size after the work area changed on the tested Gnoblin installs;
that client/compositor interaction remains unresolved.

For desktop launches, the Qt 6 QMLTermWidget plugin must match the Quickshell
runtime. Add its parent QML directory to `QML_IMPORT_PATH` inside the launcher,
after any environment cleanup. A running service needs restarting after changing
its import path; reloading QML alone does not update the process environment.

The top-bar system widget shows single-line, fixed-width rolling readings with one-minute sparklines.
Left-click opens CPU, memory and network history with one- and five-minute ranges;
hover a graph to inspect a recorded sample. Right-click (or Shift+F10) opens the
chooser for CPU, RAM, network receive and network send monitors; choices
are saved in `~/.config/bingux/monitors.ini`. Network rates aggregate traffic
across interfaces. At least one monitor stays visible so the chooser remains
accessible, and unavailable readings retain their width.
History is bounded to five minutes in memory. Missing samples and outages leave
gaps rather than artificial zero readings. CPU and memory charts use a fixed
0–100% scale; both network traces share a labelled scale based on the visible peak.
Both monitor surfaces have fixed Overview, Processes and Services tabs. Overview
scrolls beneath the tabs and arranges CPU, memory, swap, GPU, storage and network
in a responsive grid. CPU includes per-core graphs, temperature, load averages,
model, mean clock, uptime and kernel. GPU shows identity/driver, utilisation,
VRAM, temperature, power, clock and fan speed. Storage shows root/home capacity
and disk traffic. Unsupported sensors display a dash. Swap has its own graph.

Processes, Services and the Tailscale lists reuse `DataTable.qml` and
`TableRowSurface.qml`. Processes and Services use a full-height, virtualised table with sortable
columns, search, rounded headers and horizontal scrolling on narrow sidebars.
Click State to cycle each state group to the top in alphabetical order;
other column headers toggle ascending and descending order.
Wheel scrolling moves four rows per notch immediately, without momentum;
Shift+wheel scrolls horizontally. Ctrl/Shift-click, drag selection and Ctrl+A
select multiple rows; Escape clears selection. Rows retain their order while
hovered or selected, so updates do not move targets beneath the pointer.
Processes use installed app icons matched by executable, desktop ID or window
class. Right-click (or Shift+F10) offers Copy PID, Pause, Resume, End process and
Force quit for the selection. Signal actions use PID file descriptors and verify
recorded start times to reject reused PIDs. Permission errors appear in the panel.
Process CPU uses one logical core as 100%, so workloads may exceed 100%.
Services lists user or system units and offers Start, Stop and Restart for the
selection, subject to the current user's permissions.

The daemon samples sensors every two seconds and caches process scans for six
seconds, publishing up to 8192 readable processes. It reads process names, not
command-line arguments. Services refresh every ten seconds while visible.
Desktop focus events do not add duplicate graph samples or rebuild process rows;
hidden process tables do not update. The client accepts bounded frames up to
8 MiB to accommodate the full process snapshot.
Graph readouts roll individual digits upward or downward
as the value changes, coalesce rapid pointer updates, and respect reduced motion.

Top-bar popups follow their buttons and stay within the desktop area beside the
sidebar, including after resizing or moving controls into overflow. The system
monitor popup is 520px wide and at most 640px tall, with compact graphs and
scrolling that stops at the content bounds in both the popup and sidebar.

The top-bar clock stays centred in the monitor area left after sidebar
reservations. Status controls that no longer fit move into the More menu.
Rolling numbers use the font's advance widths; only changed digits slide.
Run `python3 tests/top-bar-layout.py` for centring/overflow layout checks and
`tests/sidebar-notes.sh rolling-number` for digit spacing and motion checks.

The Overview processor section shows a responsive grid of per-logical-CPU graphs, sampled
from `/proc/stat` every two seconds. Each graph has its own CPU ID and fixed
0-100% scale, with shared 1m/5m history. New cores and counter resets start
with an unavailable reading; removed cores leave gaps in history.

Popup shortcuts are configured in `gnoblin.toml`: bare `Super` runs
`binguxctl search toggle` on release, `<Super>period` runs `binguxctl emoji open`,
and the capture shortcut runs `binguxctl capture toggle`. The search daemon no
longer opens UI from D-Bus signals. Held window/input switchers retain their
shortcut sessions. Run `python3 tests/popup-shortcuts-live.py` in an idle desktop
session to check release timing and chord suppression.

Use a direct Quickshell executable for `BINGUX_QUICKSHELL`; the IPC client does
not need the shell's QML/Mesa runtime launcher. `binguxctl --any-display` (or
`BINGUX_ANY_DISPLAY=1`) supports a selected shell whose display identifier differs
between runtimes. Search starts mostly opaque and completes its reveal in 90 ms.

Search uses Gnoblin's `capture-input = true` shortcut option to buffer typing
between Super release and keyboard focus. Its persistent compositor connection
acknowledges the mapped surface and focused text input before native events are
replayed, including editing keys. This is independent of the reveal animation.
