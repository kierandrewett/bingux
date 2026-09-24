# Secure session locking

Bingux supplies the lock-screen client; Gnoblin owns policy, timers, recovery,
and the decision to start it. The client uses Quickshell's `WlSessionLock`,
which implements `ext-session-lock-v1`. It is a real session-lock surface for
every Wayland output, not a layer-shell or regular window.

## Lifecycle

Gnoblin's native compositor launches the fixed packaged command
`/usr/bin/bingux-lock` and authorizes that exact process ID before requesting
the Wayland lock. The client has no broker token, D-Bus authority, or policy
channel. It requests the Wayland lock and creates a lock surface for each
output. `WlSessionLock.secure` becomes true only when Quickshell receives the
compositor's confirmation that every output is covered; Gnoblin's native
compositor state, not any client report, authorizes suspend, `LockedHint`, and
the claim that the session is locked.

Gnoblin may pass `GNOBLIN_LOCK_START_FD`, an inherited read-end of a startup
pipe. The launcher reads exactly one byte before it starts Quickshell; EOF or
a read error exits without connecting to Wayland. The pipe is only an ordering
barrier: the compositor admits the child PID before writing the byte. It is
not an authentication credential and is ignored for manual launches.

On authentication success the client requests `unlock_and_destroy` and stays
alive. Quickshell 0.2.1 does not expose `wl_display.sync`, so a local
`locked=false` is not proof that the compositor has processed the request and
does not cause process exit. Quickshell maps the protocol's compositor-sent
`finished` event to `secure=false`; an externally finished lock exits then.
The native compositor owns final cleanup of the PAM-unlock path. A lock-client
crash while locked must leave the compositor locked and blanked. Gnoblin must
provide a trusted replacement/recovery client before enabling general use.

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

## Appearance and timeout

The dedicated client reads `~/.config/bingux/lock-theme.json`. It accepts only
hex colours, a local absolute background-image path, and a 12-hour clock flag.
It never writes this file and does not read the normal Bingux settings store.

```json
{
    "background": "#111318",
    "surface": "#261f232b",
    "text": "#ffffff",
    "mutedText": "#c7cbd5",
    "accent": "#3584e4",
    "backgroundImage": "/home/me/Pictures/lock.jpg",
    "useTwelveHourClock": false
}
```

`IdleTimeoutSeconds` remains a manual Gnoblin lockd setting. It is inactive
until the compositor gate and broker policy cutover are enabled. Bingux does
not expose it in Settings yet because a visible control without an active
broker would be misleading. Once lockd owns live policy, Settings can bind a
real timeout control to that broker configuration.

## Protocol requirements

The compositor must advertise `ext_session_lock_manager_v1`. The protocol
requires one fresh, configured lock surface per output and makes normal client
content unavailable once it emits `locked`. The client must not be replaced
with a layer-shell surface: that only draws above ordinary windows and does
not secure input or hide the underlying session.

Quickshell 0.2.1 provides the API used here. Quickshell 0.3.1 also contains
session-lock fixes for sleep, wake, DPMS and output changes, so upgrade before
enabling this on systems which can take that version.
