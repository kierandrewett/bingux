# bsm compositor core

`packages/bsm-compositor` is the compositor half of `bsm`: the Wayland server, the
brokered connections, the scene, input routing and every full-screen transition. It
decides no policy. The manager drives it with `SceneCommand`s and reads `SceneEvent`s
(`packages/bsm-proto/src/scene.rs`); the architecture and the invariants it serves are
in [architecture.md](architecture.md).

```rust
bsm_compositor::run(config, cmd_rx /* calloop channel */, evt_tx /* std mpsc */) -> anyhow::Result<()>
```

`run` blocks the calling thread (winit needs the main thread) until the command channel
is closed or the backend ends. The headless test driver is `bsm_compositor::Harness`.

Status words used below: **tested** = covered by a test that was run and passed (see
[Tests](#tests)); **unverified** = written and compiled but not run against real
hardware or a real peer; **blocked** = needs something that does not exist yet.

## Contents of the crate

| Module        | What it is                                                                                                  |
| ------------- | ----------------------------------------------------------------------------------------------------------- |
| `engine`      | Pure scene and transition engine. No Wayland, no GPU, no clock. Unit tested.                                |
| `roles`       | Roles and the global visibility table.                                                                       |
| `peer`        | `SO_PEERCRED`/`SO_PEERPIDFD`, cgroup scope and exe checks. Pure or `/proc` only.                            |
| `protocol`    | Server code for `bsm_session_host_v1`, generated from `packages/bsm/protocol/bsm-session-host-v1.xml`.       |
| `state`       | Wayland globals, outputs, the loop. Hand-written `can_view` for every global.                                |
| `broker`      | Greeter and session sockets, held connections, promotion, rejection, disconnect handling.                    |
| `host`        | The hosted session registry, the protocol implementation and the per-output frame evidence.                  |
| `input`       | Input owner, focus, pointer and keyboard, libinput/winit event translation.                                  |
| `render`      | Plan to render elements, generic over the renderer. Cursor.                                                  |
| `gfx`, `gfx_winit`, `gfx_drm` | Backends. Headless (Pixman), Winit (GLES), Drm (libseat, udev, KMS, GBM, GLES, libinput). |
| `harness`     | Stepped headless compositor on a thread, frame capture, input injection.                                     |

Cargo features: `backend-winit` and `backend-drm`, both on by default. The headless
backend is always built. `--no-default-features` builds without libseat, libinput, udev
and EGL, which is enough for the engine and the harness.

## Configuration and watchdog knobs

`Config` fields added by the security review and the boot handover: `request_pass_limit`
(requests per client per loop pass, default 50 000), `heartbeat: Option<Heartbeat>`,
`hold_first_scene: Option<Duration>` and `first_scene: FirstScene`.

* `Heartbeat` is beaten once per pass of the compositor loop (at least every 250 ms while idle).
  `bsm-notify` should send `WATCHDOG=1` only while `Heartbeat::age()` is small, so a wedged
  compositor loop is restarted by systemd (review F3). Wiring it in `packages/bsm` is the
  manager side.
* `FirstScene` is set once; `wait(timeout)` returns `Presented`, `TimedOut`, `NotHeld` or `None`.

## Client budgets (review F3, F4, F11)

Each client has a budget (`roles::ClientBudget`): requests per loop pass, live surfaces (512),
`bsm_session_host_v1` binds (4), a token bucket for control requests (`report`,
`get_session_surface`, `ack_configure`, registry binds: 64 burst, 20 per second) and one for
commits (2000 burst, 1000 per second). A client that exceeds any of them is disconnected; for a
bridge the manager hears `SessionConnectionRejected` and `SessionGone`. Identical repeated
reports are dropped before they reach the manager. A hidden session's commits do not wake the
renderer. **tested** (`security.rs`: report flood, request flood with a lowered pass limit, surface
cap, destroyed surface, evidence arming; `roles::budget_tests`).

## Connection and role model

No shared socket exists. There are two kinds of listening socket and nothing else:

* the greeter socket (`Config::greeter_socket`, mode `0660`, optional chown to a group),
* one socket per registered session generation
  (`<session_socket_dir>/<session>-<generation>.sock`, mode `0666`).

A stream is **never** inserted into the Wayland display before it is verified, so a peer
that fails a check receives no protocol byte at all (the stream is closed). Accepted
streams are inserted with `Display::insert_client` and a `ClientState` that carries the
role. The client cannot name or change its role. Rejections are logged; a rejected
session connection is reported as `SceneEvent::SessionConnectionRejected` with a
reason (at most 32 per session, to bound event floods).

| Check                                 | Greeter            | Hosted session                                           |
| ------------------------------------- | ------------------ | -------------------------------------------------------- |
| uid equals the registered uid         | yes, at promotion  | yes, at accept (so a stranger is rejected before binding) |
| pid equals the registered pid         | yes                | yes (`BindHost.host_pid`)                                |
| `/proc/<pid>/cgroup` contains scope   | no                 | yes (`BindHost.scope`; a leading `/` makes it a path prefix, otherwise a path component). A bare unit name is spoofable by a same-uid process that creates a cgroup of that name in its delegated subtree: the helper should report the absolute cgroup path (review F7) |
| `/proc/<pid>/exe` equals `host_exe`   | no                 | **best effort, not effective across uids**: a mismatch rejects, but bsm has no capabilities so the link of any other uid is unreadable and skipped (review F7) |
| `SO_PEERPIDFD` (Linux 6.5+)           | **required**; used for the pid | **required**; used for the pid and to detect a peer that exited during verification. A peer that cannot be pinned (older kernel) is refused (review F8) |
| single use                            | one connection per generation | one concurrent connection: the socket file and listener are removed the moment the host is promoted and re-armed (same path, same checks) when that connection closes, so a restarted bridge or a bridge of a restarted bsm can reconnect. A rejected attempt does not burn the socket |

Two-step registration (agreed with the manager): `RegisterSession` (scope `""`,
`host_pid` 0) creates the socket and answers `SessionSocketReady`; a connection that
arrives before `BindHost` is uid-checked and then **held outside the display** (10 s) and
verified when `BindHost` arrives. Held connections are one per peer process (a newer one
from the same pid replaces the older), peers that already exited free their slot, and
when 32 are held the oldest is evicted, never the newest (review F9; a same-uid attacker
with more than 32 processes inside the window can still push the real bridge out, which
is a self-inflicted login failure). The greeter is handled the same way when it connects
before `RegisterGreeter`.

Recovery after a restart of bsm: the socket path is a pure function of session id and
generation (`<dir>/<id>-<generation>.sock`; the name is not secret, only the uid, pid and
scope checks are authoritative, review F13). `RegisterSession` for the same id and
generation unlinks and re-binds a socket left by the dead process, `BindHost` may bind a
registration that has no connection (a restarted bridge has a new pid), and a bridge
reconnects with backoff.

Disconnects: a greeter that vanishes emits `GreeterGone` and is deregistered (a new
generation must be registered for the next one); a hosted bridge that vanishes emits
`SessionGone`, its surfaces and evidence are dropped, and the session is no longer
presentable.

Global visibility is decided by `roles::may_see`, implemented with hand-written
`GlobalDispatch::can_view` for every global (Smithay's `delegate_*` macros create
ungated global dispatchers, so only their non-global halves are used).

| Global                      | Greeter | Hosted user |
| --------------------------- | :-----: | :---------: |
| `wl_compositor`             | yes     | yes         |
| `wl_subcompositor`          | yes (*) | no          |
| `wl_shm`                    | yes     | yes         |
| `wl_seat`                   | yes     | yes         |
| `wl_output`                 | yes     | yes         |
| `zxdg_output_manager_v1`    | yes     | no          |
| `zwlr_layer_shell_v1`       | yes     | no          |
| `xdg_wm_base`               | yes (**) | no         |
| `wp_viewporter`             | yes     | no          |
| `wp_fractional_scale_manager_v1` | yes | no         |
| `zwp_linux_dmabuf_v1`       | yes     | yes         |
| `wp_presentation`           | yes     | no          |
| `bsm_session_host_v1`       | no      | yes         |

(**) Qt's Wayland plugin will not start without a shell integration. The greeter may create
toplevels and popups; they are configured and acknowledged but never composed, never get
focus and popup grabs are dismissed at once. **tested** (`greeter_xdg_toplevels_are_accepted_but_never_composed`).

(*) An addition to the table in the architecture page: Qt binds `wl_subcompositor`.
It creates no capability and hosted clients do not get it.

**tested**: both registries are compared exactly (`security.rs`); a forged
`wl_registry.bind` of a hidden global kills the connection in both directions; wrong
uid, wrong pid, wrong scope and wrong exe are rejected and closed without a byte; a
bound-later connection receives nothing before `BindHost`; the socket is single use; one
session cannot use another's socket (`pid` of a real other process); greeter uid/pid
checks and one connection per generation. The peer checks are also unit tested against
the test process's own credentials with deliberately wrong expectations
(`peer::tests`). **unverified**: behaviour with different real uids (needs root), the
`SO_PEERPIDFD` fallback on kernels older than 6.5, cgroup scopes written by systemd.

## `bsm_session_host_v1`

Implemented server side: `identity` (session, generation, uid), `output_added`,
`output_changed`, `output_removed`, `input_state`, `closed`; `get_session_surface`,
`report`, per-output `bsm_session_surface_v1` with `configure`, `ack_configure` and
`presented`. Rules that are enforced:

* The role is re-read from the connection on every request; requests of a stale
  generation raise `stale_generation`. Unknown outputs raise `invalid_output`, a second
  surface for an output (or a surface that already has a role) `already_constructed`,
  inconsistent counts `invalid_report`. `report` must carry the generation string of the
  identity event.
* `configure` is sent immediately after `get_session_surface` (no xdg-style initial
  commit) and again, with a new serial, whenever the output changes.
* A buffer counts as evidence only when the latest configure was acknowledged **and**
  the attached buffer is exactly `width x height` at scale 1 with no transform.
  `SurfaceEvidence.presented_frames` counts such commits since the acknowledgement; it
  is reset to 0 when the output changes. `SessionSurfaces` is emitted on configure, ack,
  the first valid frame, and invalidation (not on every frame). Every forwarded `report`
  arms each surface of the session: its next valid, acked commit emits one more
  `SessionSurfaces` (with the incremented `presented_frames`), so the manager can prove a
  frame arrived after a report (the `DesktopReady` gate). At most one per surface per report.
* `presented`: for a visible session after every composed valid commit (sequence
  numbers increase); for a session that is not on screen once, for the first valid
  commit after the ack, as the ready-frame proof.
* Frame callbacks: every frame to a visible session, at most every 250 ms to a hidden
  one (it keeps serving applications without rendering at full rate).
* Only the root `wl_surface` of a session surface is ever composed, never its subsurface
  tree, at one buffer pixel per output pixel regardless of the output's fractional scale.

**tested** (`security.rs`, `extras.rs`): identity, errors, report forwarding, evidence
rules, `presented` ordering, buffer release on replacement (the bridge needs its shm
slots back), 1:1 composition on a 1.5 scale output. **unverified**: dmabuf buffers from
a real GPU client (the global exists and `dmabuf_imported` imports into the active
renderer; no test client sends dmabufs).

## Scene, protection and presentable sessions

`engine::Engine` holds the base scene (`Cover`, `Greeter`, `Session(key)`), the
protected set and the running transition. The compositor asks it for a `Plan` before
every frame and tells it when the frame was composed. Everything on screen derives from
the plan; there is no other path to pixels.

A session is composed only when it is registered, its host is connected, it is **not
protected**, and **every current output** has a surface with a valid frame presented
after its acknowledged configure (`ProbeView::session_presentable`). Otherwise the
output shows the cover colour: a new output with no frame yet blanks the whole desktop
until proven (**tested** `a_foreground_session_with_an_unproven_new_output_is_not_composed`).
This is deliberately stricter than per-output composition.

`Protect`:

* removes the session from the scene at once (the scene becomes the cover),
* revokes its input immediately (owner becomes `None`, keyboard leave, `input_state(0)`,
  `InputOwnerChanged(None)`),
* replies `Protected` only after a frame that was **planned after the protection** was shown
  on every output. The engine keeps a protection queued until the next plan, so a flip that
  was already in flight when `Protect` arrived (and still shows the session) cannot release
  the reply (review F1a); a pending protect keeps asking for a redraw (F1b). On DRM "shown"
  means every queued page flip completed.
* a frame that did not reach every output (a render or queue error, an output that was
  skipped, a flip that never came within 500 ms) acknowledges nothing: the protection stays
  owed, the scene is composed again after a 100 ms back-off, and after 5 failures in a row the
  running transition is given up with `Failed` (cover plus greeter). The compositor never
  invents an acknowledgement; liveness of the state machine is the manager's timeout
  (F1c, F1d). While the seat is paused (VT switched away) nothing is acknowledged either.
* DPMS off puts the cover on the scanout buffers first and switches the displays off after
  that flip, so `Protect` while blanked and the wake-up never expose the old desktop; the first
  frame after wake is the current scene (F1e; the order on real hardware is unverified).

An unknown session is protected trivially and still answered after a frame. A stale
generation is dropped.

**tested**: `protect_removes_pixels_and_input_before_the_reply_and_input_handover_is_refused`,
`output_changes_never_expose_a_protected_session` (output removed, added and reshaped
while locked: no protected pixel on any output on any frame).

### Input owner

Exactly one of `None`, `Greeter`, `Session(id)`. Keyboard and pointer focus and every
`wl_seat` event go only to surfaces of the owner. `SetInputOwner` is queued and applied
at the next frame boundary. It is **refused**, with `InputOwnerChanged` naming the owner
that stays, unless the target session is the *committed, unprotected, presentable
foreground*: not protected, not merely a transition target, and the transition that
reveals it has reached `Committed`. The greeter therefore keeps the keyboard until the
first valid desktop frame is on screen. Every frame the owner is re-checked and dropped
to `None` if it stopped being allowed (crash, unregister, scene change).

Logging (through `tracing`, because `bsm` installs a tracing subscriber without the `log` bridge; info, visible in `journalctl -u bsm`): `input owner A -> B`, `keyboard focus -> greeter|session ...`,
`first key N delivered to ...` after every focus change, and a warning `key N has no keyboard focus and is
dropped (...)` that says whether the greeter is connected and whether any greeter layer surface accepts
the keyboard (`keyboard_interactivity` none means nobody can type). On DRM each libinput device added is
logged with its capabilities. **tested**: `frames::greeter_keyboard_works_whatever_order_owner_and_connection_arrive_in`
(owner set before the greeter connects; on-demand and exclusive interactivity) passes through the harness
path; the libinput-to-seat path itself is unverified until a VM run shows the log lines.

Cursor: only the owner's cursor is drawn. A client cursor surface larger than 256 px is
replaced by the built-in arrow. **tested** (`input_goes_only_to_the_owner`,
`the_cursor_is_drawn_for_the_owner_only`).

## Transitions

`BeginTransition` is driven by the frame clock and `bsm_proto::transition::trajectory`.
Layer order per frame, top to bottom: cursor, cover overlay, then session and greeter in
the order the trajectory gives; the clear colour is the opaque cover. Greeter and cover
opacity are group-correct because the greeter is drawn opaque and a cover-coloured
overlay of alpha `1 - greeter_opacity` is drawn over it (`PowerFade`: alpha `cover_opacity`).
The same plan is used for every output in a frame, so outputs advance in lockstep.

| Kind            | Plan                                                                                                   | End scene |
| --------------- | ------------------------------------------------------------------------------------------------------ | --------- |
| `NewLoginFade`  | greeter at full opacity, offset 0; target session above at `ease_out_cubic(t)`                          | session   |
| `UnlockSlideUp` | greeter translated by `-ease_unlock(t)` output heights; target session **only in the area the greeter vacated** | session |
| `Lock`, `SwitchUser`, `Suspend` | source is protected first (by the engine if the manager has not); greeter fades in over the cover | greeter |
| `LogoutFade`    | greeter underneath at 1; source session above at `1 - ease_in_out_cubic(t)`                             | greeter   |
| `PowerFade`     | whatever is on screen, cover overlay `ease_in_out_cubic(t)`; the cover is held after commit            | cover     |

* Unlock clips the session to the vacated strip, so a translucent greeter reveals the
  cover, never an unauthorised or early session pixel. `BeginTransition(target)` for
  `NewLoginFade` and `UnlockSlideUp` is what lifts the target's protection (there is no
  unprotect command); a failed or cancelled transition puts the protection back.
* Events: `Prepared` (validated, at begin), `Animating` (first frame composed),
  `Committed` (final frame composed; the base scene swaps), `Completed` (the frame after,
  confirming the committed scene). A transition that cannot start emits `Failed`.
* Validation happens before the first frame and on **every** frame (greeter presentable,
  target/source registered, unprotected, presentable). A failure emits `Failed` after
  restoring the safe scene: cover plus greeter, every touched session protected, never a
  desktop buffer.
* Only one transition at a time; a second `BeginTransition` emits `Failed` for it and
  leaves the running one alone.
* Reduced motion: progress is 1 in the first frame, with the same checks and events.
* `CancelTransition` before `Committed` restores the scene and protection from before the
  transition and emits `Cancelled`. **Committed `PowerFade`** can also be cancelled (the
  manager does this when logind refuses): the cover fades back out onto the previously
  authorised scene and `Cancelled` follows. `Suspend` cannot be cancelled after commit.
* Time: `now_ms` is the compositor clock (wall clock, or the harness's stepped clock);
  transition time zero is the first frame planned after the command.

`ShowGreeterOnly` cancels a running transition (`Cancelled`) and shows the greeter scene.
When the greeter is not connected or has no mapped surface on some output, the plan has
no greeter layer: the opaque cover is shown until one is back.

### Greeter crash

A greeter that disconnects while a session is protected or the seat is locked leaves the
cover. **tested** (`greeter_crash_while_protected_shows_the_opaque_cover_until_a_greeter_returns`):
every pixel of every output is the cover colour, no stale greeter and no desktop; a new
greeter registered with a new generation brings the lock scene back.

## Backends

| Backend    | State                                                                                                                     |
| ---------- | ------------------------------------------------------------------------------------------------------------------------- |
| `Headless` | Pixman into memory, any output list (two monitors in the tests), stepped or wall clock, RGBA capture, PNG dump. **tested** |
| `Winit`    | GLES in a window; one output `WL-1` that follows the window. Smoke run observed (below). **tested by a manual smoke run**   |
| `Drm`      | libseat, udev hotplug, DRM/KMS, GBM, GLES, libinput, VT switching (Ctrl+Alt+Fn), session pause/resume, DPMS. **compiles; unverified at runtime** |

### Winit smoke

`cargo run --example dev_compositor` opens the winit backend, registers a fake greeter
(blue gradient with a red band) and a fake hosted session (green checkerboard) over real
sockets, then runs: new login fade, lock, unlock slide up, power fade, refused power
action (cancel). It was run inside a private headless `sway` (the development host had
no running Wayland session at the time) and captured with `grim`; the run ended with
exit code 0. The contact sheet is `docs/bsm/img/compositor-winit-smoke.png`: cover, then
greeter, three identical greeter frames, the unlock slide caught mid-way (greeter moved
up, session revealed from the bottom, orientation correct), then the session. One
harmless `eglQuerySurface: BAD_SURFACE` line appears at start-up from the buffer-age
query before the first swap. `--hold` keeps the greeter only.

### Drm backend: what is and is not known

Written against Smithay 0.7 and compiled in the default feature set. It has **not** been
run: the development host owns its seat. It is meant to be exercised in the QEMU guest
(`tests/distro_vm.py`). Structural properties built in:

* The scene engine hears about a frame (`Protected`, transition phases, frame callbacks)
  only when all queued page flips completed (`State::awaiting`, `drm_vblank`). If a flip
  never arrives within 500 ms (or an output failed) the frame is a *failed* frame: nothing
  is acknowledged and it is retried (see above).
* Boot handover: the DRM device is opened without disabling any CRTC, an active CRTC keeps
  its mode, only heads bsm does not drive are switched off. With `Config::hold_first_scene`
  and at least one lit CRTC at start, nothing is presented until the plan has a greeter or
  session layer (or the hold ends and a cover is presented). `FirstScene` fires `Presented`
  after the flip of the first scene frame, `TimedOut`, or `NotHeld` (headless, winit, no
  hold, no lit CRTC), and the compositor logs `first scene presented (reason) after N ms`.
  The hold decision and `FirstScene` are unit tested; the path itself is for the QEMU run.
* Pause (VT switched away): rendering and libinput stop; the frame in flight is dropped and its protections are owed again.
  Resume: libinput and the DRM device are reactivated, every compositor is reset, connectors
  are re-scanned and everything is redrawn from the plan, never from an old buffer.
* Hotplug: udev `Changed` re-scans connectors; outputs appear and disappear through
  `set_outputs`, which tells every hosted session (`output_added/changed/removed`, new
  configure serials) and invalidates evidence.
* Rendering is on demand (a commit, a command, a running transition) plus vblank.

Not known until run on real or virtual hardware: connector/crtc assignment on exotic
topologies, multi-GPU (only the primary GPU is used), DPMS through the legacy `DPMS`
property on atomic drivers, direct scanout and overlay-plane behaviour, VT switch
latency, whether `frame_submitted` timing matches the 500 ms flip timeout on virtio-gpu.
The zero-copy question is open: nothing here claims it, and the cost of a session frame
is measured by the bridge side (shm copy today).

## Tests

Run in `packages/bsm-compositor` (`CARGO_TARGET_DIR=/tmp/bsm-target/bsm-compositor`):

```text
cargo test        # 82 tests: 32 unit, 6 extras, 22 frames, 22 security (plus doc tests)
```

All passed, repeatedly (six consecutive runs without a failure). Artifacts are written to
`$BSM_TEST_ARTIFACTS` (default `<tmp>/bsm-compositor-artifacts/<test>/`): a PNG of every
composed frame (`frames/frame-<seq>-<output>.png`) and `trajectory.csv`
(`test,frame,time_ms,output,t,greeter_y,session_opacity,cover_overlay,sample`).

| Proof                                   | Test                                                                                           |
| --------------------------------------- | ---------------------------------------------------------------------------------------------- |
| New login: greeter constant at full opacity, session opacity 0 to 1 above it, two outputs byte-identical every frame | `frames::new_login_fades_the_desktop_in_above_a_constant_greeter_on_two_outputs` |
| Unlock: greeter y follows the easing exactly, only authorised pixels beneath, no cover pixel at any position, two outputs in sync, input not handed over before `Committed` | `frames::unlock_slides_the_greeter_up_over_only_the_authorised_session` |
| Reduced motion = one secure frame       | `frames::reduced_motion_commits_in_one_secure_frame`                                           |
| Failed transition returns to cover+greeter | `frames::failed_transition_returns_to_cover_plus_greeter_and_hides_the_target`              |
| Cancel restores scene and protection    | `frames::cancel_before_commit_restores_the_previous_scene`                                     |
| Protect before input handover           | `frames::protect_removes_pixels_and_input_before_the_reply_and_input_handover_is_refused`      |
| Power fade holds, refused fades back    | `frames::power_fade_holds_the_cover_and_a_refused_power_action_fades_back`                     |
| Greeter crash                           | `frames::greeter_crash_while_protected_shows_the_opaque_cover_until_a_greeter_returns`         |
| Hotplug while protected / foreground    | `frames::output_changes_never_expose_a_protected_session`, `frames::a_foreground_session_with_an_unproven_new_output_is_not_composed` |
| Roles, rejections, protocol errors      | `security.rs` (13 tests)                                                                       |
| Engine invariants                       | `engine::tests` (15 tests), `peer::tests`, `roles::tests`                                      |

The tests drive the real compositor thread with real Wayland clients over real sockets.
The clients (`tests/common/mod.rs`) authenticate with the test process's own credentials
and a real second process (`sleep`) stands in for "another process".

## Open issues

* Hosted identity is uid plus pid (plus cgroup scope, which only separates other uids when the
  helper reports an absolute path, and an exe check that is inert across uids). Spec section 3
  prefers a socketpair created by the helper and inherited by the bridge; that would remove the
  path, the pending list and these guesses in one step and is not implemented.
* Request flood protection covers the interfaces bsm dispatches through its own wrappers
  (surfaces, regions, subsurfaces, shm, seat, output, xdg, viewporter, fractional scale,
  presentation, the session protocol, registry binds). Requests handled inside
  `wayland-backend` itself (`wl_display.sync`, `get_registry`) and layer-shell/dmabuf
  requests are not counted; per-client buffer and dmabuf-import quotas do not exist.
* `evt_tx` is an unbounded `std::sync::mpsc` sender. Report and evidence events are bounded by
  the per-client budgets above rather than by a bounded channel.
* The greeter's `Overlay` layer surfaces draw above the whole greeter fade (they are part
  of the greeter group), which is intended; popups of layer surfaces are not composed.
* Output scale: sessions are always 1:1 in pixels; the greeter uses fractional scale.
  Output transforms other than `Normal` are announced but not applied to the render (the
  hardware backends render untransformed).
* Touch input is not routed (pointer and keyboard only).
* The frame clock on DRM is "render on demand plus vblank"; there is no predictive frame
  scheduling, so animation smoothness on hardware is unmeasured.
