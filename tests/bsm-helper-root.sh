#!/bin/sh
# Root-only checks of bsm-helper against the real system PAM stack, logind and
# systemd, with real accounts. Run as root inside the Bingux OS image (the QEMU
# live ISO); it refuses to run as anyone else. The unprivileged tests in
# packages/bsm-helper/tests cannot cover setuid, pam_systemd, scopes or
# pam_faillock, which is what this script is for.
#
# usage (in the guest, as root):
#   BSM_HELPER=/usr/libexec/bsm/bsm-helper \
#   BSM_PROBE=/usr/libexec/bsm/bsm-helper-probe \
#   sh tests/bsm-helper-root.sh
#
# Preconditions: bsm-helper, bsm-helper-probe, the PAM services bsm-login and
# bsm-unlock (packaging/bsm/pam), the units bsm-helper.socket and
# bsm-helper.service (packaging/bsm/systemd) and the sysusers file
# (packaging/bsm/sysusers) are installed. tests/bsm-helper-vm.py boots the
# image, stages exactly that and runs this script.
#
# It creates the accounts bsmt1 and bsmt2 (password "bsm-test-pw"), rewrites
# /etc/bsm/helper.toml for its run (the leader and bridge are `sleep`, the
# greeter is `sleep`) and removes the accounts and the file at the end.
# Output: one `PASS name` / `FAIL name: reason` / `INFO ...` line per check,
# and the raw `loginctl show-session` records. Exit status: 0 all passed, 1 a
# check failed, 77 skipped (not root).

[ "$(id -u)" = 0 ] || { echo "SKIP: this script needs root"; exit 77; }

HELPER=${BSM_HELPER:-/usr/libexec/bsm/bsm-helper}
PROBE=${BSM_PROBE:-/usr/libexec/bsm/bsm-helper-probe}
SOCK=/run/bsm-helper/helper.sock
PW=bsm-test-pw
WORK=/run/bsm-helper-test
FAILS=0

pass() { echo "PASS $1"; }
fail() { echo "FAIL $1: $2"; FAILS=$((FAILS + 1)); }
info() { echo "INFO $*"; }
check() { # name, command...; status 0 when the check passed
    name=$1
    shift
    if "$@" >"$WORK/last.out" 2>&1; then
        pass "$name"
    else
        fail "$name" "$(tr '\n' ' ' <"$WORK/last.out" | cut -c1-300)"
        return 1
    fi
}
eq() { # name, expected, actual
    if [ "$2" = "$3" ]; then pass "$1"; else fail "$1" "expected '$2', got '$3'"; fi
}
probe() { runuser -u bsm -- env BSM_PROBE_PASSWORD="${BSM_PROBE_PASSWORD:-$PW}" "$PROBE" --socket "$SOCK" "$@"; }
field() { # json-line-file key -> value (string or number)
    grep -o "\"$2\":\(\"[^\"]*\"\|[0-9-]*\|null\)" "$1" | head -n 1 | sed 's/^[^:]*://; s/^"//; s/"$//'
}
cleanup() {
    systemctl stop bsm-helper.service 2>/dev/null
    for s in $(loginctl list-sessions --no-legend 2>/dev/null | awk '{print $1}'); do
        case "$(loginctl show-session "$s" -p Name --value 2>/dev/null)" in bsmt1 | bsmt2) loginctl terminate-session "$s" ;; esac
    done
    pkill -u bsmt1 2>/dev/null
    pkill -u bsmt2 2>/dev/null
    userdel -r bsmt1 2>/dev/null
    userdel -r bsmt2 2>/dev/null
    rm -rf "$WORK" /run/bsm-greeter /run/bsm-helper/sessions
    [ -f /etc/bsm/helper.toml.bsm-test-orig ] && mv /etc/bsm/helper.toml.bsm-test-orig /etc/bsm/helper.toml || rm -f /etc/bsm/helper.toml
}
trap cleanup EXIT

mkdir -p "$WORK" /etc/bsm /run/bsm/sessions
chmod 1777 "$WORK"
[ -f /etc/bsm/helper.toml ] && cp /etc/bsm/helper.toml /etc/bsm/helper.toml.bsm-test-orig
systemd-sysusers 2>/dev/null
id bsm >/dev/null 2>&1 || { echo "FAIL precondition: account bsm missing (sysusers)"; exit 1; }
id bingux-greeter >/dev/null 2>&1 || { echo "FAIL precondition: account bingux-greeter missing (sysusers)"; exit 1; }
for u in bsmt1 bsmt2; do
    userdel -r "$u" 2>/dev/null
    useradd -m -s /bin/sh "$u" || { echo "FAIL precondition: cannot create $u"; exit 1; }
    echo "$u:$PW" | chpasswd
done
usermod -aG video bsmt1
U1=$(id -u bsmt1)
U2=$(id -u bsmt2)
mkdir -p /run/bsm/sessions
chown bsm:bsm /run/bsm /run/bsm/sessions

write_config() { # leader-command bridge-command [allowed_exe]
    cat >/etc/bsm/helper.toml <<EOF
# The probe stands in for bsm: it is admitted by executable. The unit check is
# disabled here because the probe does not run in bsm.service.
[socket]
allowed_exe = "${3:-$PROBE}"
allowed_cgroup = ""

[pam]
free_failures = 50

[session]
gnoblin = ["/bin/sh", "-c", "$1"]
bridge = ["/bin/sh", "-c", "$2"]
runtime_wait_secs = 40
start_timeout_secs = 60
default_grace_ms = 2000

[greeter]
command = ["/bin/sh", "-c", "setsid sleep 555 & exec sleep infinity"]
EOF
}
LEADER='echo $$ > '$WORK'/leader-$BSM_SESSION_ID.pid; setsid sleep 987 & exec sleep infinity'
BRIDGE='echo $$ > '$WORK'/bridge-$BSM_SESSION_ID.pid; exec sleep infinity'
write_config "$LEADER" "$BRIDGE"
# Images that ship bsm gate its units on `bingux.bsm` with drop-ins; this run boots
# the plain image (greetd keeps the display), so the helper units must not be gated.
rm -rf /etc/systemd/system/bsm-helper.socket.d /etc/systemd/system/bsm-helper.service.d \
    /usr/lib/systemd/system/bsm-helper.socket.d /usr/lib/systemd/system/bsm-helper.service.d
systemctl daemon-reload
systemctl stop bsm-helper.service 2>/dev/null
systemctl start bsm-helper.socket
check "unit: socket is active" systemctl is-active bsm-helper.socket
eq "unit: KillMode=process" "KillMode=process" "$(systemctl show bsm-helper.service -p KillMode)"
eq "socket: owner and mode" "bsm 600" "$(stat -c '%U %a' $SOCK)"

# ---- connection policy -----------------------------------------------------
if ! check "connect: the bsm account is served" probe ping; then
    echo "--- journal of the helper units ---"
    journalctl -u bsm-helper.service -u bsm-helper.socket --no-pager -n 40
    echo "--- end ---"
fi
check "connect: root is refused although it can open the socket" sh -c "! $PROBE --socket $SOCK ping"
check "connect: another account cannot open the socket" sh -c "! runuser -u bsmt2 -- $PROBE --socket $SOCK ping"
HPID=$(systemctl show bsm-helper.service -p MainPID --value)
check "helper: running as root" test "$(stat -c %U /proc/$HPID)" = root
info "helper pid $HPID: $(grep -E '^(Uid|CapEff|NoNewPrivs|Seccomp):' /proc/$HPID/status | tr '\n\t' '  ')"
eq "helper: no seccomp filter inherited by users (Seccomp=0)" "Seccomp: 0" "$(grep ^Seccomp: /proc/$HPID/status | tr '\t' ' ')"
eq "helper: no_new_privs off (setuid helpers work in desktops)" "NoNewPrivs: 0" "$(grep ^NoNewPrivs: /proc/$HPID/status | tr '\t' ' ')"

# Review F10: another process of the bsm uid must not be served, only bsm itself.
systemctl stop bsm-helper.service
write_config "$LEADER" "$BRIDGE" /usr/bin/bsm
check "connect: a bsm-uid process that is not /usr/bin/bsm is refused (F10)" sh -c "! runuser -u bsm -- $PROBE --socket $SOCK ping"
systemctl stop bsm-helper.service
write_config "$LEADER" "$BRIDGE"
check "connect: the configured executable is served again" probe ping

# ---- authentication and lockout policy ------------------------------------
BSM_PROBE_PASSWORD=wrong-password probe login bsmt1 "$U1" 1 >"$WORK/bad1.out" 2>&1
check "auth: a wrong password is refused" grep -q "authentication failed" "$WORK/bad1.out"
check "auth: the correct password is accepted by the real PAM stack" probe login bsmt1 "$U1" 2
if command -v faillock >/dev/null 2>&1 && grep -q pam_faillock /etc/pam.d/system-auth; then
    faillock --user bsmt1 --reset
    for i in 1 2 3 4 5 6; do BSM_PROBE_PASSWORD=wrong-password probe login bsmt1 "$U1" "1$i" >/dev/null 2>&1; done
    if probe login bsmt1 "$U1" 20 >"$WORK/locked.out" 2>&1; then
        fail "faillock: the distro lockout applies to bsm logins" "login succeeded after 6 failures"
    else
        pass "faillock: the distro lockout applies to bsm logins (pam_faillock refused the correct password)"
    fi
    faillock --user bsmt1 --reset
    check "faillock: reset makes the account usable again" probe login bsmt1 "$U1" 21
else
    info "pam_faillock is not part of system-auth in this image: lockout is not tested"
fi
check "auth: unlock stack accepts the password" probe login bsmt1 "$U1" 30 unlock
check "auth: a user without a password match under unlock is refused" sh -c "! BSM_PROBE_PASSWORD=nope $PROBE --socket $SOCK login bsmt1 $U1 31 unlock"

# ---- a real hosted session -------------------------------------------------
S1=s000000a1
G1=1001
probe open bsmt1 "$U1" $S1 $G1 /run/bsm/sessions/$S1-$G1.sock >"$WORK/open1.out" 2>"$WORK/open1.err"
check "session: OpenSession succeeds with the real stack (pam_systemd, user manager)" grep -q session_opened "$WORK/open1.out"
[ -s "$WORK/open1.err" ] && info "open1 stderr: $(cat "$WORK/open1.err")"
SCOPE=$(field "$WORK/open1.out" scope)
LSESS=$(field "$WORK/open1.out" logind_session)
LEAD=$(field "$WORK/open1.out" leader_pid)
HOST=$(field "$WORK/open1.out" host_pid)
info "SessionOpened: scope=$SCOPE logind_session=$LSESS leader=$LEAD host=$HOST"
check "session: a logind session id was reported" test -n "$LSESS" -a "$LSESS" != null
if [ -n "$LSESS" ] && [ "$LSESS" != null ]; then
    loginctl show-session "$LSESS" >"$WORK/session1.show" 2>&1
    echo "--- loginctl show-session $LSESS ---"
    cat "$WORK/session1.show"
    echo "--- end ---"
    eq "logind: Type=wayland" "Type=wayland" "$(grep ^Type= "$WORK/session1.show")"
    eq "logind: Class=user" "Class=user" "$(grep ^Class= "$WORK/session1.show")"
    # logind leaves an empty Seat out of `show-session`.
    check "logind: no seat (the Seat property is empty or absent)" sh -c "! grep -q '^Seat=.' $WORK/session1.show"
    eq "logind: Active=yes without a seat" "Active=yes" "$(grep ^Active= "$WORK/session1.show")"
    eq "logind: owned by the user" "Name=bsmt1" "$(grep ^Name= "$WORK/session1.show")"
    eq "logind: scope matches SessionOpened.scope" "Scope=$SCOPE" "$(grep ^Scope= "$WORK/session1.show")"
    eq "logind: scope is session-N.scope" "session-$LSESS.scope" "$SCOPE"
    eq "logind: Desktop=gnoblin" "Desktop=gnoblin" "$(grep ^Desktop= "$WORK/session1.show")"
    eq "logind: LockedHint starts as no" "LockedHint=no" "$(grep ^LockedHint= "$WORK/session1.show")"
    info "logind State: $(grep ^State= "$WORK/session1.show") Remote: $(grep ^Remote= "$WORK/session1.show") Service: $(grep ^Service= "$WORK/session1.show") Leader: $(grep ^Leader= "$WORK/session1.show")"
fi
check "scope: leader is in the session scope" grep -q "$SCOPE" /proc/"$LEAD"/cgroup
check "scope: bridge (host_pid) is in the session scope" grep -q "$SCOPE" /proc/"$HOST"/cgroup
SUP=$(awk '{print $4}' /proc/"$LEAD"/stat)
eq "scope: the supervisor is the leader's parent and named bsm-session" "bsm-session" "$(cat /proc/"$SUP"/comm)"
check "scope: the supervisor (PAM holder) is in the scope too" grep -q "$SCOPE" /proc/"$SUP"/cgroup
check "scope: the helper itself is NOT in the session scope" sh -c "! grep -q '$SCOPE' /proc/$HPID/cgroup"
for p in "$LEAD" "$HOST"; do
    eq "identity: pid $p runs as bsmt1 (real, effective, saved, fs)" "Uid:	$U1	$U1	$U1	$U1" "$(grep ^Uid: /proc/"$p"/status)"
    eq "identity: pid $p has no capabilities" "CapEff:	0000000000000000" "$(grep ^CapEff: /proc/"$p"/status)"
done
VIDEO=$(getent group video | cut -d: -f3)
check "identity: supplementary groups were set (video)" grep -q "^Groups:.*\b$VIDEO\b" /proc/"$LEAD"/status
check "environment: /run/user/uid exists" test -d /run/user/"$U1"
check "environment: the user manager runs" systemctl is-active user@"$U1".service
check "environment: the user bus socket exists" test -S /run/user/"$U1"/bus
LENV=$(tr '\0' '\n' </proc/"$LEAD"/environ)
BENV=$(tr '\0' '\n' </proc/"$HOST"/environ)
for v in XDG_SESSION_TYPE=wayland XDG_SESSION_CLASS=user XDG_SESSION_DESKTOP=gnoblin DESKTOP_SESSION=gnoblin "XDG_RUNTIME_DIR=/run/user/$U1" "USER=bsmt1" "LOGNAME=bsmt1" "HOME=/home/bsmt1" "BSM_SESSION_ID=$S1" "BSM_GENERATION=$G1" "XDG_SESSION_ID=$LSESS" "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$U1/bus"; do
    check "env(leader): $v" sh -c "printf '%s\n' '$LENV' | grep -qx '$v'"
    check "env(bridge): $v" sh -c "printf '%s\n' '$BENV' | grep -qx '$v'"
done
for v in XDG_SEAT XDG_VTNR WAYLAND_DISPLAY DISPLAY; do
    check "env: $v is not set (leader and bridge)" sh -c "! printf '%s\n%s\n' '$LENV' '$BENV' | grep -q '^$v='"
done
check "env: the brokered socket reaches the bridge" sh -c "printf '%s\n' '$BENV' | grep -qx 'BSM_HOST_SOCKET=/run/bsm/sessions/$S1-$G1.sock'"
check "env: the brokered socket does NOT reach the leader" sh -c "! printf '%s\n' '$LENV' | grep -q '^BSM_HOST_SOCKET='"
sleep 5
check "keyring: pam_gnome_keyring saw the password in auth and unlocked in session (F8, one handle)" sh -c "journalctl -t bsm-helper --no-pager | grep -q 'unlocked login keyring'"
check "pam: the session is still open five seconds after login (no premature pam_close_session)" sh -c "loginctl list-sessions --no-legend | awk '{print \$1}' | grep -qx '$LSESS'"
info "sessions after one login: $(loginctl list-sessions --no-legend | tr '\n' ';')"

# ---- what polkit makes of a seatless session (information, not a verdict) -----
# org.freedesktop.login1.reboot is allowed without authentication for an active,
# local session. pkcheck exit status: 0 authorised, 1 not authorised, 2 needs
# authentication, 3 no agent available. The bsm polkit rules are not installed by
# this script, so this is the distribution's own behaviour.
if command -v pkcheck >/dev/null 2>&1; then
    LIVE_LEADER=$(loginctl list-sessions --no-legend | awk '$4 == "seat0" {print $5; exit}')
    for entry in "$LEAD:hosted session of bsmt1 (no seat)" "$LIVE_LEADER:the live user's seat0 session"; do
        pid=${entry%%:*}
        [ -n "$pid" ] || continue
        pkcheck --action-id org.freedesktop.login1.reboot --process "$pid" >/dev/null 2>&1
        info "polkit: org.freedesktop.login1.reboot for ${entry#*:} (pid $pid): pkcheck exit $?"
    done
fi

# ---- LockedHint through logind --------------------------------------------
check "lock: SetLockedHint(true)" probe lock $S1 $G1 true
eq "lock: logind LockedHint=yes" "LockedHint=yes" "$(loginctl show-session "$LSESS" -p LockedHint)"
check "lock: SetLockedHint(false)" probe lock $S1 $G1 false
eq "lock: logind LockedHint=no" "LockedHint=no" "$(loginctl show-session "$LSESS" -p LockedHint)"

# ---- two users at once -----------------------------------------------------
S2=s000000a2
G2=1002
probe open bsmt2 "$U2" $S2 $G2 /run/bsm/sessions/$S2-$G2.sock >"$WORK/open2.out" 2>&1
check "second user: OpenSession" grep -q session_opened "$WORK/open2.out"
L2=$(loginctl list-sessions --no-legend | awk '$3 == "bsmt2" {print $1}' | head -n 1)
info "sessions with two users: $(loginctl list-sessions --no-legend | tr '\n' ';')"
check "second user: its logind session is Active too" sh -c "loginctl show-session $L2 -p Active | grep -qx Active=yes"
check "second user: a different scope" test "$(loginctl show-session "$L2" -p Scope --value)" != "$SCOPE"
LEAD2=$(field "$WORK/open2.out" leader_pid)

# ---- close only one session ------------------------------------------------
check "close: CloseSession of the first session" probe close $S1 $G1 1000
check "close: leader gone" sh -c "! kill -0 $LEAD 2>/dev/null"
check "close: bridge gone" sh -c "! kill -0 $HOST 2>/dev/null"
check "close: the whole scope was stopped, including a setsid background process" sh -c "! pgrep -u bsmt1 -f 'sleep 987'"
sleep 2
check "close: logind session of the first user is gone" sh -c "! loginctl list-sessions --no-legend | awk '{print \$1}' | grep -qx '$LSESS'"
check "close: the second user's session is untouched" kill -0 "$LEAD2"
check "close: the second user's logind session still exists" sh -c "loginctl list-sessions --no-legend | awk '{print \$1}' | grep -qx '$L2'"

# ---- helper restart and adoption ------------------------------------------
systemctl stop bsm-helper.service
sleep 1
check "adopt: the session survives a helper stop" kill -0 "$LEAD2"
check "adopt: its logind session survives" sh -c "loginctl list-sessions --no-legend | awk '{print \$1}' | grep -qx '$L2'"
check "adopt: a new helper starts on demand" probe ping
check "adopt: CloseSession works after the restart" probe close $S2 $G2 1000
check "adopt: the adopted session's leader is gone" sh -c "! kill -0 $LEAD2 2>/dev/null"

# ---- the leader dying ends the session -------------------------------------
S3=s000000a3
G3=1003
probe open bsmt1 "$U1" $S3 $G3 /run/bsm/sessions/$S3-$G3.sock >"$WORK/open3.out" 2>&1
L3=$(field "$WORK/open3.out" logind_session)
LEAD3=$(field "$WORK/open3.out" leader_pid)
HOST3=$(field "$WORK/open3.out" host_pid)
# A watcher must be connected when the leader dies, as bsm is; events are not queued for nobody.
probe wait-exit $S3 >"$WORK/wait3.out" 2>&1 &
WAIT3=$!
sleep 1
kill -9 "$LEAD3"
wait $WAIT3
check "leader exit: the helper reports SessionExited (status -9)" grep -q '"status":-9' "$WORK/wait3.out"
sleep 2
check "leader exit: the bridge was stopped with it" sh -c "! kill -0 $HOST3 2>/dev/null"
check "leader exit: the logind session was closed (pam_close_session ran)" sh -c "! loginctl list-sessions --no-legend | awk '{print \$1}' | grep -qx '$L3'"

# ---- bridge missing: login fails cleanly -----------------------------------
systemctl stop bsm-helper.service
write_config "$LEADER" "$BRIDGE"
sed -i 's|^bridge = .*|bridge = ["/nonexistent/bsm-host"]|' /etc/bsm/helper.toml
probe open bsmt1 "$U1" s000000a4 1004 /run/bsm/sessions/s000000a4-1004.sock >"$WORK/open4.out" 2>&1
info "open4: $(tr '\n' ' ' <"$WORK/open4.out" | cut -c1-400)"
check "failure: a missing bridge fails the login with a clear error" grep -q "bsm-host" "$WORK/open4.out"
sleep 2
# The user manager's own `manager` class session lingers for a moment by design.
check "failure: no user-class logind session is left behind" sh -c "! loginctl list-sessions --no-legend | awk '\$3 == \"bsmt1\" && \$6 == \"user\"' | grep -q ."
check "failure: no process of the failed login is left" sh -c "! pgrep -u bsmt1 -f 'sleep infinity'"
systemctl stop bsm-helper.service
write_config "$LEADER" "$BRIDGE"

# ---- a respawning process cannot outlive the session (F6) -------------------
systemctl stop bsm-helper.service
printf '#!/bin/sh\nwhile :; do sleep 100 & sleep 0.02; done\n' >"$WORK/respawn.sh"
chmod 755 "$WORK/respawn.sh"
write_config 'echo $$ > '$WORK'/leader-$BSM_SESSION_ID.pid; setsid '$WORK'/respawn.sh & exec sleep infinity' "$BRIDGE"
S5=s000000a5
probe open bsmt1 "$U1" $S5 1005 /run/bsm/sessions/$S5-1005.sock >"$WORK/open5.out" 2>&1
check "respawner: session opened" grep -q session_opened "$WORK/open5.out"
sleep 2
check "respawner: the user has many processes now" test "$(pgrep -u bsmt1 -c -f 'sleep 100')" -gt 5
probe close $S5 1005 300 >/dev/null 2>&1
sleep 1
check "respawner: nothing of the user's survived cgroup.kill, respawner included" sh -c "! pgrep -u bsmt1 -f 'sleep 100'"
check "respawner: no shell loop left either" sh -c "! pgrep -u bsmt1 -f 'while :'"
systemctl stop bsm-helper.service
write_config "$LEADER" "$BRIDGE"

# ---- user processes get no inherited descriptors (F5) -----------------------
S6=s000000a6
write_config 'ls -l /proc/$$/fd/ > '$WORK'/fds-$BSM_SESSION_ID.txt; exec sleep infinity' "$BRIDGE"
probe open bsmt1 "$U1" $S6 1006 /run/bsm/sessions/$S6-1006.sock >/dev/null 2>&1
sleep 1
check "fds: the leader has exactly 0, 1, 2 and they are /dev/null" sh -c "test \"\$(grep -c ' -> ' $WORK/fds-$S6.txt)\" = 3 && ! grep -v '/dev/null' $WORK/fds-$S6.txt | grep -q ' -> '"
probe close $S6 1006 300 >/dev/null 2>&1
systemctl stop bsm-helper.service
write_config "$LEADER" "$BRIDGE"

# ---- the greeter -----------------------------------------------------------
GG=2001
BSM_PROBE_HOLD=12 probe greeter $GG /run/bsm/greeter-wayland.sock /run/bsm/greeter-control.sock >"$WORK/greeter.out" 2>&1 &
GJOB=$!
sleep 4
GPID=$(grep -o '"pid":[0-9]*' "$WORK/greeter.out" | head -n 1 | cut -d: -f2)
check "greeter: it was spawned" test -n "$GPID"
if [ -n "$GPID" ]; then
    GU=$(id -u bingux-greeter)
    eq "greeter: runs as bingux-greeter (all ids)" "Uid:	$GU	$GU	$GU	$GU" "$(grep ^Uid: /proc/"$GPID"/status)"
    eq "greeter: no capabilities" "CapEff:	0000000000000000" "$(grep ^CapEff: /proc/"$GPID"/status)"
    GENV=$(tr '\0' '\n' </proc/"$GPID"/environ)
    check "greeter env: WAYLAND_DISPLAY is the greeter socket" sh -c "printf '%s\n' '$GENV' | grep -qx 'WAYLAND_DISPLAY=/run/bsm/greeter-wayland.sock'"
    check "greeter env: BSM_CONTROL_SOCKET" sh -c "printf '%s\n' '$GENV' | grep -qx 'BSM_CONTROL_SOCKET=/run/bsm/greeter-control.sock'"
    check "greeter env: XDG_RUNTIME_DIR is private" sh -c "printf '%s\n' '$GENV' | grep -qx 'XDG_RUNTIME_DIR=/run/bsm-greeter/$GG/rt'"
    check "greeter env: no session bus, no seat, no host socket" sh -c "! printf '%s\n' '$GENV' | grep -q '^\(DBUS_SESSION_BUS_ADDRESS\|XDG_SEAT\|BSM_HOST_SOCKET\|SSH_AUTH_SOCK\)='"
    eq "greeter: private dir owner and mode" "bingux-greeter 700" "$(stat -c '%U %a' /run/bsm-greeter/$GG/rt)"
    eq "greeter: home owner and mode" "bingux-greeter 700" "$(stat -c '%U %a' /run/bsm-greeter/$GG/home)"
    check "greeter: it is not in a logind session" sh -c "! grep -q 'session-' /proc/$GPID/cgroup"
    info "greeter groups: $(grep ^Groups: /proc/"$GPID"/status)"
fi
wait $GJOB
check "greeter: its setsid child died with it" sh -c "! pgrep -u bingux-greeter -f 'sleep 555'"
# Review F1: replacing a greeter leaves nothing of the old one and no root-chowned surprise.
BSM_PROBE_HOLD=2 probe greeter-replace 2101 2102 /run/bsm/greeter-wayland.sock /run/bsm/greeter-control.sock >"$WORK/greeter2.out" 2>&1
check "greeter: replacement reported the old exit and the new spawn" sh -c "grep -q greeter_exited $WORK/greeter2.out && grep -c greeter_spawned $WORK/greeter2.out | grep -qx 2"
check "greeter: nothing of either generation is left" sh -c "! pgrep -u bingux-greeter ."
check "greeter: no generation directory is left" sh -c "! ls /run/bsm-greeter/ | grep -q ."
check "greeter: it was killed on request" sh -c "grep -q greeter_exited $WORK/greeter.out"
check "greeter: its private directories are removed" sh -c "! test -e /run/bsm-greeter/$GG"
[ -n "$GPID" ] && check "greeter: the process is gone" sh -c "! kill -0 $GPID 2>/dev/null"

echo "--- journal of the helper units (tail) ---"
journalctl -u bsm-helper.service -u bsm-helper.socket --no-pager -n 60
echo "--- end ---"
echo "RESULT failures=$FAILS"
[ "$FAILS" = 0 ]
