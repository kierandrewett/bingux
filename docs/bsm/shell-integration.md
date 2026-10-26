# Bingux shell integration with bsm

How the Bingux desktop shell behaves when bsm (the Bingux Session Manager,
see [architecture](architecture.md)) hosts its session, and what stays the same
when it does not. Source: `shell/bingux/Bsm*.qml`, `Bsm*.js`,
`SessionPower.qml`, `packaging/gnoblin/bingux.lua`.

## Modes

| Mode           | How it is chosen                           | Power, lock and logout                                                              | Readiness                      |
| -------------- | ------------------------------------------ | ----------------------------------------------------------------------------------- | ------------------------------ |
| Standalone     | `BSM_SESSION_ID` unset or empty            | `gnoblinctl session ACTION` behind the shell's local black fade; `bingux-lock` for lock | none reported                  |
| Integrated     | `BSM_SESSION_ID` set (by `bsm-helper`)     | `bsmctl ACTION --json`, outcome from `bsmctl monitor --json`; no local overlay      | JSON lines to `ready.sock`     |

Standalone Gnoblin and Bingux (greetd, `ext-session-lock-v1`, `bingux-lock`)
are not changed by bsm. In a standalone session the integration code is not
loaded: the two `Loader`s that start it (`shell.qml`, `Wallpaper.qml`) have
`active: BsmMode.integrated`, which is false.

### Environment

| Variable            | Set by      | Seen by                              | Use                                                                 |
| ------------------- | ----------- | ------------------------------------ | ------------------------------------------------------------------- |
| `BSM_SESSION_ID`    | bsm-helper  | Gnoblin, Bingux processes, `bsmctl`  | Switches integrated mode on. Not a secret.                          |
| `BSM_GENERATION`    | bsm-helper  | Gnoblin, Bingux processes            | Decimal launch id sent with every readiness line.                   |
| `XDG_RUNTIME_DIR`   | systemd     | Bingux processes                     | `$XDG_RUNTIME_DIR/bsm/ready.sock` is the readiness socket.          |

`BSM_HOST_SOCKET` (the brokered Wayland socket) goes to the bridge only; Bingux
never receives it.

A malformed `BSM_GENERATION` (not 1 to 20 decimal digits) disables reporting:
one warning `[bsm] BSM_GENERATION is missing or not a decimal number` is logged
and nothing is sent, because the bridge would ignore the lines anyway.

## Session and power actions

`BsmSession.qml` (singleton, loaded only by processes that use it) and its
pure half `BsmSession.js`.

Dispatch, all through `bsmctl` when integrated:

| Where                                   | Action                                            |
| --------------------------------------- | ------------------------------------------------- |
| Power options page (`SessionPower.qml`) | `suspend`, `logout`, `reboot`, `poweroff`, `switch-user` |
| Control Centre lock button              | `lock`                                            |
| Super+L (`bingux.lua`)                  | `lock`                                            |

`switch-user` is offered only in integrated mode.

### Request and outcome

1. The shell runs `bsmctl ACTION --json`. Only the six action names above can
   become a command; there is no shell interpolation.
2. bsm answers. Accepted: exit 0 and one JSON line `{"operation_id": N}`.
   Refused: exit non-zero and `{"error": CODE, "message": TEXT}`
   (`CODE` is a `bsm_proto::ErrorCode`; a bare string or
   `{"error": {"code", "message"}}` also parses). No usable answer
   (`bsmctl` missing, bsm not running) is reported as `unavailable`. Exit 0
   without an operation id is `no_operation`.
3. One long-lived `bsmctl monitor --json` (started by the desktop shell at
   start-up, restarted with a 1 s to 30 s backoff if it ends) delivers
   `MonitorEvent` lines. The shell follows the operation id through
   `transition` (`requested`, `authorising`, `preparing`, `animating`,
   `committed`, `failed`, `cancelled`, `completed`) and `operation_finished`.
   Events that arrive before the answer are kept by id and applied when the id
   is known.

The outcome rules, enforced and unit tested in `BsmSession.js`:

* An accepted request is `pending`. It becomes `completed` only when bsm
  reports `completed` (or `operation_finished` with `success: true`). The UI
  never decides success because a request was sent.
* `failed` shows the sentence for the reason code followed by the code, for
  example `Another application is blocking this. Close it and try again.
  (inhibited)`. `cancelled` is shown the same way, with its own reason.
* A final outcome is not rewritten by later or repeated events. Events for
  other operations are ignored.
* If the monitor ends while an operation is open, the operation fails with
  `monitor_lost` instead of waiting forever.
* One operation is open at a time; a second request while one is open is not
  started.
* There is no transition overlay in integrated mode: `SessionPower.qml` creates
  no `bingux-session-fade` window and animates nothing. bsm composes every
  fade. The UI shows the confirmation, a short status (`Preparing...`) and
  errors. The lock button and key do not wait for bsm; a refused `lock` is shown
  as a notification ("Could not lock the session ... (code)").

### Super+L

`bingux.lua` declares the shortcut `bingux-lock` on `<Super>l`. Gnoblin's Lua
has no `os` or `io` library, so the file cannot test the environment at load
time; the shortcut runs

```sh
if [ -n "$BSM_SESSION_ID" ]; then exec bsmctl lock; fi; exec bingux-lock
```

so the choice is made when the key is pressed, from the environment Gnoblin
runs in. In a standalone session this adds a Super+L binding that starts
`bingux-lock`, the same command as the Control Centre lock button. No Super+L
binding existed before. A refusal by `bsmctl lock` from the key is not shown in the shell (no
UI is involved).

## Readiness: `ready.sock`

The bridge (`bsm-host`) creates `$XDG_RUNTIME_DIR/bsm/ready.sock` (SOCK_STREAM)
before Gnoblin starts Bingux. Bingux is two Quickshell processes that report,
each as one `component`:

| Component   | Process (`bingux.lua` autostart)        | Required surfaces                                                    |
| ----------- | ---------------------------------------- | -------------------------------------------------------------------- |
| `wallpaper` | `bingux-wallpaper` (`WallpaperShell.qml`) | role `wallpaper`, one on every screen                                |
| `desktop`   | `bingux` (`shell.qml`)                   | role `bar`; role `dock` when the dock is enabled in the preferences  |

Popouts (`bingux-popouts`: search, window switcher, capture, emoji), tooltips,
the notification surface and every other layer surface are optional. They never
report and never block.

### Wire format

One JSON object per line, UTF-8, at most 4096 bytes, terminated by `\n`:

```json
{"generation":"4242","component":"desktop","event":"launched","required":["bar","dock"]}
{"generation":"4242","component":"desktop","event":"surface","output":"Virtual-1","role":"bar","mapped":false}
{"generation":"4242","component":"desktop","event":"surface","output":"Virtual-1","role":"bar","mapped":true}
{"generation":"4242","component":"desktop","event":"ready"}
```

| Field        | Meaning                                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| `generation` | `BSM_GENERATION`, a decimal string.                                                                                               |
| `component`  | `wallpaper` or `desktop`.                                                                                                         |
| `event`      | `launched`, `surface` or `ready`.                                                                                                 |
| `required`   | (`launched`) roles this component will map. A later `launched` with a different list replaces it (the dock preference changed).  |
| `output`     | (`surface`) the `wl_output` name Bingux sees, which is Gnoblin's connector name.                                                  |
| `role`       | (`surface`) `wallpaper`, `bar` or `dock`.                                                                                         |
| `mapped`     | (`surface`) `true` once a frame has been rendered for the window; `false` before that, and again when the window is unmapped or its screen goes away. |

Meaning of each event:

* `launched`: the component has created every window it requires (the
  wallpaper window on each screen; the bar window, and the dock window if the
  dock is enabled) and, for the desktop, has read its preferences, so the set of
  required surfaces is known. The window may still be hidden.
* `surface mapped:false`: a required window exists but has not rendered a frame.
  Sent with `launched` so the bridge learns the output names early.
* `surface mapped:true`: Qt finished swapping a frame for that window while it
  was mapped, with a non-zero size and its content ready (below).
* `ready`: every required surface of this component, on every screen it
  covers, is `mapped:true`. Nothing is sent before `launched`, and `ready` comes
  last in the batch that completes it.

Session-level readiness is the bridge's decision: `launched` and `ready` from
both components for the current generation, every required surface mapped, no
later `mapped:false`, and a cross-check against Gnoblin's layer-surface list
(`bingux-wallpaper`, `bingux-top-bar`, `bingux-dock`). bsm then validates a
presented frame on every required output before it reveals the desktop.

### What "rendered" means

`BsmSurfaceProbe.qml` watches one `PanelWindow`. It turns `rendered` on at the
first `QQuickWindow.frameSwapped` of the window's backing surface that happens
while the window is mapped (`backingWindowVisible`), has a size, and
`contentReady` holds. Setting `visible: true` is not evidence. It turns
`rendered` off when the window is unmapped. A later content change (a
wallpaper crossfade) does not turn it off.

`contentReady`:

* bar and dock: always true.
* wallpaper: the preferences are loaded and one of: no picture is configured;
  the picture is decoded and its fade-in has finished (`opacity >= 0.99`); the
  picture failed to load. A picture that never loads therefore cannot hold
  the reveal.

`frameSwapped` proves Qt committed a frame to Gnoblin. It does not prove that
bsm received it; bsm checks that itself (`SurfaceEvidence.presented_frames`).

`Startup.settled` is not read anywhere in the readiness code (a test greps for
it). `Startup` keeps its scheduling role for background helpers and is
unchanged.

### Reconnects, reloads and a missing socket

* Each connection starts from nothing. After a connect (first connection,
  bridge restart, shell restart or Quickshell reload) the process sends its
  full current state: `launched`, one `surface` per required window, `ready`
  when true. The lines are idempotent for the bridge, which keys state on
  `(generation, component, output, role)`. There is no acknowledgement in the
  protocol, so "already acknowledged" is per connection: within a connection a
  line is sent only when it changes.
* A surface that stops being mapped is withdrawn with `mapped:false`, and
  `ready` is sent again when everything is mapped again. A screen that is
  unplugged withdraws its surfaces.
* A new screen with no window yet keeps the earlier surfaces mapped, holds
  `ready` back, and resumes when its wallpaper has rendered.
* A missing or refused socket logs one warning
  (`[bsm] cannot reach .../ready.sock`), and the process keeps working. It
  retries with a delay of 0.5 s doubling to 15 s, without further logging until
  a connection has succeeded once.

## Tests

| Test                                   | Covers                                                                                                                                                         |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tests/bsm-session.test.mjs`           | `BsmSession.js`: commands, request answers, monitor events, outcome rules, monitor loss, error code coverage against `bsm-proto`.                              |
| `tests/bsm-readiness.test.mjs`         | `BsmReadiness.js`: required surfaces by configuration, launched and ready conditions, line ordering and diffs, withdrawal, reconnect, no use of `Startup`.    |
| `tests/bsm-fake-ready.py`              | Stand-in `ready.sock` server; validates every line and the ordering rules. Usable as a command.                                                                |
| `tests/bsm-shell-integration.py`       | The production QML in headless Sway against the fake socket and fake `bsmctl`/`gnoblinctl`: six scenarios, see the file header. Needs `sway`, `grim`, Pillow.  |
| `tests/bsm-lock-binding.py`            | The Super+L command with and without `BSM_SESSION_ID`.                                                                                                         |
| `tests/gnoblin-config-loader.py`       | `bingux.lua` through Gnoblin's real Lua loader; the `bingux-lock` shortcut loads exactly once.                                                                 |

The integration test replaces the shell's own bar, dock, theme and
preferences with small stand-ins: the stand-in bar and dock are real
layer-shell windows with the production namespaces, but the production
`shell.qml` is not run in it (it needs a full Bingux session). The production
`Loader` block in `shell.qml` that starts `BsmDesktop` is spliced into the
fixture unchanged.

## Not verified

* A real `bsmctl` and a real bridge: the contract above is what the shell
  implements and tests; the answers and events in the tests are written from
  `bsm-proto`, not produced by `bsm`.
* The production `shell.qml` end to end in a bsm-hosted Gnoblin (needs the
  hosted mode and the ISO).
* The Super+L key press itself in a running Gnoblin; the loader and the command
  are tested separately.
* Behaviour with several screens beyond the unit-level model (the headless
  test has one output). The bar and the dock are single windows on the default
  screen in the current shell, as `shell.qml` creates them.
