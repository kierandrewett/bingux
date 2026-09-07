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
