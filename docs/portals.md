# Portals

Bingux runs its own XDG desktop portal backend, `xdg-desktop-portal-bingux`.
It owns the permission rules for the portal interfaces that Bingux serves.
Gnoblin does not check permissions. Read the
[backend README](../packages/bingux-portal/README.md) for the identity rules and the
Screenshot decision table.

## What the backend serves

The backend serves one interface: `org.freedesktop.impl.portal.Screenshot`.
It answers on the bus name `org.freedesktop.impl.portal.desktop.bingux`.

| Path                                                                            | Purpose                                      |
| ------------------------------------------------------------------------------- | -------------------------------------------- |
| `/usr/libexec/bingux/xdg-desktop-portal-bingux`                                 | Backend executable.                          |
| `/usr/share/xdg-desktop-portal/portals/bingux.portal`                           | Routing descriptor for `xdg-desktop-portal`. |
| `/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.bingux.service` | D-Bus activation.                            |
| `/usr/lib/systemd/user/xdg-desktop-portal-bingux.service`                       | Systemd user unit.                           |
| `$XDG_CONFIG_HOME/bingux/portal-permissions.toml`                               | Permission store, read on each request.      |

The `bingux` package installs these files and depends on `xdg-desktop-portal`.

## Routing

`xdg-desktop-portal` picks a backend for each interface from its routing
configuration. Gnoblin writes that configuration from `gnoblin.configure.portals`.
The Bingux session file sets this route for Screenshot:

```lua
gnoblin.configure {
    portals = {
        default = {"gnoblin", "*"},
        interfaces = {
            ["org.freedesktop.impl.portal.Screenshot"] = {"bingux", "gnoblin", "*"},
        },
    },
}
```

The route tries `bingux` first. If Bingux is not installed, it tries `gnoblin`.
If that is not installed either, `*` selects the first installed backend in
name order. Other interfaces use `gnoblin`, then `*`.

How your own config combines with this route:

The Gnoblin docs (`docs/config/configure/portals.md`) say that an existing
per-user `gnoblin-portals.conf` or generic `portals.conf` takes precedence.
Gnoblin leaves that file alone and reports a conflict. It does not write the
route. The portal service then uses your file, so the Bingux Screenshot route
does not apply. Remove the file to use the Gnoblin-generated route.

When no Bingux backend is installed, the route falls to `gnoblin`, then to `*`.
`*` selects the first installed backend in name order, which may be an
unrelated backend. This doc does not test that case.

## Permission store

The store is `$XDG_CONFIG_HOME/bingux/portal-permissions.toml`, or
`~/.config/bingux/portal-permissions.toml` when `XDG_CONFIG_HOME` is unset.
A missing file means no rules and a default of `inherit`, which resolves to `ask`.

The file has a top-level `version` and `default`, then any number of `[[rule]]`
tables:

- `version` must be `1`.
- `default` is `inherit`, `ask` or `deny`. `allow` is refused at the top level.
- Each rule has a unique `name`, at least one selector, one or more
  `capabilities` and a `level`.

### Capabilities

| Capability       | Enforced today | Notes                                              |
| ---------------- | -------------- | -------------------------------------------------- |
| `screenshot`     | Yes            | The only interface the backend serves.             |
| `screen-cast`    | No             | Accepted by the store. No interface checks it yet. |
| `remote-desktop` | No             | Accepted by the store. No interface checks it yet. |
| `input-capture`  | No             | Accepted by the store. No interface checks it yet. |
| `access`         | No             | Accepted by the store. No interface checks it yet. |

`monitors` applies only to `screen-cast` and `remote-desktop`. `devices` and
`clipboard` apply only to `remote-desktop`. The store refuses other
combinations.

### Levels

| Level     | Meaning                                                                                         |
| --------- | ----------------------------------------------------------------------------------------------- |
| `inherit` | Use the store `default`. With no default, this means `ask`.                                     |
| `ask`     | Show the interactive selector.                                                                  |
| `allow`   | Allow without a prompt. Screenshot shows the selector only when the request sets `interactive`. |
| `deny`    | Refuse the request.                                                                             |

### Selectors

A rule needs at least one selector. Use the one that fits the requester:

- `app_id` is an exact match on the application ID the frontend passes.
- `exe` is an absolute path to a host program. The rule value is not resolved
  through symlinks. It must equal the canonical path of the requester's
  executable exactly. A request that has an application ID has no executable
  identity, so an `exe` rule never matches it.
- `pattern` is a glob over the full identity string. `*` matches any run of
  characters and `?` matches one character. Identity strings look like
  `app-id:org.example.App` or `host-exe:/usr/bin/tool`.

### Precedence

Rules are checked in file order:

1. Any matching `deny` wins, whatever comes later in the file.
2. Otherwise the last matching rule decides. A rule with `inherit` falls back
   to the store default.
3. When no rule matches, the store default applies.

### Full example

The rules below cover each capability and level. They are a template, not a
recommendation.

```toml
version = 1
default = "ask"

# Screenshot from a sandboxed viewer needs no prompt.
[[rule]]
name = "viewer-screenshot"
app_id = "org.example.Viewer"
capabilities = ["screenshot"]
level = "allow"

# A host tool may not start screen-cast on DP-1.
[[rule]]
name = "tool-screen-cast"
exe = "/usr/bin/tool"
capabilities = ["screen-cast"]
level = "deny"
monitors = ["DP-1"]

# Every example.* app asks before remote control, with keyboard and pointer.
[[rule]]
name = "example-remote-desktop"
pattern = "app-id:org.example.*"
capabilities = ["remote-desktop"]
level = "ask"
devices = ["keyboard", "pointer"]
clipboard = true

# Input capture from any host program in /usr/bin uses the store default.
[[rule]]
name = "host-input-capture"
pattern = "host-exe:/usr/bin/*"
capabilities = ["input-capture"]
level = "inherit"

# File-access requests from one app are allowed.
[[rule]]
name = "notes-access"
app_id = "org.example.Notes"
capabilities = ["access"]
level = "allow"
```

With this file, `org.example.Viewer` gets Screenshot without a prompt. Any
Screenshot request from `/usr/bin/tool` falls to the `default` of `ask`, because
`tool-screen-cast` covers only `screen-cast`. Any `remote-desktop` request from
an `org.example.*` app shows the selector.

The store must stay valid. A parse or validation error denies every portal
request until the file is fixed. Gnoblin does not validate this file.

## Which identity an app has

The backend derives one identity for each request:

- `app-id:<id>` when the frontend passes an application ID.
- Otherwise `host-exe:<path>`, where `<path>` is the canonical path of the
  requesting process's executable (`/proc/<pid>/exe`).

The backend prints no identity, and no command shows one yet. To check a host
program, find the process that made the request and run
`readlink -f /proc/<pid>/exe`. For a sandboxed app, check the application ID
the frontend passes. That path is not tested on a live session.

## Failure modes

These results come from the backend code and its integration test.

| Situation                                         | Response | Outcome                                              |
| ------------------------------------------------- | -------- | ---------------------------------------------------- |
| Screenshot saved                                  | 0        | The response carries a `file://` URI.                |
| User cancels the selector                         | 1        | No file is written.                                  |
| Shell reports an error                            | 2        | The shell message is logged.                         |
| Shell is busy with another capture                | 2        | Refused at once.                                     |
| Shell `binguxctl` is missing                      | 2        | Refused promptly.                                    |
| Rule level is `deny`                              | 2        | Refused without calling `binguxctl`.                 |
| No identity, or caller is not the frontend        | 2        | Refused.                                             |
| Request handle is malformed or names no requester | 2        | Refused.                                             |
| Requester runs as another user                    | 2        | Refused.                                             |
| Store is invalid                                  | 2        | Every request is refused and stderr names the error. |
| Shell result polling fails 25 times in a row      | 2        | The capture is cancelled.                            |
| `Close` on the request handle                     | (none)   | The running capture is cancelled.                    |

## Interface known state

The table lists the 20 interfaces that Gnoblin's portal declaration names. It
shows which package declares each one in the ISO. A declaration does not prove
that a backend implements the interface.

| Interface       | Declared in the ISO by                   | Bingux backend | Tested                 |
| --------------- | ---------------------------------------- | -------------- | ---------------------- |
| Screenshot      | `bingux`                                 | Served         | Stand-in frontend only |
| Access          | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| Account         | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| AppChooser      | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| Background      | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |
| Clipboard       | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |
| DynamicLauncher | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| Email           | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| FileChooser     | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| GlobalShortcuts | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |
| InputCapture    | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |
| Inhibit         | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| Lockdown        | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |
| Notification    | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| Print           | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| RemoteDesktop   | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |
| ScreenCast      | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |
| Settings        | `xdg-desktop-portal-gtk` (`UseIn=gnome`) | No             | No                     |
| Usb             | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |
| Wallpaper       | None (only `xdg-desktop-portal-gnoblin`) | No             | No                     |

Notes:

- `xdg-desktop-portal-gtk` declares `UseIn=gnome`. Whether `xdg-desktop-portal`
  uses it under `XDG_CURRENT_DESKTOP=Gnoblin` is not tested [INFERENCE: it
  filters backends by `UseIn`]. Treat its rows as unverified.
- `xdg-desktop-portal-gnoblin` declares every interface in the Gnoblin list. It
  is optional and is not in the ISO profile. Under a Gnoblin session it is the
  backend for those interfaces only when it is installed.
- An interface with no ISO backend has no implementation on the image. Treat a
  request for it as failing until a test shows otherwise.
- The integration test drives the Screenshot backend with a stand-in frontend
  on a private bus. It does not run a real `xdg-desktop-portal` session.
