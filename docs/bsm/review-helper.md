# Security review: bsm-helper, packaging/bsm, bsm-appearance

Scope: `bsm/helper` (root helper), `packaging/bsm` (units, sysusers, tmpfiles,
PAM, polkit, D-Bus policy) and `bsm/appearance` (imgconv sandbox, publisher,
accounts). Read-only review: no source was edited, no payload was run, no network call
was made. Status words: CONFIRMED means the code path was read end to end and the
weakness is visible in the source; UNCONFIRMED means the exploit needs a condition that
was not exercised (named in each entry). Nothing below was reproduced dynamically.

Reviewed on branch feat/bsm. Files read in full: helper `server.rs`, `auth.rs`,
`ticket.rs`, `session.rs`, `sys.rs`, `env.rs`, `greeter.rs`, `config.rs`, `main.rs`,
`ipc.rs`, `cgroup.rs`, `logind.rs`, `pam.rs` (FFI and conversation), all of
`packaging/bsm`, appearance `publish.rs`, `sys.rs`, `sandbox.rs`, `imgconv.rs`,
`convert.rs` (spawn side), `record.rs`, `accounts.rs` (file readers), plus the fd
validation in `bsm/manager/src/runtime/dbus.rs` and the ACL table in
`bsm/manager/src/core/dbus.rs` because they feed the publisher.

## Summary table

| ID  | Severity     | Status      | Where                                             | One line |
| --- | ------------ | ----------- | ------------------------------------------------- | -------- |
| F1  | Medium       | CONFIRMED (path), race UNCONFIRMED | greeter.rs:33-56, server.rs:805-814 | Root chown/chmod follows symlinks inside a directory the greeter account already owns |
| F2  | Medium       | CONFIRMED   | server.rs:307-326, sandbox.rs:159, bsm.service, convert.rs:267 | The image converter runs as the same uid that the helper, D-Bus and polkit trust |
| F3  | Low-Medium   | CONFIRMED   | auth.rs:103-111, server.rs:476-480                | PAM_USER is never compared with the account the session is opened for |
| F4  | Low-Medium   | CONFIRMED   | server.rs:201-244, session.rs:366-376, 513-519    | Adoption trusts any socket file in a directory whose mode nobody sets |
| F5  | Low          | CONFIRMED   | session.rs:156-179                                | Users inherit stdout/stderr (journal stream) and any non-CLOEXEC fd |
| F6  | Low          | CONFIRMED   | session.rs:461-509, 474-483                       | Session stop is a racy pid sweep; can be outrun; killpg on a reaped pid in dev mode |
| F7  | Low          | CONFIRMED   | server.rs:485-487, 555-567                        | No throttle, unbounded dying children, fail delay bypassed by supersede |
| F8  | Low          | CONFIRMED   | packaging/bsm/pam/bsm-login:26-30                 | include (not substack) lets a sufficient module skip pam_gnome_keyring |
| F9  | Low          | CONFIRMED   | polkit/50-bsm.rules:112-123                       | bsm uid gets YES on power actions for any process; hostedSession forks loginctl per check |
| F10 | Low          | CONFIRMED   | publish.rs:251-268, 333-335, sys.rs:69-85         | Converter output only magic-checked; world-readable cache; squatted uid dir is a permanent DoS |
| F11 | Info         | CONFIRMED   | bsm.service:59-83, D-Bus policy                   | Hardening gaps and information disclosure by design |

Controls that were checked and held are listed at the end ("Checked, no finding").

---

## F1 Medium: greeter runtime directory is chowned before its children are created (TOCTOU, root, follows symlinks)

Location: `bsm/helper/src/greeter.rs:33-37` (`private_dir`), `41-56`
(`prepare_dirs`), caller `bsm/helper/src/server.rs:805-814`.

Evidence. `private_dir` does `create_dir(path)`, then `chown(path, uid, gid)`, then
`set_permissions(path, 0o700)`. `std::os::unix::fs::chown` is `chown(2)`, which follows a
symlink in the final component, and `set_permissions` is `chmod(2)`, which also follows.
`prepare_dirs` first creates `<runtime_root>/<generation>` and hands it to the greeter
account, and only then creates `rt` and `home` inside it, as root. From the moment the
parent is chowned, the greeter uid can rename, delete and create entries in it.

Exploit sketch. Precondition: a process running as `bingux-greeter` is alive while the
helper prepares the next greeter. The greeter is the component the architecture treats as
least trusted (it renders QML and decodes images), so a compromised greeter is the
attacker here. `spawn_greeter` replaces the old greeter with `kill_group(old.pid, SIGKILL)`
and goes straight on to `remove_dirs` and `prepare_dirs`; it does not wait for the group to
die, and a descendant that called `setsid`/`setpgid` is not in the group at all, so it
survives. (The helper runs with `KillMode=process` and the greeter lives in the helper's
own cgroup, so nothing else cleans up.)

1. The surviving process watches `<runtime_root>` (mode 0755, listable) with inotify for the
   next generation directory (generations are monotonic and predictable).
2. When the helper creates `<gen>/rt`, the process (it owns `<gen>`) does
   `rmdir rt; ln -s /etc/shadow rt` (or any root-owned target) inside the window between
   `create_dir` and `chown`.
3. Root runs `chown(rt, greeter_uid, greeter_gid)` and `chmod(rt, 0700)` on the symlink
   target. The attacker now owns and can edit a root file. Root follows from there.

The race window is small but the attacker controls the loop and the inotify event arrives
before the helper's next syscall, so it should not be treated as theoretical. I did not
run it; hence "race UNCONFIRMED". The unsafe ordering itself is CONFIRMED.

Minimal fix (no new dependencies):

* Create `<gen>` root-owned mode 0700, create `rt` and `home` inside it (still root-owned,
  inside a directory no one else can enter), chown them with `fchownat(.., AT_SYMLINK_NOFOLLOW)`
  on an `O_DIRECTORY|O_NOFOLLOW` fd, and chown `<gen>` itself last.
* Before `prepare_dirs`, wait until the old greeter's cgroup (or at least its process
  group and session) is empty. Better: start the greeter in its own transient scope or
  service so `cgroup.kill` ends everything it ever forked.
* Keep `remove_dir_all`: the toolchain's version is symlink safe (fixed in Rust 1.58.1);
  pin that as a minimum.

---

## F2 Medium: the image converter runs as `bsm`, the one uid the helper, D-Bus name and polkit trust

Locations: `bsm/appearance/src/convert.rs:267-302` (spawn as the caller's uid,
inside `bsm.service`'s cgroup), `bsm/appearance/src/sandbox.rs:62-73,141-160`,
`bsm/helper/src/server.rs:307-326`, `packaging/bsm/polkit/50-bsm.rules:112-118`.

Evidence and exploit sketch. The decoder is the largest untrusted-input parser in the
privileged side. Its sandbox is good (rlimits, empty netns, no_new_privs, seccomp
allow list applied before the first decoded byte, `KillProcess` default, descriptors
closed). The weaknesses are about what a kernel or seccomp escape or a bug in an allowed
syscall would reach, because the converter has bsm's uid:

* Helper socket: `accept_connections` admits any peer with `uid == allowed_uid` and
  nothing else (server.rs:307-317), and permits exactly one connection
  (server.rs:320-326). A process with uid `bsm` that is not the compositor can connect
  whenever bsm's connection is down (bsm restarting, crashed, or killed by the next
  point) and then holds the single slot; the real bsm is refused ("second connection")
  and no login can happen until the squatter lets go. With the slot it can also issue
  `AuthStart`, `SpawnGreeter`, `KillGreeter`, `SetLockedHint` and `CloseSession`. It
  cannot mint a ticket without a valid password, so this is denial of service plus
  greeter control, not root.
* `tgkill` is in the seccomp allow list with no argument condition (sandbox.rs:159).
  `tgkill(tgid, tid, sig)` can address any thread group the uid may signal. A converter
  that has code execution can loop `tgkill(pid, pid, SIGKILL)` over pids and kill bsm,
  which trips `OnFailure=bsm-fallback.service` after three restarts
  (`StartLimitBurst=3`) and swaps the display manager for greetd. Needs prior code
  execution in the sandbox: UNCONFIRMED as an end to end exploit, CONFIRMED as an
  over-wide allow rule.
* polkit grants `YES` to subject.user === 'bsm' for power-off, reboot and suspend,
  including the `-multiple-sessions` variants (50-bsm.rules:113-118). Any process of that
  uid, including the converter, can reboot the machine under other users' sessions.
* `futex` and `madvise` are allowed without argument filters; they are the main
  remaining kernel surface.

Minimal fixes, in order of effort:

1. sandbox.rs: add `condition(0, Eq, getpid())` to the `SYS_tgkill` rule (the filter is built at
   run time, so the own pid is known), and restrict `futex` to the private wait/wake ops
   (`FUTEX_WAIT_PRIVATE`, `FUTEX_WAKE_PRIVATE`, their BITSET variants).
2. server.rs: after the uid check, also compare `/proc/<peer pid>/exe` (take the pid from
   SO_PEERCRED, or better SO_PEERPIDFD to avoid pid reuse) with `/usr/bin/bsm`, so the
   converter and any other helper process of the bsm uid are refused. Apply the same to
   the polkit rule only if a process-level check becomes available; otherwise 3.
3. Run `bsm-imgconv` under its own uid (a second sysusers account or
   `systemd-run --property=DynamicUser=yes` with the output fds passed in), so a
   sandbox escape lands outside the trust set. This also lets the converter be removed
   from `bsm.service`'s cgroup.

---

## F3 Low-Medium: PAM_USER is not re-checked after authentication

Location: `bsm/helper/src/auth.rs:103-111` (`run_conversation`),
`bsm/helper/src/server.rs:476-480,682-688`.

Evidence. The helper resolves the account itself (`getpwnam(user)`, uid must equal the
requested uid, `account_allowed`), calls `pam_start(service, pw.name)`, runs
`pam_authenticate` and `pam_acct_mgmt`, and on success mints a ticket for `pw.uid` and
later opens the session as `pw` (supervisor: `assume_identity(pw)`). It never reads
`PAM_USER` back with `pam_get_item`. Modules are allowed to change `PAM_USER` (sssd and
winbind canonicalise names, `pam_succeed_if`-style or site modules can map one login to
another).

Exploit sketch. Only on a machine whose stack maps or rewrites the user (not the shipped
stack): the password that authenticated "alice" yields a PAM handle whose later phases
(`pam_setcred`, `pam_open_session`, pam_systemd, pam_loginuid) act on the rewritten
user, while the helper starts the desktop as `alice`. The result is a logind session,
loginuid and keyring of one account with processes of another. Needs a site specific
mapping: UNCONFIRMED.

Fix: after `pam_acct_mgmt` succeeds, `pam_get_item(PAM_USER)`, `getpwnam` it and require
the same uid as `pw.uid`; otherwise return `Done { ok: false, code: PamFailure }`.
Re-check the same item after `pam_open_session` in `supervisor_main` and abort the
session if it changed.

---

## F4 Low-Medium: adoption trusts any listening socket in `<state_dir>/sessions`

Locations: `bsm/helper/src/server.rs:201-244` (`adopt_running_sessions`),
`bsm/helper/src/session.rs:366-376` (`create_dir_all` of the sessions
directory), `session.rs:513-519` (`connect_adopt`), `session.rs:447-457`
(`accept_adopter`), `packaging/bsm/systemd/bsm-helper.socket:14`.

Evidence. The supervisor refuses adopters whose uid is not its own euid
(session.rs:451), which is right. The helper side does not check who it is talking to:
`connect_adopt` connects to every `*.sock` in the directory as root and believes the
`Opened { session, generation, logind_session, .. }` greeting it gets back. The only
thing that makes the directory safe is that it is writable by root alone, and nothing
sets that. `/run/bsm-helper` is created by the socket unit with `DirectoryMode=0755`
(root); `sessions` under it is created by the supervisor with `create_dir_all` and
whatever umask the process had after `pam_open_session` (session.rs:367-369). It is
absent from `packaging/bsm/tmpfiles/bsm.conf`. With the usual 022 the mode is 0755 and
the design holds; with a PAM stack that leaves umask 0 (a stack-dependent surprise) the
directory is world-writable, any user can bind a socket there, and root then adopts it.

Impact if the directory is writable: the helper registers a fake session with an
attacker chosen id, generation and `logind_session`. `SetLockedHint` later runs as root
against that logind id (it is validated to alphanumerics only, logind.rs:38-40), so one
user could set or clear the locked hint of another user's logind session, and a fake
`SessionExited` can confuse the manager. The 0755 mode also lets every local user list
session ids and generations. UNCONFIRMED as an exploit (needs the wrong umask).

Fix:

* Add `d /run/bsm-helper/sessions 0700 root root -` to tmpfiles (and `d /run/bsm-helper
  0711 root root -`; 0711 suffices for bsm to reach `helper.sock`).
* In `adopt_running_sessions`, `lstat` each socket (owner 0, mode 0600, not a symlink) and
  read SO_PEERCRED from the connected fd (listener uid must be 0) before trusting the
  greeting.
* In the supervisor, create the directory with `mkdir` mode 0700 and `fchmod`, not
  `create_dir_all`.

---

## F5 Low: users inherit the helper's stdout/stderr and any non-CLOEXEC descriptor

Location: `bsm/helper/src/session.rs:156-179` (`spawn_as_user`, used for the
leader, bridge and greeter).

Evidence. The child resets signals, `setsid`, drops identity, sets umask and PDEATHSIG,
points fd 0 at /dev/null and `exec`s. It does not close descriptors above 2 and does not
touch 1 and 2. Everything the helper and supervisor own is CLOEXEC (socketpairs,
signalfd, the accepted connection, the inherited listener after `F_SETFD`), so today only
fds 1 and 2 leak: the journal stream of `bsm-helper.service`. Descriptors opened by PAM
modules in the supervisor (pam_gnome_keyring, pam_systemd helpers, third party modules)
are not under the helper's control and are inherited if the module forgot CLOEXEC.

Impact. Every process of every hosted user, and the greeter, writes into the helper's
journal stream. A user can emit lines that look like helper log lines (journald
attributes the stream to `bsm-helper.service` and the helper's identifier) and can
flood the unit's rate limit so real helper logs are dropped. The user never receives a
privileged descriptor today; the sweep is the guard against a future one.
Log forging attribution: UNCONFIRMED (journald behaviour not run).

Fix: in the child, after `assume_identity`, `dup2` /dev/null (or a per-session
`sd_journal_stream_fd` with a different identifier) onto 1 and 2, and
`close_range(3, ~0, 0)` except the exec-error pipe (already CLOEXEC); call
`sys::close_fds_except(&[])` which already exists.

---

## F6 Low: session teardown is a racy pid sweep and can be outrun

Location: `bsm/helper/src/session.rs:461-509` (`terminate`).

Evidence. `terminate` lists `cgroup.procs` of the scope, `kill`s each pid with SIGTERM,
waits the grace, then loops `list; SIGKILL each; sleep 50 ms` for at most three
seconds and gives up with "processes survived SIGKILL". Then `run` calls
`pam_close_session` and `pam_delete_credentials` regardless.

* A user process that forks continuously (a fork loop or a respawning daemon) can stay
  ahead of a sweep that sleeps 50 ms between listings, so the user's processes outlive
  the session close, lock and logout. After the supervisor exits nothing watches the
  scope. For logout of a user who is hostile to the machine this defeats the
  guarantee that "logout ends the session"; it is not a privilege escalation.
* Signalling pids from a text listing has the usual window in which a pid exits and
  is reused before `kill`. Impact is small because only pids listed from the
  scope a moment earlier are targeted.
* In the no-scope development configuration (`require_scope = false`) `signal` also
  calls `kill_group(ch.leader.pid, ..)` and `kill_group(ch.bridge.pid, ..)` after
  `ch.reap()` has marked the leader as exited (session.rs:478-482). A reaped leader's pid
  can be recycled and the supervisor, root, then signals an unrelated process group.
  Not reachable with the shipped `require_scope = true`.

Fix: kernel 5.14+ has `cgroup.kill`; write `1` to `<scope>/cgroup.kill` (atomic for
the whole subtree, no pid races, fork-proof) and wait for `cgroup.events populated 0`.
Keep the sweep only as a fallback. Guard the `kill_group` calls with
`status.is_none()`.

---

## F7 Low: helper-side authentication has no throttle and a supersede that skips PAM's failure delay

Location: `bsm/helper/src/server.rs:485-487` (supersede loop), `555-567`
(`abort_auth`), `559` (`Dying`), `packaging/bsm/pam/bsm-login`, `bsm-unlock`.

Evidence. A new `AuthStart` aborts the live conversation, but the old child is only
killed after 1 s (`Dying { kill_at: +1000 ms }`) and not counted anywhere, so an
unlimited number of root children can overlap. A caller that sends `AuthStart`,
`AuthResponse`, `AuthStart`, ... gets a guess per message pair with no delay: PAM's
`pam_faildelay` fires inside each child after the verdict, and the helper no longer
waits for that verdict. The helper itself counts and delays nothing (docs/bsm/helper.md
says so at line 451). Throttling is therefore entirely `pam_faillock` in the stack's
`system-auth`.

Facts about that stack: the image's `system-auth` has `pam_faillock` (VM run documented
at docs/bsm/helper.md:447-451). The build host's own `/etc/pam.d/system-auth`
(lines 6-10) has only `pam_faildelay delay=2000000` plus `pam_unix.so nullok` and no
`pam_faillock`; on that kind of machine `bsm-login` has no lockout at all, and `nullok`
accepts an empty password. The comments in `pam/bsm-login:21` and `pam/bsm-unlock:10`
assume lockout "from system-auth", which is only true of some distributions.

The caller here is bsm (a compromised or buggy manager, or the greeter through it),
which is why this is Low: bsm already sees the passwords. The same point bounds
resource use: each superseded child is a root process with a PAM handle.

Fix: refuse a second `AuthStart` with `Busy` instead of superseding, or count and cap
`dying` (for example 4) and refuse beyond that; add a per-uid failure counter in the
helper with an exponential delay before `AuthDone`; state in the packaging docs that
the stack must carry `pam_faillock` and check for it in `bsm-set-display-manager enable`.

---

## F8 Low: PAM `include` lets a `sufficient` module skip `pam_gnome_keyring` (keyring and fingerprint)

Location: `packaging/bsm/pam/bsm-login:26-27` (and the account and session includes at
29, 33), `packaging/bsm/pam/bsm-unlock`.

Evidence. `include` splices the lines of `system-auth` into the calling stack. A
`sufficient` success inside the included lines ends the whole `auth` stack, so the line
after the include (`auth optional pam_gnome_keyring.so`) is not executed. Linux-PAM's
`substack` exists for this reason (pam.conf(5): a `sufficient` or `[... done ...]` result in a
substack returns to the calling stack instead of ending it), and GDM's own stacks use
`substack`. This host's `system-auth` uses `auth sufficient pam_fprintd.so` and
`auth sufficient pam_unix.so nullok` (lines 8-9). With that style of `system-auth` the
keyring module never stashes the password in the auth phase, so
`pam_gnome_keyring auto_start` in the session phase cannot unlock the login keyring
(the keyring prompts later or stays locked). The documented proof (docs/bsm/helper.md:443-446)
was taken on the image, whose `system-auth` is not of that style; it does not cover
Fedora authselect style stacks.

This is a functional break with a security flavour: users end up typing the same
password into a second prompt, or secrets in the keyring are unavailable. It does not
weaken authentication.

Fix: `auth substack system-auth`, `account substack system-auth`, `session substack
system-auth` in `bsm-login`, and `substack` in `bsm-unlock`. Test on a host with a
`sufficient` style `system-auth`: UNCONFIRMED until run.

Other PAM observations (no finding): `pam_nologin` is in the account phase of both
services as intended (note that `/run/nologin`, created at shutdown, also blocks
unlock; acceptable, consistent with GDM). `pam_systemd` appears twice (system-auth
optional, then `required`); pam_systemd returns early when a session already exists on
the handle, and the helper insists on a scope in `after_open` (session.rs:310-316), so
a skipped first run is retried by the second. `pam_loginuid` precedes both. The helper
refuses `pam_confdir` unless it is root-owned and not group or world writable
(main.rs:70-81).

---

## F9 Low: polkit rule scope

Location: `packaging/bsm/polkit/50-bsm.rules:112-123` (rule), `97-110` (`hostedSession`).

Verified against the installed action files (this host): every action in
`HOSTED_ACTIONS` has `allow_active` set to `yes` (login1 power-off, reboot, suspend
and their `-multiple-sessions` variants at
`/usr/share/polkit-1/actions/org.freedesktop.login1.policy:168-188,201-221,267-286`;
udisks2 filesystem-mount, encrypted-unlock, eject-media, power-off-drive, loop-setup,
rescan in `org.freedesktop.UDisks2.policy`; the NetworkManager actions in
`org.freedesktop.NetworkManager.policy`). So the hosted rule does not grant anything a
normal active seat session would not already have; the comment's claim is accurate for
the installed policy versions. Seatless polkit behaviour (`subject.active` true for a
session without a seat) was not exercised here: UNCONFIRMED; the VM test is the evidence.

Residual issues:

* Any process in a hosted session gets the same grants (that matches an active seat
  session), including `login1.reboot-multiple-sessions`: a user can reboot the machine
  while other users are logged in without a password. This is the default of the
  installed `login1.policy` for active sessions, so not an over-grant by this rule.
  `set-wall-message` is implied by `power-off` and `reboot` (login1.policy:176,209),
  so hosted sessions and bsm can also post wall messages.
* Subject `bsm` gets YES for the power actions regardless of who in that uid asks; see
  F2 for what that means if the uid is compromised.
* `hostedSession` runs `polkit.spawn(["/usr/bin/loginctl", ...])` on every check. polkit
  evaluates rules on its authority thread; a user generating many authorisation
  requests (any desktop component does) serialises on forks of `loginctl`, and a slow
  logind stalls every polkit decision. No injection: `subject.session` is set by polkit
  from the kernel, not by the caller. Mitigate by caching per session id for a few
  seconds (`Service` and `Class` cannot change for a session) or by reading
  `/run/systemd/sessions/<id>` with `polkit.spawn` of `/usr/bin/cat` only when the first
  call succeeds.
* There is no rule for the greeter account, so the greeter's reboot goes through bsm
  (`[greeter_power]` policy in bsm) and then bsm's own YES. A compromised greeter cannot
  reach logind through polkit. Confirmed by reading the rule: no branch names it.

---

## F10 Low: appearance cache

Locations: `bsm/appearance/src/publish.rs:251-268`, `318-342`,
`bsm/appearance/src/sys.rs:68-85`, `packaging/bsm/share/tmpfiles-active.conf:9`.

* Converter output is trusted after a size and a magic check. `read_back` accepts up
  to `MAX_READBACK_BYTES` = 64 MiB (publish.rs:48) whatever the per-kind limit in
  `ConvertLimits` is, and the width and height written to `profile.json` come from the
  converter's own report (publish.rs:672-679). The files are then read by the greeter's
  Qt image loaders. This is defence in depth for the case F2 describes: a compromised
  converter could write a hostile PNG or JPEG straight into a directory the greeter
  trusts. Fix: pass `limits.max_output_bytes` to `read_back`, parse the PNG IHDR or
  JPEG SOF in the parent and require the dimensions to match the report and the
  documented maxima.
* Files are mode 0644 and directories 0755 (sys.rs:69-85, publish.rs:338), so every local user
  can read every account's avatar, wallpaper and `profile.json`. The design says so
  (greeter cache), and `show_personal_images = false` removes the files; the
  account name to uid mapping is also visible. Informational.
* `open_user_dir` accepts an existing `<cache>/<uid>` only if it is owned by the
  publisher (publish.rs:333-335). `tmpfiles-active.conf` takes the directory over only at
  `bsm.service` start; before that it is 1777 and persistent on disk. A hostile local
  user who created `<cache>/<victim uid>` (or `<cache>/<victim name>/`) while the
  directory was world-writable makes publishing for the victim fail with
  `UntrustedDirectory` for ever, and nothing removes the squatter even though bsm now
  owns the parent. Fix: when the entry is not ours and bsm owns the parent, `renameat` it
  to a quarantine name (or `unlinkat` with `AT_REMOVEDIR` after `O_NOFOLLOW` checks)
  and recreate it.
* Passed-in source fds: the D-Bus layer rejects non-regular, non-read-only and
  over-16-MiB descriptors (`bsm/manager/src/runtime/dbus.rs:403-427`) and the converter
  repeats `fstat` and the size check (imgconv.rs:118-128,188-195). A regular file on a
  user-mounted FUSE filesystem that never answers blocks the converter until the 30 s
  wall-clock limit (convert.rs:255), repeated for avatar and wallpaper under the
  per-uid flock; the job runs on its own thread so the compositor is not blocked.
  Bounded by `Timeouts::publish` (25 s) and the per-uid rate limit. Informational.

TOCTOU checks that held: all cache writes go through directory fds with `O_NOFOLLOW`,
`O_EXCL` temporary files, `fsync` and `renameat`; state is replaced the same way;
`open_beneath` refuses symlinks below the root and checks ownership from `fstat` of the
opened descriptors, not from a path; the AccountsService icon path is only accepted
below configured roots and with trusted owners.

---

## F11 Info: unit hardening, D-Bus policy, secrets

Units.

* `bsm-helper.service` sets no sandboxing on purpose and documents why (inherited by every
  desktop). Consequence to keep in mind: it has `PR_SET_DUMPABLE=0` from `main.rs:29`
  but no `LimitCORE`; `dumpable` survives `fork` and `setresuid` and is reset by `execve`
  of the user's program, which is the intended behaviour. No `UMask=` is set; the helper
  inherits the manager default and `spawn_as_user` resets 022 for users.
* `bsm.service` (lines 59-83) is well hardened. Not set: `ProtectKernelTunables`,
  `ProtectProc`/`ProcSubset` (bsm reads `/proc/<pid>/cgroup` of D-Bus callers, so
  `ProtectProc=default` is the right choice), `RestrictNamespaces` (the converter needs
  `user` and `net` for `unshare`; `RestrictNamespaces=user net` would still be an
  improvement), `DevicePolicy=closed` with `DeviceAllow` for DRM, input and render
  nodes, `SystemCallFilter` (not profiled, stated in the unit). `NoNewPrivileges=yes` with an
  empty bounding set is consistent with sysusers giving bsm membership of
  `bingux-greeter` for socket group chowns.
* `bsm-seatd.service` runs seatd as root with `-g bsm`: every member of group `bsm` can take
  DRM and input devices. Members are only the bsm account; any process of that uid has
  the same reach (F2). No `CapabilityBoundingSet` is set; seatd needs few capabilities,
  worth trimming after profiling (UNCONFIRMED which are required).
* `bsm.service:54` runs `systemd-tmpfiles --create` as root from `/usr/share/bsm`
  (root-owned); that is fine.

D-Bus policy (`packaging/bsm/dbus/org.bingux.Session1.conf:26-34`). Only uid `bsm` may own the
name; every account may send to it, so access control rests entirely on bsm's
per-method checks (core/dbus.rs:9-18, 37-53). That table was read and is consistent with
the policy. Read-only methods (`GetUsers`, `GetSessions`, `GetStatus`) disclose the
existence, names and session scopes of every account to every local uid including
`nobody`, by design (documented in the file header). Principal detection uses the
caller's cgroup and uid from the bus; it trusts that the bus resolved the pid
without a reuse race (UNCONFIRMED, depends on dbus-broker or dbus-daemon using a pidfd).

Secrets handling in the helper: request datagrams and PAM responses are zeroised
(server.rs:362-387, `AuthResponse` at 404-407, ipc.rs:73-110, pam.rs:157-203); auth
children are forked after `close_fds_except`; no secret reaches a log (decode errors
are logged by category only, server.rs:1092-1106). Residual, low: serde_json's
scratch buffer for strings that contain escapes is not zeroised (UNCONFIRMED, library
internal), no `mlock`, so a password can reach swap.

---

## Checked, no finding

* Ticket forgery and replay (`ticket.rs`, `server.rs:628-711`). 64-bit values come from
  `getrandom`, zero and collisions are retried, failure to read randomness discards the
  ticket (server.rs:628-643). A ticket is removed on first redemption, whether or not the
  checks pass, and it records uid, user, intent and generation. `open_session` redeems,
  then re-reads the passwd entry by uid and requires the same name
  (server.rs:682-684), so a ticket cannot open a session for another account.
  An Unlock ticket has no payload and is refused for sessions
  (server.rs:663-693). The table is capped at 64 and expires. Only the single
  connection (uid `bsm`) can redeem, so cross-uid redemption is not possible.
* Root to user transition (`sys.rs:408-431`, `session.rs:128-199`). `initgroups`,
  `setresgid`, `setresuid`, in that order, with a check of real, effective and saved
  ids and an attempt to regain root that must fail; this also clears permitted and
  effective capabilities because the uid becomes non-zero (no `SECBIT_KEEP_CAPS` is
  set anywhere). `setsid`, signals reset, PDEATHSIG set last. The environment is
  built explicitly and passed to `execve`, so nothing from the helper leaks in;
  `LD_*`, `XDG_SEAT`, `XDG_VTNR`, `BASH_ENV`, `ENV`, `WAYLAND_DISPLAY`, `DISPLAY` and the
  bridge socket variable are filtered from PAM and configuration (env.rs:23-32), and
  `HOME`, `USER`, `LOGNAME`, `SHELL`, `XDG_RUNTIME_DIR`, session variables are forced.
  The filter is a deny list, so `GCONV_PATH`, `LOCPATH`, `NLSPATH`, `GLIBC_TUNABLES` and
  `PATH` from PAM pass through; the sources are root-owned PAM files and no
  `user_readenv` is used in the shipped stacks, so nothing user-controlled reaches it.
  Rlimits and umask: limits set by `pam_limits` in the supervisor are inherited; the
  umask is fixed at 022 (the shipped stack has no `pam_umask`).
* PID reuse in the helper (`server.rs`). Reaping and signalling are on one thread and
  `child_exited` removes every record of a reaped pid; parked sessions are
  addressed by socket, not by pid; `CloseSession` addresses the supervisor by its
  socket (server.rs:416-423) and matches (session, generation), so the stale-generation
  case is refused.
* Greeter spawn (`server.rs:789-837`). Socket paths must be lexically inside
  `socket_root`, absolute, without `..`; the greeter account must exist and not be uid
  0; uid below `min_uid`, root and the service accounts cannot start sessions
  (`account_allowed`, server.rs:441-456). The greeter gets an explicit minimal
  environment (greeter.rs:81-114) and no PAM session. (The directory creation issue is F1.)
* Helper socket permissions. Unit: `SocketMode=0600`, `SocketUser=bsm`, `ListenSequentialPacket`;
  helper-created fallback socket is bound under umask 0177 then chmod (sys.rs:48-68).
  Inherited fds are only trusted when `LISTEN_PID` matches, the type is SEQPACKET, and the
  variables are removed afterwards (server.rs:122-145).
* Unit ordering. `bsm-fallback.service` takes no input; `bsm-set-display-manager` is
  root only, `set -eu`, and writes fixed paths.

## What was not verified

Nothing was executed. Not exercised: seatless polkit `subject.active`, journald
attribution of inherited stdout, `pam_keyinit force revoke` key ownership after the
uid change (the session keyring is created while the supervisor is root; confirm with
`keyctl show @s` as the user in `tests/bsm-helper-root.sh`), real D-Bus peer pid
reuse, behaviour of an `authselect` style `system-auth` with `bsm-login`, and every
race named UNCONFIRMED above.
