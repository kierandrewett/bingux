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
| `/proc/<pid>/cgroup` contains scope   | no                 | yes (`BindHost.scope`; a leading `/` makes it a path prefix, otherwise a path component) |
| `/proc/<pid>/exe` equals `host_exe`   | no                 | **best effort**: a mismatch rejects, an unreadable link (another uid, no `CAP_SYS_PTRACE`) is skipped |
| `SO_PEERPIDFD` (Linux 6.5+)           | used for the pid   | used for the pid and to detect a peer that exited during verification |
| single use                            | one connection per generation | the socket file and listener are removed the moment the host is promoted; a rejected attempt does not burn the socket |

Two-step registration (agreed with the manager): `RegisterSession` (scope `""`,
`host_pid` 0) creates the socket and answers `SessionSocketReady`; a connection that
arrives before `BindHost` is uid-checked and then **held outside the display** (at most
4, 10 s) and verified when `BindHost` arrives. The greeter is handled the same way when
it connects before `RegisterGreeter`.

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
| `wp_viewporter`             | yes     | no          |
| `wp_fractional_scale_manager_v1` | yes | no         |
| `zwp_linux_dmabuf_v1`       | yes     | yes         |
| `wp_presentation`           | yes     | no          |
| `bsm_session_host_v1`       | no      | yes         |

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
  the first valid frame, and invalidation (not on every frame).
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
* replies `Protected` only after a frame was planned and composed without it. On DRM that
  means after the page flip completed (the engine is told at vblank).

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
  only when all queued page flips completed (`State::awaiting`, `drm_vblank`). A 500 ms
  watchdog releases it if a flip never arrives, so a broken output cannot freeze the
  state machine.
* Pause (VT switched away): rendering and libinput stop, the awaited frame is released.
  Resume: libinput and the DRM device are reactivated, every compositor is reset, connectors
  are re-scanned and everything is redrawn from the plan, never from an old buffer.
* Hotplug: udev `Changed` re-scans connectors; outputs appear and disappear through
  `set_outputs`, which tells every hosted session (`output_added/changed/removed`, new
  configure serials) and invalidates evidence.
* Rendering is on demand (a commit, a command, a running transition) plus vblank.

Not known until run on real or virtual hardware: connector/crtc assignment on exotic
topologies, multi-GPU (only the primary GPU is used), DPMS through the legacy `DPMS`
property on atomic drivers, direct scanout and overlay-plane behaviour, VT switch
latency, whether `frame_submitted` timing matches the 500 ms watchdog on virtio-gpu.
The zero-copy question is open: nothing here claims it, and the cost of a session frame
is measured by the bridge side (shm copy today).

## Tests

Run in `packages/bsm-compositor` (`CARGO_TARGET_DIR=/tmp/bsm-target/bsm-compositor`):

```text
cargo test        # 47 tests: 21 unit, 4 extras, 11 frames, 11 security
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
| Roles, rejections, protocol errors      | `security.rs` (11 tests)                                                                       |
| Engine invariants                       | `engine::tests` (15 tests), `peer::tests`, `roles::tests`                                      |

The tests drive the real compositor thread with real Wayland clients over real sockets.
The clients (`tests/common/mod.rs`) authenticate with the test process's own credentials
and a real second process (`sleep`) stands in for "another process".

## Open issues

* `SO_PEERPIDFD` only protects against pid reuse on Linux 6.5+. Older kernels fall back to
  the `SO_PEERCRED` pid, which can name a recycled number in a narrow window.
* The greeter's `Overlay` layer surfaces draw above the whole greeter fade (they are part
  of the greeter group), which is intended; popups of layer surfaces are not composed.
* Output scale: sessions are always 1:1 in pixels; the greeter uses fractional scale.
  Output transforms other than `Normal` are announced but not applied to the render (the
  hardware backends render untransformed).
* Touch input is not routed (pointer and keyboard only).
* The frame clock on DRM is "render on demand plus vblank"; there is no predictive frame
  scheduling, so animation smoothness on hardware is unmeasured.
