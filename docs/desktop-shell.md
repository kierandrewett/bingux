# Bingux desktop-shell contract

## Scope

The Bingux desktop shell is an optional profile feature. It supplies the top bar, dock, search surface, notification surface, and on-screen display for a Gnoblin session. The top bar includes clock, tray, metrics, privacy, input, network, audio, and power indicators. It is not a compositor and it does not modify GNOME Shell UI.

The tray uses `QsMenuAnchor` for left and right menu actions. Pixmap-only tray items, including the disconnected Tailscale item in the validation VM, use a visible local glyph fallback when the layer-shell icon provider cannot render the image URL.
The reference implementation uses Quickshell 0.2.1 for layer-shell surfaces. Quickshell supplies `zwlr_layer_shell_v1`, `zwlr_foreign_toplevel_manager_v1`, StatusNotifierItem menus, desktop-entry actions, and the desktop-notification service used by this shell. Bingux pins the exact Quickshell package through its flake lock.

Quickshell is pre-1.0. Bingux source must target the pinned 0.2.1 release line. A Quickshell update is a deliberate compatibility change, not an automatic API promise.

## Process model

```text
Gnoblin init.lua shortcut
          |
          v
binguxctl -> Quickshell IPC -> Bingux popup
                                 |
                          search-v1.sock
                                 |
                          bingux-searchd
```

Search, emoji, capture and the window switcher receive shortcuts over persistent compositor connections. Bare Super toggles search on release without another key or pointer action. Keep duplicate command bindings out of `init.lua`. `bingux-statusd` owns the Gnoblin OSD and desktop-state signal subscription. The QML process does not parse Gnoblin D-Bus output or start long-lived daemon monitors. It connects to local Unix sockets and renders typed records from the daemons. `SystemIndicators` also runs a bounded `nmcli` probe to identify the current NetworkManager connection type; the probe is killed after two seconds and is retried every five seconds.

The shell and daemon run as the profile user. The socket directory has mode `0700`. The socket has mode `0600`. The service does not listen on TCP or another network transport.

A missing or unsupported Gnoblin D-Bus service is an unavailable integration point. The daemon must retry with bounded backoff and report an unavailable state to the shell. It must not emulate the Super key or subscribe to the development-only Mutter key signal.

## Metrics socket protocol v1

`bingux-statusd` samples `/proc/stat`, `/proc/meminfo`, and `/proc/net/dev` once per second. It publishes
newline-delimited UTF-8 JSON records at `$XDG_RUNTIME_DIR/bingux/metrics-v1.sock`.

An optional `extra` object adds `cpuTemperatureCelsius`, `load1`, `load5`, `load15`,
`logicalCpus`, `swapUsedBytes`, `swapTotalBytes`, `diskReadBytesPerSecond` and
`diskWriteBytesPerSecond`. Missing sensors and first-sample disk rates are null.
CPU temperature uses recognised CPU hwmon drivers; generic board, GPU and NVMe
sensors are excluded. Disk rates count whole physical devices, omit partitions
and virtual devices, and ignore new devices or reset counters until a baseline
exists. See the kernel's [disk statistics](https://docs.kernel.org/admin-guide/iostats.html)
and [hwmon interfaces](https://docs.kernel.org/hwmon/sysfs-interface.html).

The service sends the most recent record when a client connects. It then sends one record after each sample. The
first record after service start has `null` CPU and network rates because there is no previous sample. The socket
directory has mode `0700`, and the socket has mode `0600`.

```json
{
    "protocolVersion": 1,
    "type": "metrics",
    "cpuPercent": 17.25,
    "memoryTotalBytes": 67331813376,
    "memoryUsedBytes": 34184560640,
    "networkReceiveBytesPerSecond": 306151.62,
    "networkTransmitBytesPerSecond": 62684.49
}
```

`cpuPercent`, `networkReceiveBytesPerSecond`, and `networkTransmitBytesPerSecond` can be `null`. The QML client
must retain a received sample for no more than three seconds. It must then show the metric as unavailable while it
reconnects with bounded backoff. A metrics failure must not stop the top bar, tray, or search interface.

## OSD socket protocol v2

Gnoblin emits a standard on-screen-display request only when its native OSD is
disabled for the session or for that OSD type. `bingux-statusd` is the sole
consumer of this D-Bus signal. It validates the request and publishes one
newline-delimited UTF-8 JSON record to
`$XDG_RUNTIME_DIR/bingux/osd-v2.sock`.

Bingux sets `shell.osd = false` through its Gnoblin Lua drop-in as well as the session's
disabled-features setting. `osd-bridge.js` supplies the same signal on older
running Gnoblin builds which can suppress their OSD but cannot emit the event.
It checks the native interface first and stays inactive when native support is
present. Monitor connectors come from Mutter DisplayConfig, and unloading the
script restores the original OSD handler.

OSD records are transient. The daemon does not cache them. A new socket client
receives only requests that arrive after its connection. The socket directory
has mode `0700`, and the socket has mode `0600`.

```json
{
    "protocolVersion": 2,
    "type": "osd",
    "monitorIndex": 0,
    "outputNames": ["DP-1"],
    "icon": "audio-volume-high-symbolic",
    "label": "Volume",
    "level": 0.75,
    "maxLevel": 1
}
```

`monitorIndex` identifies the request that a later OSD replaces. The shell must
not use it to select a screen. `outputNames` is a non-empty list of the physical
Mutter connector names in the logical monitor. It has at most 16 unique names.
Each name has at most 128 UTF-8 bytes, the total is at most 1024 bytes, and no
name contains a control character. Quickshell exposes the same connector as a
`ShellScreen.name`, so the shell renders the request on every matching screen
and must not fall back to a positional screen index.

`icon` is a themed icon name or an empty string. `icon` has at most 256 UTF-8
bytes. `label` has at most 2048 UTF-8 bytes. Neither string contains a control
character. `level` and `maxLevel` are finite values no less than `-1`. The shell
shows the level bar when `maxLevel` and `level` are non-negative. A zero maximum
from older compositor forwarding uses the standard maximum of one. The bar
uses `level / maxLevel`; the percentage uses `level * 100`, so amplified volume
can correctly show values above 100%. A tick marks normal volume on amplified
ranges. Negative levels or maxima represent status-only requests.

The shell keeps one request per monitor and replaces it when the next request
for that monitor arrives. It expires every request after 1.5 seconds, including
a request whose output is not currently connected. The OSD surface has no
keyboard focus and an empty pointer region. It cannot block an application
input event.

The card sits above the dock and uses the shell's surface, radius, and accent.
It is centred in the desktop area remaining beside the sidebar on that output,
including while the sidebar opens or closes. Percentages use the same sliding
counter as dock badges, rolling in the direction of the value change.
It separates the control name, optional device or mute detail, percentage, and
level bar. Opening uses the shared scale and fade motion; closing retains the
content while fading without scaling. Repeated requests replace the content and
restart the timeout. Reduced motion skips these animations.

## Notification ownership

Gnoblin disables its MessageTray UI in a Bingux session. The Bingux Quickshell
process owns the desktop-notification service. It supports plain-text body
content and notification actions. It does not advertise markup, hyperlinks,
images, inline replies, action icons, or persistence for closed notifications.

The Control Centre uses the shared `ShellPopup` primitive, hosted in the same
layer window as notifications. Opening it slides the existing notification
cards below its controls; closing it returns them to the top-right position.
Expiry pauses while the Control Centre is open. Cards retain their delegates,
groups, gestures and actions, and overflow scrolls within the reserved area.
The control layout groups connectivity and display shortcuts, exposes sound
and session controls, and selects a playing MPRIS player (or the first available
player) for the shared `MediaControls` widget. The dock's `DockMediaControls`
and the compact `ControlCentreMedia` are presentation wrappers around that
widget, including artwork caching/transitions, track-title scrolling, transport,
seek previews and elapsed/remaining time. The seek binding waits for the player
range to initialise, so a newly opened widget shows the current position.

Control buttons fade fixed-colour layers in opacity, preventing a dark flash
when hover leaves. Keyboard focus has its own outline. Detail pages restore
focus to their opener on Back or Escape; Escape on the overview closes the panel.
Bluetooth has a separate keyboard-accessible power switch, saved network
connections refresh while open and sort connected entries first, and the sound
slider supports keyboard steps and wheel input. Clear all retains notification
cards until their normal slide-out animation completes.

All open notifications remain in a newest-first stack. The stack scrolls when
it exceeds the screen height. Arrival does not hide or expire an older card.
Normal toasts hide after 2.5 seconds (or a shorter requested timeout). Hiding or
swiping away a desktop toast retains the notification in the control centre and
its app's dock badge and menu preview. Clear controls in the centre or dock menu,
Clear all, and application withdrawals remove retained notifications. A timeout
of `0` keeps the desktop toast visible until hidden or closed.

When an application replaces a notification ID, the shell keeps the card in its
current stack position and calculates a new expiry from the
replacement timeout.
Open notifications are retained and replayed across Quickshell configuration
reloads, including their actions and received timestamps. Replayed notifications
receive a fresh timeout only if their toast was visible; archived notifications
remain hidden until the control centre or app dock menu is opened.
Cards show an icon column on the left and the app name, title, and body on the
right. The received time appears at the top-right, replaced by a dismiss button
while the card is hovered. A circular indicator fills as the timeout elapses
and pauses on hover. Notifications without a timeout have no progress ring.
The top-bar New notification button sends a preview through the same D-Bus
notification service as applications.
Cards resolve the application name and missing icon from its desktop entry, with
sender metadata as a fallback. Hovering anywhere on a card pauses its remaining
expiry time, including over actions and the dismiss button. Expiry resumes when
the pointer leaves. Updates to a hovered card retain this pause.

Cards enter from the right with a 420 ms cubic ease-out and a subtle fade to
full opacity. Existing cards shift in 320 ms to make space before the entrance
finishes. Expired and closed
cards slide to the right at full opacity before their visual card is removed. Dragging right by 48 px
(or 15 percent of a narrow card) dismisses it. Shorter drags ease back into place. Grab a moving card and drag it left to
cancel dismissal and return it to its original position.
Set `BINGUX_REDUCED_MOTION=1` in the shell environment to make these transitions
instant. Notification cards do not request keyboard focus. Their pointer mask contains
only the visible card stack.

## Desktop UI rules

- The top bar is a top-layer surface with a positive exclusive zone.
- The dock is a top-layer surface anchored to the bottom edge. It must not reserve work area unless a profile explicitly selects that behaviour.
- Search is an overlay surface. It asks for on-demand keyboard focus only while visible. It returns focus when it closes.
- Notifications and on-screen displays are overlay surfaces with no keyboard focus.
- All icon controls have at least a 24 by 24 pixel pointer target.
- Search supports typing, Up and Down, Enter, and Escape. Escape closes the surface and clears transient selection.
- The tray uses StatusNotifierItem and DBusMenu support. Right-click menus must come from the item when it exposes one.
- A profile with `bingux.networking.tailscale.enable` starts the official
  `tailscale systray` client as the profile user. It publishes a
  StatusNotifierItem, so the tray displays its state and delegates left-click
  and right-click menus without a Bingux-specific Tailscale implementation.
- Dock actions use `.desktop` entry actions. Gnoblin does not currently export a dynamic application-menu protocol for foreign toplevels. Do not claim that arbitrary in-window menus are available until a separate, versioned Gnoblin interface exists.

## Search socket protocol v1

The transport is newline-delimited UTF-8 JSON. One record is one line. Each record is at most 64 KiB. The receiver must reject malformed JSON, records over the limit, unknown required fields, and protocol versions other than `1`.

Every request contains these fields:

```json
{
    "protocolVersion": 1,
    "type": "query",
    "requestId": "opaque-client-request-id"
}
```

`requestId` is an opaque ASCII string with 1 to 64 characters from `[A-Za-z0-9_-]`. It avoids JavaScript integer precision loss. A response that refers to a request must repeat the same `requestId`.

### Daemon-to-shell events

```json
{
    "protocolVersion": 1,
    "type": "show-search",
    "monotonicUsec": "123456789"
}
```

`show-search` and `gnoblin-super-release` integration records are legacy wire formats. The daemon no longer emits them; the shell accepts and ignores old show-search records during upgrades. Popup opening is owned by `binguxctl` commands in `init.lua`.

```json
{
    "protocolVersion": 1,
    "type": "integration-state",
    "name": "gnoblin-super-release",
    "state": "ready"
}
```

`state` is `ready` or `unavailable`. The shell shows no error popup for an unavailable event source. It keeps normal search access available through the top-bar action.

### Shell-to-daemon requests

A query request is:

```json
{
    "protocolVersion": 1,
    "type": "query",
    "requestId": "q-01",
    "query": "firefox",
    "limit": 20
}
```

`query` is UTF-8 text with at most 512 bytes. `limit` is an integer from 1 to 50. The daemon returns zero or more partial result records followed by one completed record:

```json
{
    "protocolVersion": 1,
    "type": "results",
    "requestId": "q-01",
    "complete": false,
    "elapsedUsec": 820,
    "results": [
        {
            "resultId": "app:firefox.desktop",
            "providerId": "apps",
            "kind": "application",
            "title": "Firefox",
            "subtitle": "Web browser",
            "icon": "firefox",
            "score": 0.98
        }
    ]
}
```

`kind` is one of `application`, `file`, `folder`, `database`, `calculation`, `weather`, `chat`, or `action`. `title`, `subtitle`, and `icon` are plain text. The UI must render them as text, not as markup. `score` is a finite number from 0 through 1.

The completed record has `complete: true`. It can contain an empty `results` array. The daemon must discard results for a request that the client has superseded with a later request.

The shell sends `cancel` for an active query or activation when it replaces or closes the search
surface:

```json
{
    "protocolVersion": 1,
    "type": "cancel",
    "requestId": "q-01"
}
```

`cancel` is idempotent and produces no response. It applies only to the current request for the
same socket client. The daemon removes the chat route and releases its admission permit
immediately. A queued or running worker still drains the bounded work item, but it cannot deliver
a response after the route is cancelled. An HTTPS chat request that has already started cannot be
safely retracted. The daemon keeps at most four bounded worker executions and a bounded queue until
each worker returns. Provider protocol v1 has no cancellation record. If the daemon has already sent
an external activation to its provider, it cannot stop that provider action. It still discards the
later completion or failure event.

Activation is separate from a query:

```json
{
    "protocolVersion": 1,
    "type": "activate",
    "requestId": "a-01",
    "resultId": "app:firefox.desktop"
}
```

The shell never receives an executable command from a result. The daemon resolves the short-lived `resultId` and performs or forwards the action. It returns `activated` or an `error` record. A stale or unknown result ID fails without action.

Calculation activations copy the calculation result text to the configured `commands.clipboard` argument vector. The daemon writes UTF-8 text to the helper's standard input, closes the input, waits for a successful exit, and reports `provider-failed` on a write, exit, or two-second timeout failure. The command is profile-owned configuration and is never constructed from shell input.

An error record is:

```json
{
    "protocolVersion": 1,
    "type": "error",
    "requestId": "q-01",
    "code": "invalid-request",
    "message": "request is invalid"
}
```

Valid codes are `invalid-request`, `unsupported-protocol`, `unavailable`, `provider-failed`, and `unknown-result`. Error messages must not include a command line, secret, or stack trace.

For a rejected record with a valid `requestId`, the daemon returns that identifier. If it cannot
parse a valid identifier, it returns `requestId: "protocol-error"`. Clients must treat that value
as a connection-level protocol error, not as a response to an application request. The daemon
continues after a rejected complete record. It sends this error and closes the connection after an
invalid or oversized transport record.

## Search-provider contract v1

A provider is a long-lived, profile-trusted process. Its manifest is JSON and follows the same
version policy. Bingux discovers manifests only from configured profile paths. It does not scan
`PATH`, a network location, or arbitrary writable directories.

```json
{
    "kind": "bingux.search-provider",
    "protocolVersion": 1,
    "id": "apps",
    "displayName": "Applications",
    "command": ["/usr/libexec/bingux/bingux-provider-apps"],
    "startup": "eager",
    "priority": 100,
    "timeoutMs": 20
}
```

`id` matches `[a-z0-9]+(?:-[a-z0-9]+)*` and contains at most 64 bytes. `command` is a non-empty
argument array whose program path is absolute. Arguments must not be empty or contain NUL. The host
does not pass a shell string. `startup` is `eager` or `lazy`. `priority` is an integer from 0 to 1000.
`timeoutMs` is an integer from 1 to 10,000. Bingux rejects manifest files larger than 64 KiB.

The provider protocol uses newline-delimited UTF-8 JSON on standard input and standard output.
Each record is at most 64 KiB. The host sends this record after it starts a provider:

```json
{
    "protocolVersion": 1,
    "type": "hello",
    "hostId": "bingux-searchd"
}
```

The provider must return this record before the manifest timeout:

```json
{
    "protocolVersion": 1,
    "type": "hello",
    "accepted": true
}
```

The host gives each provider query a provider-local `queryId` that matches the same ASCII rule as
`requestId`. A provider must return one or more result records, then exactly one completion record:

```json
{
    "protocolVersion": 1,
    "type": "query",
    "queryId": "provider-query-01",
    "query": "firefox",
    "limit": 20
}
```

```json
{
    "protocolVersion": 1,
    "type": "results",
    "queryId": "provider-query-01",
    "complete": false,
    "results": [
        {
            "resultId": "firefox.desktop",
            "kind": "application",
            "title": "Firefox",
            "subtitle": "Web browser",
            "icon": "firefox",
            "score": 0.98
        }
    ]
}
```

Provider-local `resultId` values match `[A-Za-z0-9._:-]{1,128}`. The combined UTF-8 byte length of
`title`, `subtitle`, and `icon` must not exceed 24 KiB, and these fields must not contain control
characters. The host maps them to short-lived opaque socket result identifiers. Built-in application
results also include an optional `desktopId` field in the shell response. It identifies the installed
`.desktop` entry for dock pinning; `resultId` remains the opaque activation token. Shell clients must
accept this optional field. It is not part of the external provider response schema. The host may split a
provider result batch into multiple socket records to keep each record within 64 KiB. It never sends
a provider command or executable text to QML.

Activation is a separate provider record:

```json
{
    "protocolVersion": 1,
    "type": "activate",
    "activationId": "provider-activation-01",
    "resultId": "firefox.desktop"
}
```

The provider responds with `{"protocolVersion":1,"type":"activated","activationId":"provider-activation-01"}`
or with one `error` record containing the matching `queryId` or `activationId`. Valid provider
error codes are `invalid-request`, `unavailable`, and `provider-failed`. Provider protocol v1 has
no cancellation record. The host drops an activation that it has not sent to the provider. It
cannot retract an activation record that it has already sent.

The host starts eager providers before the first query, runs provider queries concurrently, and
enforces each manifest timeout. A malformed, oversized, out-of-order, or version-mismatched
provider record stops that provider and reports `provider-failed` for the affected request. A
provider must not make a network request on the query path. Weather reads a local cache. AI chat is
explicit user work after activation and has no instant-result promise. SQLite providers must use
configured, read-only queries with `?1` for the search text and `LIMIT ?2` for the result bound.
SQLite reads accept only regular files and stop after the built-in query deadline.

Provider code and manifests are trusted profile software. They run with the profile user
permissions. A manifest must not contain a secret. A provider that needs a credential receives a
profile-declared runtime secret path or environment variable from the user's secret manager.

## Performance rule

Focused protocol and unit checks cover the search daemon and status/OSD paths; the
standalone packaging checks also assert that the notification and OSD surfaces are configured.
The QML client repeats record and field-boundary validation in `SearchSocket.qml`, but these checks
do not execute QML functions. These checks do not establish runtime or VM behaviour.

Run the local socket regression benchmark from the repository root:

```sh
cargo test --release --manifest-path packages/bingux-searchd/Cargo.toml \
  measures_warm_socket_query_latency -- --ignored --nocapture
# [benchmark] warm socket query: p95=<nanoseconds> max=<nanoseconds> samples=200
```

The benchmark is ignored by default and starts no application or file index worker, so filesystem scanning cannot affect
a sample. It warms only a calculation completion path and measures write, daemon dispatch, result encoding, socket
delivery, and JSON decoding. It does not prove the latency of a populated application index, filesystem search, SQLite,
external providers, or compositor paint. The Proxmox desktop exercise must measure those paths on the installed system.

File discovery uses `rg --files` or an equivalent background index refresh. It does not walk the file system synchronously for each keypress.

## Version policy

Version 1 records contain `protocolVersion: 1` and use closed field sets. An added or removed field,
a changed field meaning, or a required-field change requires a new protocol version and a separate
socket path. The v1 host rejects unknown fields and other versions rather than guessing.

## Sources

- Quickshell installation and package guidance: <https://quickshell.org/docs/v0.2.1/guide/install-setup/>
- Quickshell distribution and version guidance: <https://quickshell.org/docs/v0.2.1/guide/distribution/>
- Quickshell layer-shell surfaces: <https://quickshell.org/docs/v0.2.1/types/Quickshell/PanelWindow/>
- Quickshell foreign toplevels: <https://quickshell.org/docs/v0.2.1/types/Quickshell.Wayland/ToplevelManager/>
- Quickshell StatusNotifierItem support: <https://quickshell.org/docs/v0.2.1/types/Quickshell.Services.SystemTray/SystemTrayItem/>
- Quickshell notification server: <https://quickshell.org/docs/v0.2.1/types/Quickshell.Services.Notifications/NotificationServer/>
- Quickshell notification lifetime and action API: <https://quickshell.org/docs/v0.2.1/types/Quickshell.Services.Notifications/Notification/>
- Quickshell layer-shell pointer masks: <https://quickshell.org/docs/v0.2.1/types/Quickshell/QsWindow/>
- Gnoblin interface source: `~/dev/gnoblin/src/gnome-shell-overlay/js/ui/components/gnoblinControl.js`

Notifications from the same app form a collapsed stack with a count. Click the card to expand the group, then use Show less to collapse it. Groups follow their newest notification; individual notifications retain their expiry and dismissal behaviour. Overflow scrolls, with a transparent gradient over the bottom 64 pixels while more content remains below.
Collapsed app groups use progressively lower card opacity, down to 45%. Expanded cards and their background colours are fully opaque.
Expansion moves cards out from behind the group head without fading; collapse returns them behind it. Expanded groups retain their count and share a subtle enclosing background. The collapsed layers are the same notification delegates: their positions, insets and heights animate continuously between the two layouts, including when a transition is reversed.
Expanded groups use equal 6 px edge padding and 8 px gaps between cards. Backing cards keep their real geometry while their icons and contents fade away when collapsed and return on expansion.
The expanded backdrop belongs to the app group and survives removal of its head card, remaining while multiple items exist. With one item left, the backdrop fades away and the extra group padding is removed. Notification cards have soft downward shadows.
The head card uses a fully opaque surface colour. Group backgrounds start invisible and are shown only for expanded groups, avoiding an initial opacity flash.
When a new card covers the previous head of a collapsed group, the previous contents stay visible until the new card has settled, then fade out over 180 ms.
Dragging a collapsed group head progressively reveals the next card and its contents in proportion to drag distance; returning the head restores the original depth opacity.
During that drag, the next card also widens and rises towards the front-card position, then tucks back if the drag is cancelled. Promotion preserves its revealed geometry rather than resetting its inset.
A revealed successor keeps the stack-header height and shows the current count when multiple notifications will remain. The successor card count digits roll and crossfade towards the remaining count during dragging, while the outgoing card keeps its original count, and reverse on cancellation; the notification is only removed when dismissal completes. Only the final standalone successor sheds the group-control space.
Dragging a group head advances every backing card towards the preceding depth slot, including width, position and depth opacity. Deeper cards retain hidden contents; cancelling restores the whole stack.

Control Centre interaction and shared-window motion checks: `bash tests/control-centre.sh`; set `BINGUX_REDUCED_MOTION=1` to verify the instant-transition path.

Network, Bluetooth, Sound and Display open in-panel detail pages with a subtle 12 px, 160 ms eased crossfade and a back control. The page deck does not clip against its padding; the control centre uses the same shared opening animation as the calendar. Network connects saved profiles through NetworkManager; Bluetooth exposes paired-device connections; Sound selects PipeWire outputs. Full system-settings shortcuts handle pairing, new network credentials and display configuration.

The Control Centre closes through Escape, an outside click or its menu-bar toggle;
there is no header close button. Network separates saved connections from nearby
Wi-Fi, excludes container bridges and loopback interfaces, and supports explicit
connect/disconnect actions. New Wi-Fi setup opens Network settings. Bluetooth
shows paired and nearby devices, connection progress, and user-started discovery;
leaving the page stops discovery started by this panel. Pairing opens Bluetooth
settings. Sound includes separate output and microphone device selection, mute
and volume controls, with disabled states when hardware is unavailable.

Control Centre layout and interaction rules are defined in
[control-centre-design.md](control-centre-design.md). Detail pages share grouped
rows, selection checkmarks and one settings footer. Sound uses Output/Input tabs;
Network keeps saved profiles and VPNs behind Other connections; Bluetooth reveals
nearby devices only after Add a device. The overview omits the title and close button, and
Display opens system settings directly.

## Keyboard layout switcher

The right-hand top-bar layout label opens the shared keyboard-layout popup.
Super+Space advances through configured sources immediately; Super+Shift+Space
cycles backwards. The popup stays visible while Super is held and closes on release, without
taking keyboard focus from the application. A manual click opens the normal
interactive menu. The top bar uses GNOME’s disambiguated short labels such as `en₁` and `en₂`. Rapid key presses queue the latest requested
source instead of dropping input during a change.

Bingux releases GNOME's `switch-input-source` and
`switch-input-source-backward` bindings and registers its own through the shared
compositor shortcut transport. Gnoblin's `[shell] input-source-switcher = false`
option disables the native keyboard popup without disabling input sources.

Top-bar controls can be rearranged with Ctrl + drag. The control follows the pointer,
while its neighbours slide aside with the shared dock displacement animation. On release,
it eases into its slot before the new order is saved.
Release outside the bar or overflow menu to cancel. Normal clicks keep their usual action.
The overflow menu supports the same gesture within its list. Search and the centred clock
remain fixed. The order is saved in `$XDG_CONFIG_HOME/bingux/top-bar.ini` (by default
`~/.config/bingux/top-bar.ini`) and restored after reloads and restarts. Hidden controls
keep their saved position.
