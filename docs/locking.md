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

On authentication success the screen plays its exit (controls fade, the
wallpaper pushes in) and only then calls `finish()`, which requests
`unlock_and_destroy` and queues `wl_display.sync` on Quickshell's existing
Wayland connection. The callback is ordered after the unlock request, so it
proves the compositor processed that request before the client exits. A
failed sync is never replaced with a timer: the client remains alive because
it lacks completion proof. Quickshell maps the protocol's compositor-sent
`finished` event to `secure=false`; an externally finished lock exits then. A
lock-client crash while locked must leave the compositor locked and blanked.

The packaged `bingux-lock.service` has no `[Install]` section and is not a
dependency of `bingux.target`. `bingux-statusd`, which already holds the
Gnoblin compositor socket, subscribes to `gnoblin.session.lock-requested` and
answers it with `systemctl --user start --no-block bingux-lock.service`. That
makes `gnoblinctl session lock`, `org.freedesktop.ScreenSaver.Lock` and
`loginctl lock-session` show the lock screen. Starting a running unit does
nothing, so repeated requests do not stack lock clients. Without `bingux.target`
running there is no subscriber, and `gnoblinctl session lock` fails with
`no session-lock client is subscribed to lock requests`.

## Authentication

`LockAuthentication.qml` uses Quickshell's asynchronous `PamContext`,
configured as `bingux-lock`. It renders PAM's actual sequence of messages and
accepts both visible and hidden responses, so password changes, one-time codes
and other multi-prompt stacks work without assuming a single password prompt.
Responses are cleared immediately and never logged. When fprintd reports an
enrolled finger, a second, fingerprint-only conversation (`bingux-lock-fingerprint`)
runs beside the password one; either method unlocks.

The Fedora RPM installs `/etc/pam.d/bingux-lock` as a `%config(noreplace)`
file containing `auth include system-auth`. This follows the administrator's
Fedora authselect policy. Administrators must test their selected password,
fingerprint, smart-card and failure-lock configuration before enabling it.
Other distributions must provide an equivalent dedicated
`bingux-lock` PAM service; copying GDM's PAM file is not a portable contract.

## Appearance

The lock screen and the login greeter share `SessionScreen.qml`. There is no
separate lock theme file. The screen shows:

- the user's wallpaper, resolved by `bingux-session wallpaper` from the Bingux
  configuration (`desktop.wallpaper`, then the GNOME background setting). With
  no wallpaper the backdrop is a deep tint of the system accent colour;
- the date and clock at the top, in the locale's 12- or 24-hour format;
- the account picture (or a monogram), display name, a hint or status line,
  and a frosted password field at the bottom;
- the session bar at the top (next section).

Surfaces are translucent white with a blur of the wallpaper behind them. On
software rendering (`BINGUX_SOFTWARE_RENDERING=1`) blur and shadows are off and
the surfaces are plain translucent tints. Motion follows `BINGUX_REDUCED_MOTION`.

### Session bar

The bar at the top of the lock screen is built from the same widget table as
the desktop bar (`shell/bingux/DesktopLayout.js`), in the user's saved
`top-right` order, filtered by each widget's `contexts`. Only these survive on
the lock screen:

| Widget     | Lock screen                                                                 |
| ---------- | --------------------------------------------------------------------------- |
| `keyboard` | Current layout; the menu switches layouts through `gnoblinctl input select` |
| `controls` | Network and battery status; a menu with volume, Sleep, Restart, Shut Down   |

Search, tray, notifications, recording, monitors and extensions never appear.
Menus are drawn inside the lock surface itself: lock surfaces cannot parent
popup windows. `tests/lock-ui.py` opens the keyboard layout menu from the lock bar
and checks that it maps on the lock surface; `BINGUX_LOCK_POPUP_SHOT` saves that
frame.

`SessionServices.allows(capability)` gates every action a widget could take.
On the lock screen only `volume` (output volume and microphone mute) and `power` are granted; on the greeter only
`power`. Launching applications, search, tray menus, settings, notification
content and media control are refused with a `[session]` log line. The
contract test `tests/session-context.test.py` checks that every action path
consults the gate and that every widget declares its contexts.

### Volume keys and feedback

Gnoblin's session lock isolates shortcut paths, so the `wpctl` key bindings that
work on the desktop do not run while the screen is locked. The lock screen
handles these keys itself, as ordinary key presses on the lock surface:

| Key                    | Effect                                                   |
| ---------------------- | -------------------------------------------------------- |
| `XF86AudioRaiseVolume` | Raise the default output by 5 percent, up to 100 percent |
| `XF86AudioLowerVolume` | Lower the default output by 5 percent                    |
| `XF86AudioMute`        | Mute or unmute the default output                        |
| `XF86AudioMicMute`     | Mute or unmute the default input                         |

Raising or lowering the volume also unmutes the output, as the volume slider in
the session bar menu does. The lock screen stops at 100 percent. The desktop
key binding has no such limit.

A key press only changes the audio device. The card then comes from the same
record that `bingux-statusd` publishes for the desktop OSD (see
[desktop-shell.md](desktop-shell.md)), so a change made with the slider in the
session bar menu shows the card too. The card is centred on each output, between
the clock and the password field, in the lock screen's own dark glass. A key that
changes nothing, such as volume up at 100 percent, shows no card.

Both need the `volume` capability, which the lock screen grants. There is no
brightness feedback: Gnoblin binds no brightness keys and nothing reads the
backlight. The greeter handles none of these keys.

### Transitions

The clock and the account slide in on the first frame. After a successful
unlock the controls fade, the identity shrinks slightly, the dim overlay lifts
and the wallpaper pushes in over `Theme.sessionExitMotion` (360 ms), then the
client sends `unlock_and_destroy`. The desktop that appears underneath shows
the same wallpaper, so the hand-over reads as the shell returning rather than
a cut. `tests/lock-ui.py` drives this path inside a headless Sway session. It
also types the volume, mute and microphone keys on a virtual keyboard
(`wtype`), checks that they reach the lock surface, and checks that the card
shows a record sent on a stand-in for `osd-v2.sock`.

## Idle timeout

Bingux does not own idle policy. Gnoblin's `gnoblin-idle` service asks the
subscribed shell client to lock (`gnoblinctl session lock`) on the configured
idle timeout and on logind's `Lock` signal, and refuses both in greeter
sessions. Bingux answering that request with `bingux-lock` is still open; see
`TODO.md`. The hypridle example below is for other compositors.

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
