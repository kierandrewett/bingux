# Secure session locking

Bingux supplies the lock-screen client; Gnoblin owns policy, timers, recovery,
and the decision to start it. The client uses Quickshell's `WlSessionLock`,
which implements `ext-session-lock-v1`. It is a real session-lock surface for
every Wayland output, not a layer-shell or regular window.

## Lifecycle

`gnoblin-lockd` starts `bingux-lock` with a fresh `GNOBLIN_LOCK_TOKEN`.
The client refuses to start without that token. It requests the Wayland lock
and creates a lock surface for each output. Only when Quickshell reports
`WlSessionLock.secure` does it call:

```
org.gnoblin.Lock /org/gnoblin/Lock ReportPresented(s token)
```

`secure` is emitted only after Quickshell receives the compositor's confirmation
that every output is covered. The current broker records this as diagnostic
prototype evidence only: a public session-bus method authenticated by a
same-user token is not a compositor-origin security boundary. Only a
compositor-origin state event may authorize suspend, `LockedHint`, or a claim
that the session is locked. A rejected lock, missing protocol, authentication
failure, or PAM error never reports presentation.

On authentication success the client requests `unlock_and_destroy` and stays
alive. Quickshell 0.2.1 does not expose `wl_display.sync`, so Bingux does not
call `ReportEnded`: its local unlock transition is not proof that the
compositor processed the request. Gnoblin must add a compositor-confirmed
completion callback before using that method or cleaning the client up. A
lock-client crash while locked must leave the compositor locked and blanked.
Gnoblin must provide a trusted replacement/recovery client before making the
broker generally available.

The packaged `bingux-lock.service` has no `[Install]` section and is not a
dependency of `bingux.target`. It is intentionally inactive until the
compositor-side gate and recovery path are enabled.

## Authentication

`LockScreen.qml` uses Quickshell's asynchronous `PamContext`, configured as
`bingux-lock`. It renders PAM's actual sequence of messages and accepts both
visible and hidden responses, so password changes, one-time codes and other
multi-prompt stacks work without assuming a single password prompt. Responses
are cleared immediately and never logged.

The Fedora RPM installs `/etc/pam.d/bingux-lock` as a `%config(noreplace)`
file containing `auth include system-auth`. This follows the administrator's
Fedora authselect policy. Administrators must test their selected password,
fingerprint, smart-card and failure-lock configuration before enabling it.
Other distributions must provide an equivalent dedicated
`bingux-lock` PAM service; copying GDM's PAM file is not a portable contract.

## Protocol requirements

The compositor must advertise `ext_session_lock_manager_v1`. The protocol
requires one fresh, configured lock surface per output and makes normal client
content unavailable once it emits `locked`. The client must not be replaced
with a layer-shell surface: that only draws above ordinary windows and does
not secure input or hide the underlying session.

Quickshell 0.2.1 provides the API used here. Quickshell 0.3.1 also contains
session-lock fixes for sleep, wake, DPMS and output changes, so upgrade before
enabling this on systems which can take that version.
