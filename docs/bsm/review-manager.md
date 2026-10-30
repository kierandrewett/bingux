# bsm session manager and API review

Read-only review of the session manager, its APIs and the bridge against spec
sections 6 to 9, 12 and 15. Static reading only: no code was built or run, no
payload was executed, nothing was sent to a bus. Every line number below is from the
tree at review time (`feat/bsm`). Anything not seen in code or a test is marked
**unconfirmed**.

Severity is the effect on a deployed multi-user seat, assuming the attacker is
the stated principal. "Local user" means an authenticated account with a hosted
session; "greeter" means code running as the `bingux-greeter` account (the
restricted, most exposed process, so its compromise is in scope).

## Summary

| #   | Severity | Finding                                                                                              |
| --- | -------- | ---------------------------------------------------------------------------------------------------- |
| F1  | Medium   | Any hosted session can cancel another user's login or kill their half-started session via a power or logout request |
| F2  | Medium   | A background session's logout occupies the operation slot and makes the foreground user's lock return `Busy` |
| F3  | Medium   | Failed-attempt cooldown is bypassed by cancelling after the answer was submitted                    |
| F4  | Medium   | Shipped polkit rule grants `*-multiple-sessions` power actions to every hosted user; greeter power ignores other sessions |
| F5  | Medium (unconfirmed) | polkit `CheckAuthorization` for another uid's bus name may be refused to the `bsm` account, so every session power request fails |
| F6  | Medium   | Scene event channel is unbounded and hosted-client requests are not rate limited                    |
| F7  | Low      | One shared bounded driver queue, no per-caller limit on D-Bus; call tasks busy-poll when it is full |
| F8  | Low (unconfirmed) | Greeter QML renders display names with the default (auto-detect rich text) format                  |
| F9  | Medium   | bsm restart recovery cannot reattach a bridge; sessions are dropped or closed, not resumed (spec 9, invariant 9) |
| F10 | Low      | D-Bus reads and broadcast signals are visible to every local account (spec 12 "filtered by caller authority") |
| F11 | Low      | Passwords are not zeroised in the manager; two Debug-deriving wire types carry them                  |
| F12 | Low      | Greeter control socket: unbounded pending connections and a verdict/state desync after the 5 s expiry |
| F13 | Low      | Bridge accepts reporter-declared required surfaces, so a same-uid process can weaken the reveal gate |
| F14 | Info     | Caller-to-session mapping trusts connection-time pid plus cgroup; only a same-uid caller can profit |
| F15 | Low      | Persisted registry is trusted without validation on load (needs the `bsm` uid to tamper)              |
| F16 | Low      | Helper socket peer is not verified; a failed send is reported as a lost helper even if it is not    |
| F17 | Low (unconfirmed) | Unlock and switch-back gate needs one frame ever, not a fresh frame, in the manager             |

No method exists on D-Bus or the control socket that unlocks a session, reveals a desktop or
accepts an authentication result. That holds: the method list in
`bsm/manager/src/runtime/dbus.rs:577-746` and `ClientMessage` in
`bsm/proto/src/control.rs:125-154` contain none, and the reveal is only reachable
through `try_reveal` (`core/ops.rs:572-607`) after `on_auth_done` (`core/auth.rs:298-349`).

Paths below are relative to `bsm/manager/src/` unless a package is named.

---

## F1 Medium: a hosted session can cancel another user's login in flight

**Where.** `core/ops.rs:282-289` (`interrupt_current`), `core/power.rs:69`,
`core/ops.rs:773` (`request_logout`), `core/ops.rs:292-309` (`cancel_immediate`),
`core/dbus.rs:159-173` (`dbus_power`), `core/auth.rs:205-207`.

**What.** `interrupt_current` cancels any operation that has not begun a transition,
whoever requested it. `request_power` calls it at `power.rs:69` **before** polkit
and the inhibitor check have said anything, and `dbus_power` accepts a
`Principal::Session` in any running state (a locked or background session
too). `cancel_immediate` for a `Reveal` operation fails the live PAM challenge
(`ops.rs:295-299`) or, once the password was accepted and the new session is
launching, calls `begin_cleanup(.., Stopping)` on the other user's half-started
session (`ops.rs:300-307`).

**Exploit sketch.** Alice is signed in and her session is `LockedBackground`
(Bob chose "switch user"). While Bob types his password at the greeter, a process
of Alice runs `bsmctl poweroff` in a loop (or one call at the right moment).
Each call creates a `Power` operation in `Authorising` and cancels Bob's challenge
(`Cancelled`) or tears down Bob's launching session. Until polkit answers (or the
check timeout, 15 s) `greeter_begin_auth` answers `Busy` (`auth.rs:205-207`).
A tight loop denies every other user the greeter for as long as it runs. A denied
power request does not remove Alice's session, so the loop is free.
`RequestLogout` of her own session does the same once.

**Minimal fix.** Do not let a session requester preempt an operation it does not
own. In `request_power` and `request_logout`: if `self.op` exists and
`op.requester != requester` and the requester is `Requester::Session(_)`, return
`Busy`. For power, additionally move `interrupt_current` into `power_checks_done`
(after polkit and inhibitors passed). Add a test next to
`a_running_lock_is_not_interrupted_and_a_launch_is_cancelled_by_power`
(`core/tests/security.rs:606`) with a session caller instead of the greeter.

## F2 Medium: a background logout makes the foreground user's lock fail

**Where.** `core/ops.rs:773-796` (`request_logout`, non-foreground branch sets
`op.committed = true`), `core/ops.rs:282-289` (`interrupt_current` returns `Busy`
for a committed operation), `core/ops.rs:375` (`request_lock`),
`core/types.rs:142-143` (`close_grace_ms` 5000, `close_timeout` 15 s).

**What.** Logging out a session that is not on screen is treated as already
committed and holds the single operation slot until the helper reports the
session's exit or `close_timeout + grace` (up to about 20 s) passes. In that window
`request_lock` for the **foreground** user returns `Busy` (`interrupt_current`
refuses a committed operation). The same holds for the idle auto lock and
`Super+L`.

**Exploit sketch.** Alice is `LockedBackground`, Bob is `Foreground`. Alice's
process calls `RequestLogout("")` and keeps her session's processes ignoring
SIGTERM until the SIGKILL grace (5 s) and close timeout run out. Bob walks away and
presses lock: `Busy`, screen stays unlocked. A security control is denied by
another account. Whether Bingux retries on `Busy` is **unconfirmed**; the shell is
not in this review's scope.

**Minimal fix.** A logout of a non-foreground session must not use the one
operation slot as a barrier. Either run it as a background cleanup (track it in
`await_exit` without `self.op`), or let `Lock`, `SwitchUser` and suspend preempt an
operation whose `await_exit` is set (nothing is on screen to protect from
interruption).

## F3 Medium: the failed-attempt cooldown does not count cancelled submissions

**Where.** `core/auth.rs:108-114` (`CancelAuth`), `core/auth.rs:359-370`
(`fail_challenge`), `core/auth.rs:317-321` (the only place `failures` is
incremented), `core/auth.rs:351-355` (timeout), `core/auth.rs:154-157`
(`select_user` mid-flight), `core/ops.rs:198-208` (`op_finish`).

**What.** `failures[uid]` grows only when the helper's `AuthDone{ok:false}` arrives for a
live challenge. A greeter that sends `auth_response` and then immediately
`cancel_auth` (or `select_user` for another account, or a power request) makes the
manager take the challenge (`fail_challenge`) and send `AuthAbort`; the helper's
verdict, if the PAM stack already had the answer, arrives for a challenge that is gone
and is logged as `stale or duplicate AuthDone ignored` (`auth.rs:307-310`) with no count.
`failure_cooldown_active` (`auth.rs:174-178`) then never trips.

**Exploit sketch.** A compromised greeter loops `begin_auth`, `auth_response(guess)`,
`cancel_auth`. The cooldown (5 failures per 30 s) is not applied; guesses are limited
only by PAM's own delay and any `pam_faillock` the packaging ships. Whether
`AuthAbort` stops the PAM check before it records a failure is the helper's behaviour
and is **unconfirmed**; the manager must not depend on it (spec 6 asks for limits
consistent with the distro policy; the cooldown the manager does ship is not enforced
here).

**Minimal fix.** Charge on submit and refund on success. In `greeter_auth_response`
increment `failures[uid]` when the answer is forwarded to the helper; in `on_auth_done`
with `ok == true` remove it (already done at `auth.rs:327`). A cancelled conversation
then keeps its charge. Keep the existing `code != Cancelled` exclusion only for
conversations that never received an answer.

## F4 Medium: polkit grants multi-session power to every hosted user; greeter power is not session-aware

**Where.** `packaging/bsm/polkit/50-bsm.rules:44-51` and `119-121`,
`core/power.rs:39-53` (`action_id`), `core/auth.rs:115-123`,
`bsm/core/src/config.rs:74-78` (`greeter_power` defaults all true).

**What.** `power.rs` correctly asks for `*-multiple-sessions` when another account has a
running session, because stock polkit requires an administrator for it.
`50-bsm.rules` then answers `YES` for `power-off-multiple-sessions`,
`reboot-multiple-sessions` and `suspend-multiple-sessions` for every subject whose
logind session was created by `bsm-login`. Net effect: a non-administrator in a hosted
session can power off or reboot the machine with other users signed in (their sessions
are locked and background); the gate the code builds is a no-op. Separately, the greeter
path does no polkit check and no other-session check (`auth.rs:115-123`): with a locked
session present, anyone at the keyboard can reboot or power off.

The doc claims "reboot and power off follow polkit" (`docs/bsm/architecture.md`, Power and
session actions). The shipped rule is what decides, and it is permissive.

**Exploit sketch.** Bob, a standard user, runs `bsmctl poweroff` while Alice's locked
session holds unsaved work. polkit says yes, logind powers off. Alternatively a visitor at
the greeter chooses "Power off".

**Minimal fix.** Remove the three `*-multiple-sessions` entries from `HOSTED_ACTIONS`
so they keep their stock administrator-authentication default (which `polkit_check`
treats as refusal, `runtime/system.rs:306-308`). For the greeter, default `greeter_power`
to `reboot = false, poweroff = false` when a running session exists (or require the same
`-multiple-sessions` check via the `bsm` account). Both are packaging or config, apart
from the session-aware guard at `auth.rs:115`.

## F5 Medium (unconfirmed): polkit refuses `CheckAuthorization` for another uid's subject

**Where.** `runtime/system.rs:293-309`, `packaging/bsm/polkit/50-bsm.rules:112-118`.

**What.** `polkit_check` is called by the `bsm` account for the caller's bus name. polkitd
(from memory of its source; **not verified here**) refuses `CheckAuthorization`
for a subject of a different user unless the caller is uid 0 or is authorised for
`org.freedesktop.policykit.read`. The rules file grants `bsm` only the six power actions.
If polkitd behaves so, every session-originated power request fails with a
polkit error, which `runtime/driver.rs:373-378` turns into `authorised = false` (fail
closed, silent). `runtime/system.rs:28-36` already says this layer was not run against a
real polkit.

**Verify.** On the VM: call `CheckAuthorization` as `bsm` for a user's unique bus name, or
run `bsmctl reboot` from a hosted session and read the polkit journal.

**Minimal fix (if confirmed).** Add `org.freedesktop.policykit.read` to the `BSM_ACCOUNT`
branch of `50-bsm.rules`, or perform the check through a root helper request.

## F6 Medium: unbounded scene event channel fed by hosted-client requests

**Where.** `bsm/core/src/run.rs:285` (`std::sync::mpsc::channel()`),
`bsm/compositor/src/state.rs:508-512` (`emit` ignores back pressure),
`bsm/compositor/src/host.rs:237-292` (every valid `report` emits a
`SessionReport`; no deduplication or rate limit), `runtime/driver.rs:144-152` (forwarder
blocks on the bounded 1024 queue), `core/mod.rs:662-750` (`settle` rebuilds and compares
the whole snapshot after every input).

**What.** The only bounded stage is the driver queue; the compositor-to-forwarder
channel is unbounded, and the manager does non-trivial work per event. The hosted bridge
(or any code that controls it) can send `report(gnoblin_ready)` requests as fast as the
Wayland socket accepts them; each becomes a queued event and a full `settle`. Memory
grows with the backlog; D-Bus, greeter and helper events share the same queue.

**Exploit sketch.** A user whose bridge is compromised, or who modifies its behaviour
(the bridge runs as that user; whether `ptrace_scope` prevents tampering is
**unconfirmed**), floods `report`. The system compositor and manager, one process for
every user, grow until the OOM killer acts.

**Minimal fix.** Use `sync_channel` with `try_send` and a drop-and-count policy for
non-critical events on the compositor side, and make `report` idempotent in the
compositor: ignore a `report` identical to the last accepted one for that session and
kind. A hosted client that sends more than N protocol messages per second is
disconnected (the `bsm_session_host_v1` objects are the only requests it can make).

## F7 Low: one shared queue, no per-caller limit on D-Bus

**Where.** `runtime/driver.rs:33,53-55` (one `sync_channel(1024)` for all sources),
`runtime/dbus.rs:536-548` (a full queue is polled every 5 ms for up to 5 s per call),
`packaging/bsm/dbus/org.bingux.Session1.conf:30-34`.

**What.** Any local account may call every method (the bus policy allows it; bsm
decides). Calls are cheap to make and each waits in a task that wakes every 5 ms
when the queue is full; nothing limits concurrent calls per uid or per connection.
The compositor's `Protected` acknowledgement, `AuthDone` and greeter lines queue
behind them. A delayed `Protected` trips `on_protect_timeout` (`core/ops.rs:456-468`)
and fails the lock operation (the session stays hidden, fail closed).

**Minimal fix.** Prioritise: give system sources (scene, helper, greeter) their own
channel read first. Cap in-flight D-Bus calls per uid (for example 8) in
`Service::run` and answer `Busy` beyond that instead of polling.

## F8 Low (unconfirmed): display names in the greeter are rendered with auto-detected rich text

**Where.** `bsm/appearance/src/accounts.rs:319-324` (control characters removed,
`<`, `>` and `&` kept), `shell/bingux/auth/AuthCard.qml:98`,
`shell/bingux/auth/AccountPicker.qml:193` (`Text { text: ... displayName }`), and no
`textFormat` anywhere under `shell/bingux/auth` or `shell/bingux/greeter` (grep).

**What.** Qt's default `Text.AutoText` renders a string that looks like HTML as rich text.
A display name or `RealName` such as `<img src="http://host/x">` or `<b>Administrator</b>`
can spoof the label or make the greeter fetch a URL. Who can set the value (chfn policy,
AccountsService) is distro policy; **unconfirmed** whether a standard user can.

**Minimal fix.** Set `textFormat: Text.PlainText` on every `Text` that shows
account-, hostname- or PAM-supplied strings (`displayName`, `statusText`, `hostname`,
`promptText`) in `shell/bingux/auth` and `shell/bingux/greeter`.

## F9 Medium: bsm restart recovery cannot reattach the bridge (spec 9, invariant 9)

**Where.** `core/session.rs:183-196` (recovered branch of `on_socket_ready`),
`bsm/host/src/bridge.rs:1169-1172` and `257-263` (the bridge exits when the
connection goes), `bsm/proto/src/helper.rs:38-95` (no request tells a running
bridge a new socket path), `runtime/recovery.rs:18-20`, `core/mod.rs:325-336`.

**What.** The recovered path asks the compositor to bind and protect (`BindHost`,
`Protect`) and waits 30 s for the bridge. Nothing sends the freshly bound
socket path to the old bridge, and the old bridge exits `BsmLost` as soon as bsm goes
away. The recovery probe (`is_alive`) therefore finds the bridge pid gone and drops the
entry (`core/mod.rs:327-329`) without a `CloseSession`; the user's Gnoblin and apps
(still in their scope) are no longer in any registry. The test
`bsm_restart_brings_sessions_back_protected_never_foreground`
(`core/tests/security.rs:622-657`) simulates the reconnect with `bridges.insert`, so it
proves the manager half only. `docs/bsm/manager.md` already marks the bridge reconnect
"unverified"; the code evidence says it cannot work as designed.

Safety holds: nothing is revealed, everything stays hidden. The defect is that the
spec's "reconcile ... brings them back protected" is not met, and an orphaned session
may coexist with a new login of the same uid. **Unconfirmed:** what bsm-helper does with
its own session table after losing its client.

**Minimal fix.** Either (a) add a `HelperRequest::RebindSession{session, generation,
socket_path}` that makes the helper hand the path to the live bridge, and teach the bridge
to reconnect instead of exiting on `Closed`, or (b) state in the docs that a bsm restart
ends all sessions, and on recovery send `CloseSession` for a persisted entry whose bridge
is gone, so no orphan survives.

## F10 Low: reads and signals are not filtered by caller authority

**Where.** `core/dbus.rs:78-100` (every `Get*` for every principal),
`runtime/dbus.rs:930-990` (signals sent to the bus without a destination),
`packaging/bsm/dbus/org.bingux.Session1.conf:32-33` (`receive_sender` allowed for everyone).

**What.** Any local account, including service accounts such as `nobody`, can learn
which users are signed in, which sessions are locked, who is foreground and launch-failure
messages (`SessionLaunchProgress`). Spec 12 asks for subscriptions "filtered by caller
authority" and says not to leak private UI state; the content here is names, ids and
state, not window titles. The design is documented ("same as `loginctl list-sessions`",
`docs/bsm/manager.md:281`), so this is a spec deviation, not an accident.

**Minimal fix.** Return `Get*` data for `Principal::User` callers restricted to their own
uid; address signals per recipient or drop failure text from broadcast signals.

## F11 Low: secrets in manager memory; wire types derive Debug

**Where.** `core/auth.rs:262-278` (`response: String` moves through `ClientMessage` to
`HelperRequest`), `runtime/helper_client.rs:115` (serialised buffer not zeroised),
`runtime/greeter_server.rs:208-215` (the line buffer and `Vec` copies are not zeroised),
`bsm/proto/src/control.rs:123` and `bsm/proto/src/helper.rs:36` (both
derive `Debug`).

**What.** The helper zeroises (`bsm/helper/src/ipc.rs:77-108`); the manager does
not. The password lives in freed heap of a process that also owns the display. Redaction
exists only in the hand-written `Debug` of `Input` and `Effect` (`core/types.rs:467-528`).
A future `{:?}` of a bare `ClientMessage` or `HelperRequest` logs it. I found no such call
today: `greeter_server.rs:218` logs the kind only.

**Minimal fix.** `Zeroizing<String>` for `response` end to end, zeroise the serialisation
buffer after `send`, and replace the derives with redacting `Debug` impls on the two wire
types so the guard does not depend on the wrapper.

## F12 Low: greeter control socket accept path

**Where.** `runtime/greeter_server.rs:117-124` (every accepted connection is stored and a
`GreeterPeer` input queued), `:137-138` (pending connections expire after 5 s),
`:143-151` (`verdict` returns silently for an unknown id), `core/auth.rs:51-64`.

**What.** (1) No cap on pending connections: a compromised greeter (the socket is
group-accessible) can open descriptors until the process limit within the 5 s window,
starving the compositor (same process) of descriptors; each refused connection also logs a
warning. (2) If the manager's verdict arrives after the 5 s expiry, `verdict(accept=true)`
finds nothing and starts no reader, but the manager has already set
`greeter.control = Some(conn)`; no `GreeterDisconnected` ever follows, and every later
connect is refused (`auth.rs:55`) until the greeter dies. The queue delay needed (more
than 5 s) is not reachable today; it is a latent desync.

**Minimal fix.** Cap `pending` (for example 4) and refuse beyond it at accept; make
`verdict` return whether it found the connection and, if not, feed
`Input::GreeterDisconnected{conn}` back to the driver.

## F13 Low: reveal-gate required surfaces are declared by the reporter

**Where.** `bsm/host/src/readiness.rs:213-221` (`launched` takes `required` from
the line), `:254-271` (`required_components`/`required_roles`), `:203-211`,
`bsm/host/src/ready_sock.rs:21` and `bridge.rs:377-419` (16 connections, no idle
limit).

**What.** `architecture.md` says a required surface is the wallpaper, bar and dock named in
the current Bingux configuration. The bridge instead takes the list from the first
`launched` line, and `component: "all"` from any connection replaces the wallpaper and
desktop components. Another process of the same uid can therefore shrink the required set
(`{"component":"all","event":"launched","required":["wallpaper"]}`), map a fake layer
surface in the session's own Gnoblin, and cause a reveal of an incomplete desktop. No
privilege boundary is crossed (same uid, own session; the compositor still requires
acknowledged, presented frames: `core/session.rs:440-449`), which the architecture already
accepts, but the documented rule is not what is implemented. Sixteen idle connections also
starve the real reporters (the launch then times out).

**Minimal fix.** Pass the required roles to the bridge from its own configuration (a file
or argument written by the helper), ignore `required` on the wire, and close a connection
idle for more than a few seconds.

## F14 Info: caller to session mapping

**Where.** `runtime/dbus.rs:497-517` (`resolve_caller`), `core/dbus.rs:37-53`
(`principal`), `core/types.rs:583-596` (`cgroup_in_scope`).

The uid and pid come from the bus (`GetConnectionCredentials`), never from arguments, and
the unique bus name goes to polkit, so polkit's subject cannot be swapped. A caller
is a session only if its cgroup is in the scope **and** `s.uid == caller.uid`, so an
attacker cannot name another uid's session. Residual, all same uid: the pid is the
connection creator's at connect time while the cgroup is read afterwards (pid reuse, or a
connection passed to an out-of-scope process); the unit-name form matches any path
component, so a same-uid `systemd-run --user --scope --unit=<scope name>` would map (the
name is derivable from `GetSessions`; unconfirmed that the helper's scope name is a pure
function of the id). Nothing here crosses uids. Hardening: read `/proc/<pid>/cgroup` and
`stat(/proc/<pid>)` uid together and require the owner to equal `caller.uid`; match on the
full path the helper reports, not on a component.

## F15 Low: persisted registry is trusted

**Where.** `runtime/persist.rs:19-34`, `runtime/recovery.rs:18-20`, `core/session.rs:134-171`.

`state.json` is mode 0600 and owned by `bsm`; tampering needs that uid, which already
owns the compositor. `load` still accepts any `id` string (later used by the compositor
to name `/run/bsm/sessions/<id>-<generation>.sock`, **unconfirmed** at the compositor
call site), any `scope`, and any `uid` without checking that the bridge pid belongs to that
uid or that the id matches `[A-Za-z0-9_-]{1,64}` (the rule `session_arg` applies to D-Bus
input, `runtime/dbus.rs:388-396`). Add that check and a `stat` of the `/proc/<host_pid>`
owner.

## F16 Low: helper socket and send failure

**Where.** `runtime/helper_client.rs:55-57,112-125`, `runtime/driver.rs:299-304`.

The client trusts anything that accepts on the helper path: `AuthDone`, tickets and session
events. It does not check `SO_PEERCRED` uid 0 after connect; the safety rests on the
directory permissions of `/run/bsm-helper` (packaging). A failed `send` (the 2 s write
timeout) is reported as `HelperLost` (`driver.rs:303`) while the connection may still be
up, so the manager fails the challenge and launches and forgets a session the helper may
open afterwards (`SessionOpened` for a failed launch is then ignored:
`core/session.rs:263-269`); the orphan has no `CloseSession` because the helper may not
receive one either (unconfirmed). Hardening: verify the peer uid on connect; on send
failure close and reconnect before reporting a loss.

## F17 Low (unconfirmed): unlock reveal gate is not fresh

**Where.** `core/session.rs:670-674` (`surfaces_ready`), `core/ops.rs:572-607`,
`bsm/compositor/src/host.rs:377-378,449`.

The doc comment on `try_reveal` says "fresh presented frames"; the check is `acked &&
presented_frames >= 1`, a count that resets only on an output reconfigure. For an
existing locked session a hung Gnoblin still passes. The compositor re-validates the
scene at the transition (`host.rs:54-58`), so it is **unconfirmed** whether a stale desktop
can reach the slide-up. Minimal fix: record the count at `Protect` and require more at
`try_reveal`, as the new-login gate already does for `DesktopReady`.

---

## Reviewed and sound

* **Authorisation table** in `core/dbus.rs` matches `docs/bsm/manager.md:272-279`
  except that `SelectGreeterUser` also admits the greeter (the `bsmctl` header comment,
  `bsm/core/src/ctl.rs:7-8`, says root only; the code and `manager.md` say root or
  greeter). Acting on another uid is refused: `own_session_action`
  (`core/dbus.rs:133-157`) requires the target to equal the caller's own session; root is
  the only exception.
* **Auth challenge binding**: single use (`auth.rs:312`), bound to uid, plan, generation,
  operation and a timer; a late or duplicate `AuthDone` is ignored; a mid-flight
  selection change cancels (`auth.rs:154-157`); a stale selection cannot retarget
  (`auth.rs:198-200`); `still_ok` (`auth.rs:329-341`) rechecks the world. A response
  needs an open prompt (`auth.rs:267-270`) and is capped at 4096 bytes.
* **State machine**: all setters go through `session_legal`, `phase_legal`,
  `transition_legal` (`core/tables.rs`); a compositor event cannot commit an operation
  that did not begin (`ops.rs:638-642`); `commit_op` revalidates the session and
  generation (`ops.rs:704-719`); `check_invariants` runs after every input in debug builds.
  Besides F1 and F2 I found no sequence of lock, switch and power inputs that reaches
  `Foreground` for an unauthorised session or two foreground sessions.
* **Readiness**: stale generations are ignored on both sides (compositor
  `host.rs:242-248`, manager `session.rs:328`); `DesktopReady` counts only after the
  surfaces stage and requires strictly later frames (`session.rs:357-369,440-449`);
  `OutputsConfigured` needs a compositor-observed acknowledgement, not the bridge's word.
* **Control socket framing**: lines over 64 KiB disconnect (`greeter_server.rs:236-239`),
  partial lines are buffered, malformed JSON is counted and ends at ten, `Hello` is
  required first (`auth.rs:99-102`), one connection, peer uid and pid checked
  (`auth.rs:51-64`), writer queue bounded with disconnect on overflow. The greeter can
  select, authenticate, ask for power and cancel its own operation; it cannot name a
  session, an intent the manager does not re-derive, or cancel another principal's
  operation (`ops.rs:330-332`).
* **ready.sock**: directory 0700, socket 0600, same-uid peer check, 4 KiB lines, parse
  failure disconnects (`ready_sock.rs:28-125`).
* **Input forwarding**: key, button, pointer, touch and scroll are forwarded only while
  `input_active`, and `release_all_input` runs on deactivation (`bridge.rs:1010-1078`,
  `1229-1317`). A key pressed before activation is not delivered (only its release).
* **Persistence**: atomic write, mode 0600, no secrets; a damaged file starts empty
  (`persist.rs:19-52`). Recovered sessions come back `LockedBackground` and protected,
  never foreground (`session.rs:134-171`).
* **PublishAppearance**: uid from the bus, descriptors validated (regular, read-only, not
  `O_PATH`, 16 MiB) and duplicated, rate limited per uid (`core/dbus.rs:175-202`).
  Note: the 16 worker threads (`driver.rs:32`) are shared with polkit and power jobs;
  a stalled read on a user-owned FUSE file (**unconfirmed**) can hold one.

## Not reviewed

`bsm-helper` internals beyond the abort handler, `bsm-compositor` role enforcement beyond
the cited lines, `bsm-appearance`'s image converter, the QML screens beyond the two named
files and the text-format grep, and the packaged PAM stacks. Findings that touch them (F3,
F5, F6, F9, F17) say so.

## Suggested order

F1, F2 (cross-user denial, a few lines each), F4 (packaging), F3 (one line plus a test),
F6, then F9 (decide resume or document that restart ends sessions), then the Low items.
