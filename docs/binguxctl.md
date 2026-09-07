# binguxctl

`binguxctl` controls the running Bingux shell. It uses Quickshell IPC and does not
start another shell or restart the compositor. `--help` lists commands; each
command also has its own help.

```sh
binguxctl capture                         # Open the capture selector
binguxctl capture open --mode screenshot --target region
binguxctl capture open --mode recording --target screen
binguxctl capture take                    # Press Capture with the current selection
binguxctl capture stop                    # Finish the current recording
binguxctl capture cancel
binguxctl capture status                  # JSON, including savedPath and elapsed time
binguxctl search query "firefox"
binguxctl controls open
binguxctl calendar toggle
binguxctl notifications open
binguxctl dnd on
binguxctl sidebar select notes
binguxctl sidebar open
binguxctl sidebar edge right
binguxctl keyboard next
binguxctl emoji open
binguxctl privacy status
binguxctl status
binguxctl reload
```

Opening capture is idempotent. `capture toggle` retains the keyboard shortcut's
behaviour: open/close the selector, cancel a countdown, or stop a recording.
`capture take` requires a ready selector. `--mode` and `--target` apply only to
`capture open`; omitted options retain the UI preferences. Capturing and setting
Do Not Disturb are asynchronous; their status commands report the resulting state.

Search, calendar, controls, metrics, keyboard and notifications accept `open`,
`close`, `toggle` and `status`. `sidebar` also supports `popout`, `dock`, `select`
and `edge`. `dnd` accepts `on`, `off`, `toggle` and `status`.

For new controls that do not yet have a named command:

```sh
binguxctl list                            # Discover public targets and signatures
binguxctl ipc TARGET METHOD ARGUMENT...
```

Arguments are passed directly, without shell evaluation. Read commands print
JSON. Failures print a diagnostic to stderr and return nonzero. Only an explicit
"not ready" response is retried during reload; an uncertain timeout is never
repeated because the command may already have executed.

The Nix desktop module installs the command and uses `capture toggle` for the
capture shortcut. It selects the configured Quickshell package and configuration
name. For a development instance:

```sh
binguxctl --quickshell /path/to/qs --path /path/to/shell status
binguxctl --config bingux status
```

The equivalent defaults are `BINGUX_QUICKSHELL`, `BINGUX_CONFIG_PATH` and
`BINGUX_CONFIG_NAME`. Explicit options take precedence. Reload affects the shell;
an active capture can be finalised by the outgoing capture worker.

Screenshots play GNOME's `screen-capture` sound event after a successful save.
The sound follows the current sound theme and event-sound setting. Cancelled or
failed captures, previews, and recordings do not play the shutter.

## Audio and media

```sh
binguxctl audio status
binguxctl audio volume 40
binguxctl audio up 5
binguxctl audio toggle
binguxctl audio mute --input
binguxctl media list
binguxctl media toggle
binguxctl media next --player org.mpris.MediaPlayer2.spotify
binguxctl media seek 30 --player org.mpris.MediaPlayer2.spotify
```

Audio values are percentages, limited to 0-100. `up` and `down` default to five
percentage points. Volume changes retain the current mute state; use `unmute`
explicitly. `--input` selects the default microphone instead of the output.
Unavailable devices return an error.

Media commands use the control centre's selected player, or an exact D-Bus ID
from `media list` when `--player` is supplied. Available actions are `play`,
`pause`, `toggle`, `next`, `previous` and `seek`. Seek takes an absolute position
in seconds. Unsupported actions and missing player IDs return errors.

## Notifications, dock and windows

```sh
binguxctl notifications list
binguxctl notifications invoke SESSION:ID ACTION_ID
binguxctl notifications dismiss SESSION:ID
binguxctl notifications clear
binguxctl dock list
binguxctl dock pin APP_ID
binguxctl dock move APP_ID 2
binguxctl dock launch APP_ID
binguxctl windows list
binguxctl windows minimize WINDOW_ID
binguxctl windows activate WINDOW_ID
binguxctl windows close WINDOW_ID
```

Use the exact IDs returned by the list commands. Notification lists include
available action IDs. Archived notifications can be dismissed but might no
longer have live actions. `clear` dismisses all notifications.

Dock actions operate on listed app groups and use the same pin, reorder and
launch operations as the dock. `launch` requests a new window; `activate`
restores an existing window or launches a pinned app. `unpin` removes a pin.
Positions are zero-based; moves across pinned/running sections require changing
the pin state first.

Window IDs remain stable across focus changes and expire when their window
closes or the shell reloads. Refresh `windows list` after a reload. `activate`
also restores a minimised window; `restore` only removes minimisation.

## Capture settings

```sh
binguxctl capture options
binguxctl capture configure --no-copy --cursor --delay 3
binguxctl capture open --mode recording --fps 30 --max-height 1080 --audio system
binguxctl capture open --region 100 100 800 600 --output ~/Pictures/Screenshots
```

`configure` saves options without opening the selector. `open` can apply options
and open it in one command. Supported settings include mode, target, frame rate,
height limit, delay, quality, audio source, image format, encoder, capture backend,
output directory, cursor visibility, clipboard copying and region. Use
`--no-cursor` or `--no-copy` to turn those options off. `--help` lists valid values.

Regions use logical coordinates relative to the selected capture screen. A
region must fit the screen. Each request is validated in full before any
preference changes, and options cannot change during an active capture.

## Devices and desktop settings

```sh
binguxctl controls page network
binguxctl controls page audio --input
binguxctl audio devices
binguxctl audio devices --input
binguxctl audio select DEVICE_ID
binguxctl network list
binguxctl network connect CONNECTION_UUID
binguxctl network disconnect CONNECTION_UUID
binguxctl bluetooth list
binguxctl bluetooth scan on
binguxctl bluetooth connect AA:BB:CC:DD:EE:FF
binguxctl power status
binguxctl power set balanced
binguxctl night-light on
binguxctl awake on
```

Control-centre pages are `network`, `bluetooth`, `audio`, `display`, `vpn`,
`power` and `customise`. Opening a page is idempotent. `--input` opens the
microphone tab on the audio page. Audio device IDs are PipeWire node names
from `audio devices`; pass `--input` to select a microphone.

`network list` refreshes saved connections and nearby Wi-Fi networks, then
waits up to 15 seconds for the results. `network status` returns cached state
immediately. `network refresh` starts a refresh without waiting. Connect and
disconnect first refresh the list, then operate on an exact saved connection
UUID. Create new connections or enter passwords through Network settings.
A failed connection remains visible in `network status` until the next
explicit refresh or connection change.

Bluetooth supports `on`, `off`, `toggle`, `status`, `list`, `scan on`,
`scan off`, `connect ADDRESS` and `disconnect ADDRESS`. Connect requires an
already paired, unblocked device. Command-started discovery stops after 30
seconds. `scan off` does not stop discovery owned by another client or needed
by the open Bluetooth page.

Power profiles come from `power status`; unavailable profiles are rejected.
Night Light and Keep Awake both support `on`, `off`, `toggle` and `status`.
Keep Awake uses the same session inhibitor as the control centre and ends
when the shell exits or reloads.

Device and settings changes can return `{"ok":true,"pending":true}`. This
means the request was accepted, not that the backend completed it. Inspect
`status`, `audio devices`, or `bluetooth list` to confirm the resulting state.
Network and desktop-service status include `busy` and `error`. Bluetooth
reports each device's connection state. Commands never retry an operation
when the connection closes or its result is uncertain.
