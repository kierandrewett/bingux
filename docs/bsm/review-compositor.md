# bsm compositor: connection and role model, security review

Scope: `packages/bsm-compositor` (broker sockets, `SO_PEERCRED` / `SO_PEERPIDFD`
checks, role-filtered globals, `bsm_session_host_v1`, input routing, protect and
presentability, transition validation, greeter crash cover), read against spec
section 3 and the invariants in `docs/bsm/architecture.md`. Adjacent code was read
where it decides an outcome: `packages/bsm/src/run.rs` (watchdog, event channel),
`packages/bsm-manager/src/runtime/driver.rs` (event forwarder),
`packaging/bsm/*`, and the vendored dependencies `smithay 0.7.0`,
`wayland-backend 0.3.17`, `rustix 1.1.5` from `~/.cargo/registry`.

Method and honesty statement. This is a read-only review of source. Nothing was
built, run, fuzzed or sent over a socket, and no source file was edited. Every
finding below is **[CONFIRMED BY READING]** (the code path is unambiguous in the
files cited) unless it carries **[INFERENCE]** (depends on a dependency's or the
kernel's behaviour that was read but not exercised) or **[NOT REPRODUCED]**. No
finding was demonstrated on a running system. File:line references are to the
tree as read; `state.rs` was being edited by another agent during the review (frame
timing fields appeared), so lines in that file may have drifted by a few.

Severity scale: **High** = breaks an architecture invariant against a hostile or
unlucky peer with no extra preconditions. **Medium** = breaks an invariant or takes
down the seat under a realistic precondition (authorised client, hardware error).
**Low** = hardening, defence in depth, or needs a rare precondition. **Info** =
accuracy of docs or tests.

## Summary

| #  | Severity | Title                                                                                         | Invariant / spec           |
| -- | -------- | --------------------------------------------------------------------------------------------- | -------------------------- |
| F1 | High     | `Protected` and transition phases are acknowledged without proof that a new frame was shown  | inv. 3, 8; spec 8, 15      |
| F2 | Medium   | Implicit pointer grab survives `Protect` and owner changes                                    | inv. 1, 3; spec 3          |
| F3 | Medium   | One authorised client can starve the whole event loop; the systemd watchdog cannot see it     | availability, inv. 9       |
| F4 | Medium   | Unbounded compositor-to-manager event flow shares one bounded queue with auth traffic         | availability, inv. 5       |
| F5 | Low      | Input is revoked on the `Protect` command only; implicit protection paths lag by a frame      | inv. 1, 3                  |
| F6 | Low      | `SetInputOwner(Greeter)` is always allowed, and `owner_allowed` ignores presentability        | inv. 1, 3                  |
| F7 | Low      | Hosted identity rests on uid plus pid; the scope check is spoofable and the exe check is inert across uids | spec 3, arch. role table |
| F8 | Low      | pid-recycle TOCTOU remains where `SO_PEERPIDFD` is absent (kernel < 6.5)                      | spec 3                     |
| F9 | Low      | Pending-connection slots can be exhausted; the legitimate peer is dropped silently            | availability               |
| F10| Low      | Hosted surface liveness is never tracked (destroyed `wl_surface` keeps the session "presentable") | inv. 8                 |
| F11| Low      | No per-client resource caps (hosts, surfaces, buffers, dmabuf imports, redraw triggers)       | availability               |
| F12| Info     | Keys held at hand-off are listed in `wl_keyboard.enter`; docs say otherwise                   | arch. "Transitions"        |
| F13| Info     | Doc and code differences (random socket name, test-only socket modes, `UCred` niche)          | docs                       |

Checked and found sound is in [Controls that held](#controls-that-held).

---

## F1 (High): `Protected` and transition phases are acknowledged without proof that a new frame was shown

Invariant 3 says a session is protected, "acknowledged by a rendered frame", before
any other principal gets input or pixels. `docs/bsm/compositor.md` says `Protected`
is sent "only after a frame was planned and composed without it. On DRM that means
after the page flip completed". The code gives that guarantee only on the headless
path. Five independent ways to get an early or false acknowledgement on DRM:

**F1a. A flip that was queued before the command releases the reply.**
`Engine::protect` pushes the key to `pending_protected` (`engine.rs:229-237`).
`Engine::frame_done` drains the whole list whatever frame is being completed
(`engine.rs:504-507`). On DRM the frame in flight is held in `State::awaiting`
(`state.rs:641-645`, `676-678`) and completed by `complete_awaiting`
(`state.rs:710-714`) with the plan it was *rendered from*, which pre-dates the
`Protect` command. Sequence:

1. Frame N (plan contains session S) is queued; `awaiting = Some(N)`.
2. `SceneCommand::Protect(S)` arrives (`commands.rs:24-35`): `engine.protect` queues
   the reply, `revoke_input_of` runs. `render_frame` returns early because
   `awaiting` is set (`state.rs:642-645`), so no plan without S exists yet.
3. The page flip of frame N completes: `drm_vblank` -> `complete_awaiting` ->
   `complete_frame` -> `engine.frame_done` emits `Protected`
   (`gfx_drm.rs:295-310`, `state.rs:686-707`).

The screen still shows frame N, which contains S. The engine unit test
`protect_reply_waits_for_a_composed_frame_without_the_session` (`engine.rs:694-717`)
never interleaves a plan with a protect, so it cannot catch this.

**F1b. The completion also clears the redraw request.**
`complete_frame` ends with `self.needs_redraw = self.engine.transition_running()`
(`state.rs:706`). The `needs_redraw = true` that `handle_command` set in step 2
(`commands.rs:67`) is overwritten. `after_dispatch` only renders on
`needs_redraw` (`gfx.rs:295-297`), so with no transition running the frame without
S is not planned until something else invalidates (a commit, a pointer move, the
next command). Until then S stays on screen although `Protected` was sent. Most
flows send another command right after `Protected`, which masks this; `Suspend`
("Protect all, ready the lock scene, then sleep") is the flow where a stale
scanout buffer matters.

**F1c. The 500 ms watchdog acknowledges a flip that never happened.**
`gfx.rs:289-293` calls `complete_awaiting` after 500 ms. That is the same
code path, so `Protected` (and `Committed` for a transition) is emitted although
the screen has not changed.

**F1d. DRM `render` swallows per-output failures and reports success.**
`gfx_drm.rs:125-160`: an output with `pending_flip` is skipped (`135-137`); a
`render_frame` or `queue_frame` error is only logged (`152`, `156`); the function
returns `Ok((vec![], queued))`. When no output queued anything, `deferred` is false
and `render_frame` calls `complete_frame` at once (`state.rs:676-680`). The same
happens when `gfx.render` returns `Err` (`state.rs:664-670`, "render failed" is
logged and execution continues to `complete_frame`). With two outputs where A
flips and B fails, completion waits only for A (`drm_vblank` checks
`pending_flip`, `gfx_drm.rs:304`) and B keeps showing S. A transient `EBUSY` or a
GPU reset is enough; no attacker is needed.

**F1e. DPMS off skips rendering but still acknowledges.**
`render_frame` skips `gfx.render` when `dpms_on` is false (`state.rs:662`) and then
completes the frame, so `Protect` while the screen is blanked is acknowledged
immediately. `SetDpms{on:true}` writes the DPMS property first
(`commands.rs:57-62`, `gfx_drm.rs:93-122`) and only afterwards does
`needs_redraw` (`commands.rs:67`) lead to a render, so the CRTC scans out the last
buffer (the pre-blank desktop) until the first new flip. Spec section 15 requires
"Display DPMS off/on and suspend/resume preserve secure lock state". [INFERENCE]
for the length of the exposure: it is at least one frame plus the panel wake time,
and it depends on the driver keeping the old framebuffer attached across DPMS.

Exploit sketch. No malicious peer is needed for F1b-F1e. For F1a a user who can
time `Lock` against a busy frame (a video playing in S keeps a flip in flight most
of the time) gets `Protected` one vblank before the cover; with F1b the cover may
not follow until input arrives. For the suspend path: `Protected` arrives, the
manager calls logind `Suspend`, the machine sleeps with S in the scanout buffer.
[NOT REPRODUCED]

Minimal fix.
1. In `Engine`, keep two lists: `queued_protect` (added by `protect`) and
   `in_flight_protect`. `plan()` moves `queued_protect` into `in_flight_protect`;
   `frame_done()` emits only `in_flight_protect`. A frame can then only release
   replies that were queued before *its* plan was made.
2. In `State::render_frame`, set `self.needs_redraw = false` at plan time and make
   `complete_frame` do `self.needs_redraw |= self.engine.transition_running() ||
   self.engine.has_queued_protect()` instead of assigning.
3. Make the backend return an honest result: `DrmGfx::render` returns
   `composed: bool`, true only if every output either queued a frame for this plan
   or reported an empty (unchanged, already correct) frame. When false, do not
   call `complete_frame`; keep the plan pending, set `needs_redraw`, retry on the
   next dispatch, and after N consecutive failures emit a failure event rather than
   `Protected`.
4. Replace the 500 ms "release" (`gfx.rs:289-293`) by the same failure path.
   Liveness of the state machine is the manager's timeout, not a fake frame.
5. For DPMS: on `SetDpms{on:true}`, compose a plan frame (cover at least) before
   writing the DPMS property, or write the property and treat the output as
   not-yet-shown until a flip following the property write completes.

---

## F2 (Medium): Implicit pointer grab survives `Protect` and owner changes

`clear_focus` (`input.rs:108-119`) sets keyboard focus to `None` and then calls
`ptr.motion(self, None, ...)`. `set_owner_now` documents "so no event can reach two
principals" (`input.rs:64-65`). With a button held, Smithay has installed a
`ClickGrab` (`smithay-0.7.0/src/input/pointer/grab.rs:233-249`) whose `motion`
ignores the focus argument and re-sends to the grab's original surface
(`grab.rs:356-365`); `axis` and `button` go through the same handle
(`grab.rs:377-387`). The grab only ends when all buttons are released or the grab
surface dies (`pointer/mod.rs:887-893`).

Effect. If `Protect(S)` (or any owner change) happens while a button is down in S,
S keeps receiving motion, scroll and the button release from the physical seat
until the button is up, although `owner` is `None` or the greeter. S can also keep
calling `wl_pointer.set_cursor` (Smithay's focus test passes because the grab keeps
S as pointer focus, `wayland/seat/pointer.rs:524-525`), which controls the cursor
that `SeatHandler::cursor_image` stores (`state.rs:328-333`) and the compositor
draws while another principal owns input. The reverse also holds: a greeter click
grab outlives the hand-over to a session. The data exposed is pointer position
and button state, not pixels. Needs a held button at the instant of the change.
[INFERENCE: grab semantics read in Smithay, not exercised]

Minimal fix. In `clear_focus`, call
`ptr.unset_grab(self, serial, 0)` (`PointerHandle::unset_grab`,
`smithay-0.7.0/src/input/pointer/mod.rs:178`) before the `motion(None)`. Add a
frame-harness test: press a button in session S, `Protect(S)`, move the pointer,
assert S sees no further pointer event.

---

## F3 (Medium): one authorised client can starve the whole event loop; the watchdog cannot see it

`state.rs:815` runs `display.dispatch_clients(state).expect("dispatch clients")`
from the single calloop thread. In `wayland-backend 0.3.17`,
`dispatch_events_for` loops until `next_request` returns `WouldBlock`
(`rs/server_impl/common_poll.rs:152-327`) and `dispatch_all_clients` repeats that
until epoll reports nothing (`common_poll.rs:81-107`). There is no request budget.
A client that writes trivial requests (`wl_surface.damage`, `wl_surface.commit`,
`bsm_session_host_v1.report`) faster than the compositor parses them never returns
control: no frame is planned, no `SceneCommand` is handled, no input is routed, no
`Protected` is sent, and the greeter cannot be shown. The same unbounded
`loop { accept() }` pattern is used by the listener sources (`broker.rs:86-96`,
`248-259`) but those need a peer that can connect.

Who can do it: the greeter (uid `bingux-greeter`) and the verified bridge of a
hosted session. A bridge is a user-privileged process, so a compromised user
session that controls it (or a bug in `bsm-host`) takes the seat down for every
user.

Watchdog. `bsm.service` has `WatchdogSec=30` (`packaging/bsm/systemd/bsm.service:37`)
and the docs imply a hung compositor is restarted. But `WATCHDOG=1` is sent by the
`bsm-notify` thread (`packages/bsm/src/run.rs:311-333`) whose loop condition is
`alive && !handle.is_finished()`. Both refer to the manager driver, not to the
compositor thread. A wedged compositor loop is therefore never detected; systemd
keeps `bsm` running with a frozen display. [CONFIRMED BY READING for the watchdog;
INFERENCE for the starvation rate: compositor parse speed vs client `send(2)` speed
was not measured.]

Minimal fix.
- Tie the heartbeat to the compositor: a calloop timer on the compositor thread
  stores `Instant::now()` into an `AtomicU64` every 250 ms (`loop_timeout` already
  wakes at 250 ms, `gfx.rs:283-285`), and `bsm-notify` sends `WATCHDOG=1` only if
  that value is younger than a limit.
- Bound a client's share of a dispatch. `wayland-backend` has no knob, so either
  carry a patch that returns from `dispatch_events_for` after N requests, or count
  requests per client at the Dispatch layer for the types a client can spam
  (`report`, `commit`) and disconnect on overflow (see F4 and F11).

---

## F4 (Medium): unbounded compositor-to-manager event flow shares one bounded queue with auth traffic

Every `bsm_session_host_v1.report` is forwarded as `SceneEvent::SessionReport`
with no rate limit (`host.rs:237-292`, emit at `282-286`). `get_session_surface`
followed by `destroy` on the same `wl_surface` (a repeated role is allowed because
`give_role` only fails on a different role, `host.rs:200`) emits a
`SessionSurfaces` each cycle (`host.rs:235`, `349`). Each armed surface emits one
more per report (`host.rs:287-291`, `380-390`). The channel is
`std::sync::mpsc::channel()` (unbounded, `packages/bsm/src/run.rs:285`).

The consumer is the `bsm-scene-events` thread, which does a blocking `send` into
the driver's `sync_channel(QUEUE = 1024)` (`bsm-manager/src/runtime/driver.rs:33`,
`53`, `143-148`). The greeter server and the helper client send into the same
`tx` (`driver.rs:141-142`). When the bridge outpaces the driver:

1. memory grows without bound in the compositor-side channel, and
2. the shared driver queue stays full, so greeter keystroke/auth verdicts and
   helper replies wait behind scene events (the greeter server uses `try_send`
   and drops, `greeter_server.rs`; not read in detail). Another user's login can
   fail or time out.

The manager treats stale or repeated reports as no-ops
(`bsm-manager/src/core/session.rs:326-360`), so the events carry no value past the
first few.

Minimal fix. Per session, allow at most (for example) 64 `report` requests in
total per generation plus 8 per second, and at most 64 `get_session_surface`
requests per generation; on excess `post_error(InvalidReport)` (which closes the
connection). Coalesce `SessionSurfaces` so that at most one is queued per session
until the manager has consumed the previous one. Prefer a bounded compositor event
channel with drop-oldest for the evidence events only.

---

## F5 (Low): input is revoked on the `Protect` command only; implicit protection paths lag

`SceneCommand::Protect` calls `revoke_input_of` (`commands.rs:33`). Three other
paths protect or un-foreground a session without touching input:
`Engine::begin` for `Lock`/`SwitchUser`/`Suspend` calls `protect` itself
(`engine.rs:349-357`), `Engine::fail_safe` inserts into `protected`
(`engine.rs:364-372`), and `show_greeter_only` changes `base` (`engine.rs:250-266`).
Input is reconciled only in `render_frame` (`state.rs:657`), which returns early
while a flip is in flight (`state.rs:642-645`). Until then `kb.input` and pointer
events go to the keyboard/pointer focus set at the last refresh, which is the
protected session. The window is one flip normally and up to the 500 ms watchdog
when flips stall. The manager is documented to send `Protect` first, so this is
defence in depth. The `SetInputOwner` / `Protect` race itself is sound (see
[Controls that held](#controls-that-held)).

Minimal fix. Call `self.reconcile_input()` at the end of `handle_command`
(`commands.rs:64-67`) and move the `reconcile_input` call in `render_frame` above
the `awaiting` early return.

---

## F6 (Low): `SetInputOwner(Greeter)` is always allowed; `owner_allowed` ignores presentability

`owner_allowed` returns true for `None` and `Greeter` unconditionally
(`input.rs:31-34`). A manager bug (or a replayed command) can hand the physical
keyboard to the greeter process while an unprotected desktop is the committed
foreground, or while the greeter is not on screen. Keystrokes go to the greeter
uid, which is a different principal from the user who is typing into S. In the
other direction, `owner_allowed(Session)` requires only `foreground_session()`
(`engine.rs:208-217`), not `session_presentable`: after an output hotplug the
cover is shown (`state.rs:474-486`) while S keeps keyboard and pointer focus
(`input.rs:182-196`) and the user types into a desktop they cannot see. The
compositor "decides no policy", but these are cheap cross-checks.

Minimal fix. Allow `Greeter` only when `last_plan.greeter.is_some()` or the base
is `Greeter`, and require the session to be `session_presentable` in
`owner_allowed`. The engine-level `reconcile_input` then handles both.

---

## F7 (Low): hosted identity is uid plus pid; the scope check is spoofable and the exe check is inert across uids

What actually gates `HOSTED_USER`:

- uid equality: effective (`peer.rs:123-129`, checked at accept, `broker.rs:325`).
- pid equality against the pid the helper reports: effective. The pid is read from
  `SO_PEERPIDFD` when available (`peer.rs:22-33`), which names the process that
  called `connect`, not the fd's later holder.
- cgroup scope: the helper reports a **bare unit name** (`bsm-helper/src/session.rs:317-320`,
  `384`), so `cgroup_matches` takes the "path component" branch
  (`peer.rs:76-88`, `path.split('/').any(|c| c == scope)`). Any process of the same
  uid can `mkdir` a cgroup with that name inside its delegated `user@UID.service`
  subtree and move itself in (systemd delegates that subtree to the user).
  [INFERENCE: cgroup delegation not tested on the target image.] So the scope check
  does not separate a same-uid attacker from the bridge. It still separates other
  uids' processes and processes outside the named scope for honest software.
- `/proc/<pid>/exe`: `exe_check` reads the link and "unreadable" counts as pass
  (`peer.rs:99-111`, `133-147`). bsm runs as uid `bsm` with an empty capability set
  (`bsm.service:64`), so for any peer with another uid (always, for a hosted user)
  the link is unreadable and the check never runs. `docs/bsm/compositor.md:60`
  admits "best effort"; the architecture table (`architecture.md:72`) lists it as a
  check without that caveat in the same cell.

So the effective boundary is uid + pid + (with pidfd) liveness. With the
architecture doc's own caveat that is "no wider than what that user can already
do" for the same-uid case, which I agree with: a same-uid process that wins the
pid check can show pixels and receive the user's own physical input for that
user's session, and cannot reach another uid. Severity stays Low.

Spec section 3 prefers "socketpairs ... created and passed by bsm/launcher at
spawn". That would remove the path, the pending queue, the pid/scope/exe guesswork
and F8/F9 in one step: `bsm-helper` (root) creates a `socketpair`, passes one end
to bsm over its existing `SOCK_SEQPACKET` channel with `SCM_RIGHTS`, and hands the
other to the bridge as an inherited fd (`BSM_HOST_FD`, `CLOEXEC` off for that one
fd only). The connection identity is then the fd itself; only the bridge process
tree can hold it.

Minimal fix without redesign. Have the helper report the **absolute** cgroup path
of the scope (`my_cgroup` is already known at `session.rs:309-317`) so
`cgroup_matches` uses the `starts_with(scope + "/")` branch (`peer.rs:82-83`); that
path lives under `system.slice`/`user.slice` outside the user's delegation. Fail
closed in `verify_host` when `want.exe` is non-empty and the link is unreadable,
only if the helper (which can read it) attests the exe instead; otherwise mark the
exe row in `architecture.md:72` as "not effective across uids".

---

## F8 (Low): pid-recycle TOCTOU remains without `SO_PEERPIDFD`

`peer_of` falls back to the `SO_PEERCRED` pid when `SO_PEERPIDFD` is unavailable
(kernel older than 6.5, `peer.rs:22-33`), and `still_alive` then returns true
without checking (`peer.rs:64-65`). A connection can wait in `pending` for up to
10 s (`broker.rs:34`, `490-496`) before `BindHost` makes it verifiable. The kernel
frees the pid number when the peer is reaped even if the socket still holds the
`struct pid`. Sequence: a same-uid process connects early and parks in `pending`
(`broker.rs:329-337`), passes the connected fd to a second process (or keeps a
forked child), and exits; with a small `kernel.pid_max` the helper's bridge later
receives the same pid number; `verify_host` compares the cached number to
`host_pid` (equal), reads the *bridge's* `/proc/<pid>/cgroup` and `exe` (both
match), and promotes the attacker's socket. The real bridge is then refused (single
use). With a pidfd the final `still_alive()` (`peer.rs:148-150`) catches the dead
peer, so the path is closed on kernel >= 6.5. [INFERENCE: not reproduced; needs pid
wrap and kernel < 6.5.]

Minimal fix. In `peer_of`, when `peer_pidfd` returns `None`, mark the peer
unverifiable and have `verify_host` / `verify_greeter` reject it (fail closed).
The installed image kernel is far newer, so this costs nothing there.

---

## F9 (Low): pending-connection slots can be exhausted; the legitimate peer is dropped silently

Before `BindHost` / `RegisterGreeter`, at most `MAX_PENDING = 4` connections are
held and further ones are dropped without any event (`broker.rs:35`, `132-136`,
`334-336`). The session socket is created before the helper starts the bridge
(`bsm-proto` `RegisterSession` docs, `scene.rs:63-67`), so a same-uid process can
fill the four slots during login and the bridge's connection, if it arrives before
`BindHost`, is the one that is dropped. The login then fails by timeout. The greeter
slot has no uid check at accept (`broker.rs:120-137`), so any member of the
`bingux-greeter` group can do the same. Impact is limited to a self-inflicted login
failure for a same-uid attacker and to the greeter group; no confidentiality effect.
The manager correctly ignores `SessionConnectionRejected`
(`bsm-manager/src/core/tests/security.rs:408-411`), so another uid cannot fail a
login by connecting to its socket.

Minimal fix. Evict the oldest pending connection instead of dropping the newest, run
`check_uid` at greeter accept as `session_accept` does, and prefer the inherited
socketpair design from F7.

---

## F10 (Low): hosted surface liveness is never tracked

`CompositorHandler` has no `destroyed` override (`state.rs:274-299`). If the bridge
destroys the `wl_surface` it handed to `get_session_surface` but keeps the
`bsm_session_surface_v1`, the `HostedSurface` entry keeps `acked`, `valid` and
`presented_frames >= 1` (`host.rs:49-58`), so `session_presentable` stays true
(`state.rs:474-486`) and the engine can reveal and commit a session whose surface is
gone. Render then draws whatever the dead surface's retained state yields, or
nothing (cover colour). Nothing cross-session is exposed; the damage is a frozen or
blank frame inside the session's own slot, input focus set on a dead surface
(`input.rs:155-161`), and a `surface_index` entry that is only removed with the
session surface object (`host.rs:343-351`). [INFERENCE: what Smithay renders from a
destroyed surface was not exercised.]

Minimal fix. Add `fn destroyed(&mut self, surface: &WlSurface)` to `CompositorHandler`
that removes the matching `surface_index` entry and calls `drop_hosted_surface`, and
require `hs.surface.is_alive()` in `HostedSurface::ready`.

---

## F11 (Low): no per-client resource caps

Hosted surfaces are bounded by the output count (`host.rs:193-199`), which is good.
Everything else a hosted or greeter client creates is bounded only by the client's
own fd limit and memory:

- `bsm_session_host_v1` binds: each bind pushes to `rec.hosts` and replays all
  outputs (`host.rs:110-137`).
- `wl_surface`, `wl_region`, frame callbacks (never fired for surfaces that are not
  session surfaces), shm pools and buffers: Smithay validates parameters
  (`smithay-0.7.0/src/wayland/shm/handlers.rs:127-153`) but sets no quota.
- `linux-dmabuf` imports run synchronously on the compositor thread through
  `dmabuf_imported` (`state.rs:431-441`); each `create_immed` creates an EGLImage and
  texture in the compositor's GL context. [INFERENCE: cost and caching in the GLES
  renderer were not measured.]
- Any `wl_surface.commit` from any hosted surface, visible or not, sets
  `needs_redraw` (`state.rs:289`), so a hidden session can force a full render and
  flip every vblank.

Slow readers are handled well: a client whose output buffer fills is disconnected
(`wayland-backend .../client.rs:210-212`). Protocol errors are fatal for the
connection.

Minimal fix. Keep per-client counters in `ClientState` (surfaces, buffers, pools,
host binds, dmabuf params) and `kill_client` above a generous cap; only set
`needs_redraw` for a hosted commit when that session is in the current plan or is
being proven.

---

## F12 (Info): keys held at hand-off are listed in `wl_keyboard.enter`

`architecture.md` says the keystrokes that authenticated "are not delivered to the
revealed desktop". `set_owner_now` clears focus and refocuses; Smithay's focus
change calls `enter` with every key currently held (`smithay-0.7.0/src/wayland/seat/keyboard.rs:75-77`,
`serialize_pressed_keys`). If the last password key or Return is still down when the
owner changes (the grace is the transition time, normally well over a second), the
revealed session sees its keycode in `enter` and later a release without a press.
Keycodes only, no text. Fix by tracking and releasing held keys on owner change
(`kb.input` release events to the old owner, then clear), or by documenting it.

## F13 (Info): doc and code differences

- `architecture.md:72` and `bsm-proto/src/paths.rs:17-21` say the session socket has
  a "random name". The name is `<session>-<generation>.sock` (`broker.rs:230-233`),
  where the id is `s%08x` from the manager rng (`bsm-manager/src/core/mod.rs:276-278`,
  32 bits) and the generation is a counter. It is hard to guess, but only the uid
  and pid checks are authoritative.
- `bsm/tests/support/mod.rs:667` and the harness use socket mode `0o666` for the
  greeter socket. Test code only; production default is `0660` plus a chown to the
  greeter group (`config.rs:136-144` accepts any mode up to `0777`; a root-owned
  config could weaken it. Consider refusing `o+rw`).
- `peer.rs:24` reads `SO_PEERCRED` into rustix's `UCred { pid: Pid }`, a non-zero
  type. If the kernel ever reports pid 0 (peer outside the caller's pid namespace)
  that is a niche violation. bsm runs in the init namespace so this should not
  occur. Using `libc::getsockopt` into `libc::ucred` avoids the question.
- `docs/bsm/compositor.md` says the watchdog is "a 500 ms watchdog [that] releases
  it if a flip never arrives, so a broken output cannot freeze the state machine".
  That is the F1c behaviour and should be read as a known weakness.

---

## Controls that held

Things I tried to break and could not, with the evidence.

**Global filter bypass (hidden ids and versions).** `wayland-backend` checks
`can_view` again at bind time, not only when listing: `check_bind` rejects a
wrong interface name, a version above the global's, and `!can_view`
(`rs/server_impl/registry.rs:103-129`). Every global bsm creates has a gated
`can_view`: the `gated_global!` macro covers `wl_compositor`, `wl_subcompositor`,
`wl_shm`, `wl_seat`, `wl_output`, `xdg_output_manager`, `xdg_wm_base`,
`wp_viewporter`, `wp_fractional_scale_manager`, `wp_presentation`
(`state.rs:204-218`); layer-shell and dmabuf use Smithay's filter constructors
(`state.rs:724-726`, `gfx.rs:275-281`); `bsm_session_host_v1` has its own
`can_view` (`host.rs:139-141`). Late `wl_output` globals go through the gated impl
(`state.rs:536-556`). Unknown clients see nothing (`roles.rs:84-91`). A forged
`bind` of a hidden global is an error for the connection. I found no global that
bypasses the table.

**Impersonation of the greeter or another session.** Streams are inserted into the
display only after uid and pid verification (`broker.rs:139-161`, `340-356`), the
role is attached by bsm (`insert_role_client`, `broker.rs:106-116`), there is no
shared socket, a rejected peer receives no protocol byte, a session socket is
removed when its host is promoted (`broker.rs:372-380`) and another session's
`host_pid` does not match (`security.rs`, "a_session_socket_cannot_be_used_to_impersonate_another_session").
The role is re-read from the connection on every host request (`host.rs:166-175`).
Stale-generation requests are errors. The remaining weakness is F7/F8.

**TOCTOU in pid / cgroup / exe checks with a pidfd.** `Peer` holds the pidfd for the
whole pending window (`peer.rs:12-18`), the pid is read from the pidfd, and the
liveness check is last (`peer.rs:148-150`) so any exit before the `/proc` reads is
caught. Without a pidfd see F8.

**Socket file permissions.** `bsm.service` sets `UMask=0077` (`bsm.service:83`), so
`bind` creates `0700` and `bind_socket` chmods afterwards
(`broker.rs:67-69`); there is no window with a wider mode. The greeter socket goes
`0660` then `chown` to the greeter group (`broker.rs:68-73`), which fails closed if
the group lookup fails (`run.rs:359`). `/run/bsm/sessions` is `0711`
(`packaging/bsm/tmpfiles/bsm.conf:10`): session sockets are `0666` but cannot be
listed. The stale-socket removal (`broker.rs:60-66`) runs inside a directory only
`bsm` can write.

**fd leaks into children.** The compositor spawns no child process (searched
`Command`, `spawn`, `pre_exec` under `src/`). Its listeners and accepted streams are
Rust-created (`CLOEXEC`), the pidfd comes from `SO_PEERPIDFD` (the kernel returns it
close-on-exec [INFERENCE: kernel source not read]), DRM fds are opened with `CLOEXEC` (`gfx_drm.rs:361`). The one child bsm
starts, `bsm-imgconv`, closes everything outside an allow list in `pre_exec`
(`bsm-appearance/src/sandbox.rs:90-102`, `convert.rs:294-297`). Keymap fds are sealed
memfds (`smithay .../utils/sealed_file.rs`). Not verified: whether libseat's
seatd backend sets `CLOEXEC` on the device fds it receives.

**shm validation.** Smithay rejects negative offset, non-positive size, stride below
width, `stride * height` overflow, and an offset past the pool
(`wayland/shm/handlers.rs:127-153`); a truncated pool is handled by the SIGBUS guard
(`wayland/shm/pool.rs:44-47`, `105-107`, `139-141`) and bsm blocks only `SIGTERM` and
`SIGINT` (`run.rs:180-189`), so the guard is not defeated.

**Hosted buffer validity.** A hosted commit counts as a frame only when acked,
scale 1, no transform, exact configured size (`host.rs:355-392`). A mismatching
buffer is never imported for composition because `session_presentable` fails first
and the cover is drawn (`state.rs:474-486`, `render.rs:158-194`). `wp_viewporter` is
not offered to hosted clients.

**Cursor surfaces.** A cursor surface larger than 256 px is replaced before any
import (`render.rs:262-269`, `232-240`), and only the pointer-focused client can set
the cursor (`smithay .../wayland/seat/pointer.rs:379-381`, `524-525`; but see F2).
The cursor resets on every owner change (`input.rs:81`).

**Race between `Protect` and `SetInputOwner`.** Commands are handled in order on one
channel. `Protect` clears a queued owner for that session (`input.rs:89-92`);
`apply_pending_owner` re-checks `owner_allowed` at the frame boundary
(`input.rs:54-62`); and `foreground_session()` is `None` while the base is the cover or a
transition has not reached `Committed` (`engine.rs:208-217`); a lock-family
`begin` protects its own source (`engine.rs:349-357`). I found no
interleaving that gives a protected session the keyboard. The residual gaps are F1,
F2, F5, F6.

**Transition validation.** `validate` runs before the first frame and on every frame
(`engine.rs:268-292`, `440-453`); a vanished or protected target, a missing greeter
or incomplete outputs fail to cover plus greeter with every touched session
protected (`engine.rs:364-372`). A second `BeginTransition` fails without disturbing
the running one (`engine.rs:307-311`). Unlock clips the session to the vacated strip
(`render.rs:177-190`).

**Greeter crash cover.** `client_gone` clears the greeter slot and focus
(`broker.rs:409-424`); `greeter_presentable` then fails, the plan has no greeter
layer and the clear colour is the opaque cover (`engine.rs:417-433`); a running
transition that needs the greeter fails to `fail_safe` on its next plan; sessions
stay in `protected`. A reconnecting greeter must be re-registered with a new
generation (`broker.rs:416-420`). The only caveat is F1 (the cover is composed but
may not be acknowledged honestly) and F3.

**Cross-session pixel and input leaks.** The scene is keyed by `(session, generation)`,
only the plan's session is drawn (`render.rs:158-194`), no capture or screencopy
global exists, the frame-dump directory exists only on the headless backend
(`gfx.rs:171-177`; CLI only), `unregister_session` drops surfaces, evidence and any
owner (`broker.rs:382-405`), and input only reaches surfaces of the owner
(`input.rs:182-212`) apart from F2. No data-device global exists, so no clipboard
path. Frame callbacks and `presented` events tell a session only about its own
visibility.

**Protocol error handling.** Posting an error closes the client (`wayland-backend
.../client.rs:303-305`), so a hostile host cannot keep sending after an error. The
`panic!`s in `wayland-backend` for objects created without data cannot be reached
from `get_session_surface`: every early return follows a `post_error` (client
killed) and the success path calls `data_init.init` (`host.rs:182-236`). The
`expect`/`unwrap` calls in `host.rs`, `broker.rs` and `state.rs` are each preceded
by the check that makes them safe. Note `panic = "abort"` is not set for `bsm`; a
panic on the compositor (main) thread ends the process either way.

## Not confirmed / not done

- Nothing was executed. F1 to F12 need either a DRM VM run (F1a-F1e, F10) or a
  small harness test (F2, F4, F5, F6) to move from "confirmed by reading" to
  "reproduced".
- F3 starvation rate and F11 dmabuf cost were not measured.
- Behaviour with two real uids, cgroup delegation under `user@UID.service`, kernels
  older than 6.5, libseat fd flags and Smithay's handling of a destroyed hosted
  `wl_surface` were not exercised.
- The greeter control socket, the helper protocol, the manager state machine and
  `bsm-host` were outside this scope except where they feed the compositor.

## Suggested order of work

1. F1 (engine two-stage protect reply, honest render result, redraw flag, DPMS
   order) with a DRM-path test that interleaves `Protect` with an awaited flip.
2. F2 (one line) and F5 (one line) with harness tests.
3. F3 heartbeat tie-in, then F4 rate caps.
4. F7/F8/F9 together by moving to an inherited socketpair; failing that, absolute
   scope path plus fail-closed without pidfd.
5. F6, F10, F11, F12.
