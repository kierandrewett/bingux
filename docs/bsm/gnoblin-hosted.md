# Gnoblin hosted mode (bsm side summary)

`gnoblin --hosted` is the per-user compositor that bsm hosts. It lives on the
Gnoblin branch `feat/bsm-hosted` (worktree `/home/kieran/dev/gnoblin-bsm-wt`,
commit `212dddfd`, pushed to origin). The full investigation, decisions and
unverified list are in that branch's `design/bsm-hosted.md`; the user-facing page
is `docs/hosted.md`. This page is what bsm, `bsm-helper` and `bsm-host` rely on.

## Launch

```sh
gnoblin --hosted --wayland-display bsm-<session-id>
```

- Name: 1 to 128 characters of `A-Z a-z 0-9 _ . -`, not `.` or `..`. The socket is
  `$XDG_RUNTIME_DIR/<name>`.
- Run as the session user inside the session scope.
- Required environment: `HOME`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`
  (a running user bus), `XDG_SESSION_TYPE=wayland`, `PATH`.
- Must be unset: `WAYLAND_DISPLAY`, `DISPLAY` (Gnoblin refuses to start).
- Optional: `XDG_SESSION_ID` (a normal Gnoblin session requires it).
  `BSM_SESSION_ID` and `BSM_GENERATION` are inherited by autostarted programs.
  Do not pass `BSM_HOST_SOCKET`.
- It starts headless Mutter with no monitors, no viewer and no seat or device
  ownership. Without a user `systemd --user` on the bus it waits 10 seconds
  before it spawns Mutter.
- `SIGTERM` stops it (exit status 143). A compositor crash gives a failure
  status. `gnoblin` also exits 0 on a logout it handles itself; in hosted mode a
  logout is reported instead (below).

## Interfaces for the bridge

| Need                    | Interface                                                                                              |
| ----------------------- | ------------------------------------------------------------------------------------------------------ |
| Compositor ready        | Session bus name `org.gnoblin.Hosted1`, `/org/gnoblin/Hosted1`, property `Ready` and `PropertiesChanged`. |
| Session ended           | `org.gnoblin.Hosted1` leaves the bus.                                                                  |
| Monitors                | `org.gnome.Mutter.ScreenCast` `RecordVirtual`. Size comes from the consumer. Connectors are `Meta-N`.  |
| Position and scale      | `org.gnome.Mutter.DisplayConfig` `GetCurrentState`, `ApplyMonitorsConfig`.                             |
| Input                   | `org.gnome.Mutter.RemoteDesktop` `ConnectToEIS`.                                                       |
| Required surfaces       | Control socket `$XDG_RUNTIME_DIR/gnoblin/compositor-v1.sock`: `layers.list` plus layer events.         |
| Lock request            | Control-socket event `gnoblin.session.lock-requested`.                                                 |
| Logout and power        | Signal `org.gnoblin.Hosted1.ActionRequested(s)`.                                                       |

### What `Ready` means

`Ready` is true once Mutter finished the private runtime handshake with the
Gnoblin worker: the guardian `READY` status. The compositor serves session-local
Wayland clients and the session bus services above are up. It is sent before the
user's Lua configuration loads, so it says nothing about autostart or about
Bingux. It stays true across Lua supervisor restarts. bsm's `GnoblinReady` stage
should use it; `SurfacesMapped` and `DesktopReady` must not.

### Surfaces

Connect to the control socket, request API `{"major":1,"minor":81}` (any minor
from 37 works for this call) and send:

```json
{"op":"api","id":"l","api_version":{"major":1,"minor":81},"method":"layers.list","arguments":{}}
{"op":"events","api_version":{"major":1,"minor":81},"events":["gnoblin.layer.created","gnoblin.layer.changed","gnoblin.layer.removed","gnoblin.session.lock-requested"]}
```

Use `layers.list`, not `layer.list`: the singular call is a legacy shape with only
`id`, `namespace` and `title`. A `layers.list` record has `id`, `namespace`,
`monitor_id` (the connector, for example `Meta-0`), `layer`, `anchor`,
`exclusive_zone`, `geometry` and `mapped`. Bingux's required namespaces are
`bingux-wallpaper`, `bingux-top-bar` and `bingux-dock`.

### Lock, logout and power

Gnoblin starts no locker. Every lock source ends in the
`gnoblin.session.lock-requested` event for subscribed control-socket clients.
With no subscriber the request fails ("no session-lock client is subscribed"),
so the bridge must subscribe before Bingux starts. To keep programs in the
session from starting a competing `ext-session-lock-v1` locker, the hosted
Gnoblin configuration sets `gnoblin.configure { protocols = { ext_session_lock =
false } }`; that setting belongs to the shell integration, not to Gnoblin.

In hosted mode `gnoblinctl session logout|reboot|poweroff|suspend|hibernate` and
the Lua `gnoblin.session.*` operations neither end the session nor call logind.
They emit `ActionRequested("logout" | "reboot" | "poweroff" | "suspend" |
"hibernate")` from the name owner `org.gnoblin.Hosted1`, and the session keeps
running. bsm decides and, for a logout, stops `gnoblin` itself.

### Access

No Mutter or Gnoblin access list restricts ScreenCast, RemoteDesktop or
DisplayConfig for a client of the same user on the session bus. No allow-list
change was made.

## What ran

`tests/hosted-session.py` in the Gnoblin worktree, run as the current user inside
`scripts/run-staged.sh` with a private bus, a private PipeWire and the system bus
pointed at a missing socket. 55 checks passed: readiness and its change
notification, no viewer, two virtual monitors with frames whose pixels match the
wallpaper, bar and dock layer surfaces and a `foot` window at logical scale 1.25,
DisplayConfig position and scale, an EIS socket, layer events and removal, lock
with and without a subscriber, and the five logout and power requests as
`ActionRequested`. Evidence (logs, JSON, PNG frames) is in
`distro/out/bsm-test/gnoblin-hosted/`.

## Not verified

- Input delivery through EIS (the socket is handed out; `bsm-host` tests the rest).
- A real `systemd --user` manager and `gnoblin-session.target`.
- DMA-BUF consumers and WirePlumber linking. The test consumer negotiated
  memory-mapped buffers and linked ports with `pw-link`.
- Changing the display layout after Bingux started: a bar once moved between
  monitors. Apply scale and position before Bingux launches.
- The devkit viewer path at runtime, and hosted sessions of several users at once.
