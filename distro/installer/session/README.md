# Live session loop

`bingux-live-session` is the live image's greetd `initial_session`. It runs one
Gnoblin compositor at a time and starts it again when it does not come up.
`bingux-live-switch` changes the mode (see `../README.md`, "Live session").

## Why the compositor is started again

Gnoblin gives up when its start is slow. Three fixed limits matter (Gnoblin
`src/control/gnoblin-control-connection.c:235`, `src/session/gnoblin-runtime-internal.h`,
`src/session/gnoblin-runtime-compositor.c:35`):

- The compositor waits 10 s for its runtime configuration, then exits with
  status 1 ("timed out waiting for Gnoblin runtime configuration"). The clock
  starts when the compositor starts, but the guardian spawns the process that
  sends the configuration only after `systemctl --user start gnoblin-session.target`
  returns.
- The Lua worker has 3 s to send its first configuration, and 10 s after that to
  hear from the compositor ("compositor did not complete the private runtime
  handshake"). The first start of the compositor takes 14 to 30 s on a busy
  machine ("Running Mutter" to "Using Wayland display name").
- A failed worker restarts on the built-in defaults. When Mutter does not
  acknowledge the suspension, the Lua runtime is lost for the whole session ("Lua
  runtime recovery failed"). No installer starts and the screen stays black.

The loop starts the compositor again (three tries in all) when it:

1. exits with an error in its first two minutes,
2. has no `$XDG_RUNTIME_DIR/gnoblin/compositor-v1.sock` one minute after it started, or
3. writes `defaults`, `last-good` or "Lua runtime recovery failed" to
   `$XDG_RUNTIME_DIR/gnoblin/config-fallback` in the first minute.

The second try starts in 2 to 3 s (measured in a booted ISO under load). The last try is
never stopped by the watch. A clean exit (status 0, for example a logout) and an
exit after the start window are not restarted. `bingux-live-switch` resets the count.
A real error in `installer.lua` also writes the `defaults` note, so it costs three
tries before the error panel stays.

Settings (whole seconds; for tests and support work): `BINGUX_LIVE_MAX_ATTEMPTS` (3),
`BINGUX_LIVE_START_WINDOW` (120), `BINGUX_LIVE_WATCH` (60), `BINGUX_LIVE_JOURNAL`
(1; 0 keeps the output on the terminal).

## Logs

The script re-runs itself under `systemd-cat`, so its own messages and the
output of the compositor, the supervisor and the programs Gnoblin starts go to the
journal:

    journalctl -b -t bingux-live-session

Gnoblin's own logs stay in `~/.local/state/gnoblin/session-last.log` and
`compositor-last.log`.

## Load at the start

- `packaging/systemd/bingux.target` has `DefaultDependencies=no`, so the start of
  `gnoblin-session.target` does not wait for the helper units. It waited for the
  wallpaper export (about 4 s of CPU): 6.5 s on four busy CPUs, 14 to 22 s in the E2E
  boots; now 0.4 s.
- `bingux-greeter-export.service` runs in idle time and the image's `pacman-init.service`
  (about 9 s of CPU) at nice 19.

Tests: `tests/live-session.test.py` (the loop, against fake compositors that fail, stay
up without a socket, or report a fallback) and `tests/session-start-load.test.py` (the
unit files).
