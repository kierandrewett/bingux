# Secure session locking

Bingux supplies one lock-screen client. Gnoblin exposes secure
`ext-session-lock-v1` support; it does not choose a locker or own idle policy.
Hyprlock and other conforming clients remain valid alternatives. Bingux uses
Quickshell's `WlSessionLock`, which creates a real session-lock surface for
every Wayland output, not a layer-shell or regular window.

## Lifecycle

`/usr/bin/bingux-lock` starts the independent Quickshell client. It has no
broker token, D-Bus authority, or policy channel. It requests the Wayland lock
and creates a lock surface for each output. `WlSessionLock.secure` becomes true
only when Quickshell receives the compositor's confirmation that every output
is covered.

On authentication success the client requests `unlock_and_destroy`, then queues
`wl_display.sync` on Quickshell's existing Wayland connection. The callback is
ordered after the unlock request, so it proves the compositor processed that
request before the client exits. A failed sync is never replaced with a timer:
the client remains alive because it lacks completion proof. Quickshell maps the
protocol's compositor-sent `finished` event to `secure=false`; an externally
finished lock exits then. A lock-client crash while locked must leave the
compositor locked and blanked.

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

### Optional hypridle integration

The example [hypridle-bingux.conf.example](hypridle-bingux.conf.example) uses
an `ext-idle-notify-v1` listener to start `bingux-lock` after idle time. It is
not installed, enabled, or merged into a user's hypridle configuration. Use it
only after confirming that the current compositor advertises both
`ext-idle-notify-v1` and `ext-session-lock-v1`; otherwise the timeout either
cannot fire or cannot lock securely.

The example is idle-only. Do not treat hypridle's `before_sleep_cmd` or
`inhibit_sleep=1` as proof a lock surface was presented: it only waits for the
command to spawn. `inhibit_sleep=3`, `on_lock_cmd`, and `on_unlock_cmd` depend
on Hyprland's private `hyprland-lock-notify-v1` protocol, so they cannot
provide a portable Gnoblin lock-before-suspend path. A future Bingux-owned
policy service must wait for a compositor-confirmed locked state before it
claims that guarantee.

The example starts `bingux-lock` directly. Its launcher execs Quickshell, so
`pgrep -x bingux-lock` would not reliably identify an existing lock client. If
an idle listener starts a second client while one lock is active, the
`ext-session-lock-v1` compositor rejects that request with `finished`; the
second client exits without affecting the active lock.

## Protocol requirements

The compositor must advertise `ext_session_lock_manager_v1`. The protocol
requires one fresh, configured lock surface per output and makes normal client
content unavailable once it emits `locked`. The client must not be replaced
with a layer-shell surface: that only draws above ordinary windows and does
not secure input or hide the underlying session.

Quickshell 0.2.1 provides the API used here. Quickshell 0.3.1 also contains
session-lock fixes for sleep, wake, DPMS and output changes, so upgrade before
enabling this on systems which can take that version.
