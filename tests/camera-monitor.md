# Camera graph monitor checks

Run the helper checks without opening a camera or changing a PipeWire service:

```sh
TMPDIR=/var/tmp PYTHONDONTWRITEBYTECODE=1 python3 tests/test_camera_status.py
TMPDIR=/var/tmp PYTHONDONTWRITEBYTECODE=1 python3 tests/camera-monitor.test.py
```

The fixture puts a private fake `pw-dump` on `PATH` and runs the camera helper.
It checks:

- One monitor while idle, initial camera/sharing ownership and object removals.
- Full object replacement, fragmented UTF-8 and adjacent JSON arrays.
- Numeric, boolean and null property values from `pw-dump`.
- Stable owner order when an update does not change capture details.
- Child exit/reconnect, malformed data and startup/partial-frame timeouts.
- Byte/object limits, closed output and termination cleanup.

## Monitor behavior

The real monitor command is `pw-dump -N --monitor`. PipeWire's `pw-dump(1)`
documents JSON arrays describing changes. PipeWire 1.4.11's `pw-dump.c` emits
complete changed Node/Client objects and `info:null` removals; unbound objects
can use `props:null`. Its stdout is line-buffered. The helper replaces objects
by ID and retains only privacy fields, including the existing portal-client
screen-sharing criteria. It never starts a new snapshot process for an event.

Healthy idle monitoring has no timeout or heartbeat. Startup and an incomplete
frame each have a three-second deadline. On failure, the helper publishes
unavailable with empty capture lists, reaps the child and retries after two
seconds using a fresh graph.

The helper limits each frame to 4 MiB. It retains at most 4 MiB of serialized
privacy fields and 4,096 Node/Client objects. Exceeding a limit follows the
unavailable/reconnect path.

SIGTERM/SIGINT cleanup terminates the monitor, waits up to one second, then
kills and reaps it if necessary. SIGKILL cannot run the cleanup handler.

## Read-only PipeWire check

A short read-only `pw-dump --monitor` run can confirm the installed PipeWire
format. Connecting a second read-only `pw-dump -N` client supplies a client
arrival/removal without opening media devices. Keep that check bounded and
terminate/reap both observers afterwards.

Synthetic camera/portal records check the helper's detection transitions.
They do not prove live camera use or shell presentation.

## Private native PipeWire check

Run the production helper against a temporary local PipeWire server:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tests/camera-monitor-live.py \
    --output /var/tmp/bingux-camera-native-proof
```

This needs `pipewire`, `pw-cli` and `pw-dump`. The check creates its own socket
and config in a private directory. It does not change host services or load
hardware modules.

Use `--source /path/to/camera-status.py` to compare another helper. The output
directory receives `native.json` and `native.stderr`.

The native check creates running driver nodes with `media.role=Camera`.
It checks the initial owner, removals, replacement owners, two server restarts,
a fresh graph after each reconnect, idle stability and monitor cleanup.
A numeric application name checks PipeWire's conversion of property strings
to JSON scalars. The old string-only validation reports unavailable here.

These nodes exercise the real graph and helper process. They do not capture
video or prove physical camera use, portal permission handling or shell UI.
