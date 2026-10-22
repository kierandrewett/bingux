# Bingux portal backend

`xdg-desktop-portal-bingux` is the Bingux implementation of the
`org.freedesktop.impl.portal.desktop.bingux` backend. It owns the permission
rules, the permission store and the enforcement for the portal interfaces that
Bingux serves. The first interface is `org.freedesktop.impl.portal.Screenshot`.

Gnoblin routes an interface to a backend through `gnoblin.configure.portals`.
Gnoblin does not check permissions. The frontend calls the chosen backend
directly, so this backend must enforce the rules itself.

## Files

| Path                                                  | Purpose                                          |
| ----------------------------------------------------- | ------------------------------------------------ |
| `$XDG_CONFIG_HOME/bingux/portal-permissions.toml`     | Permission store, read on each request.          |
| `/usr/libexec/bingux/xdg-desktop-portal-bingux`       | Backend executable, started by D-Bus or systemd. |
| `/usr/share/xdg-desktop-portal/portals/bingux.portal` | Routing descriptor for `xdg-desktop-portal`.     |

When `XDG_CONFIG_HOME` is unset, the store lives under `$HOME/.config`.

## Identity

The backend derives an identity for each caller. It follows Gnoblin's
`gnoblin-portal-identity.c` rules:

- The frontend must own the sender name.
- The request handle must parse under `/org/freedesktop/portal/desktop/request/`.
- The caller's user ID must match the backend's user ID.
- The identity is `app-id:<id>` for a sandboxed app, or `host-exe:<path>` for a host process.

A request with no identity is refused. It never falls back to a default allow.

## Store

Rules use the capabilities `screen-cast`, `remote-desktop`, `input-capture`,
`screenshot` and `access`. Levels are `inherit`, `ask`, `allow` and `deny`.

- A matching `deny` always wins.
- Otherwise the last matching rule wins.
- `app_id` is an exact match. `exe` is an exact path. `pattern` is a glob over
  the full identity string, such as `host-exe:/usr/bin/*`. The glob has no Lua patterns.

The file is written atomically. The parser validates it on load. If the file
exists but fails validation, the backend denies every portal request until the
file is fixed. A missing file has no rules and a default of `inherit`, which
shows the selector.

## Screenshot decisions

| Level     | Result                                                       |
| --------- | ------------------------------------------------------------ |
| `deny`    | Refused with response code 2.                                |
| `ask`     | Shows the interactive selector.                              |
| `allow`   | Shows the selector only when the request sets `interactive`. |
| `inherit` | Shows the interactive selector.                              |

Gnoblin's `inherit` showed consent without a store check. This backend shows
the selector for `inherit` as well.

The backend calls `binguxctl capture request ID --interactive` or
`--desktop`, then polls `binguxctl capture result ID` about every 200 ms. It
gives up after 25 failed polls in a row and cancels the capture. A user cancel
returns response code 1. Any other failure returns response code 2.

## GlobalShortcuts provider

`bingux-global-shortcuts` owns `org.gnoblin.Shell.GlobalShortcutsProvider` at
`/org/gnoblin/Shell/GlobalShortcutsProvider`. Only the Gnoblin portal backend
(`org.freedesktop.impl.portal.desktop.gnoblin`) may call it. Other callers are refused.

- `BindShortcuts` asks the shell for approval through `binguxctl ipc shortcuts`.
  The shell shows the approval UI and returns the approved bindings. The request
  waits for the user.
- Approval needs an app_id. The frontend derives it from the caller. A host
  program with no app_id is refused: the shell rejects an empty app_id, and the
  provider has no other identity to use. The Gnoblin backend passes only
  `app_id` to the provider, so the request handle that names a host caller is
  not available. Fixing this needs a contract change in Gnoblin and here
  together: `BindShortcuts` and `ConfigureShortcuts` take the request handle,
  and grants are keyed by `identity::Identity::key()`. See `docs/portals.md`.
- A user denial returns `AccessDenied` on the provider. The frontend maps that
  error to response code 2, not 1.
- Approved grants are kept in `$XDG_CONFIG_HOME/bingux/global-shortcuts.toml`,
  or under `~/.config` when `XDG_CONFIG_HOME` is unset. The file mode is 0600.
- When saved bindings change, the provider calls `RebindShortcuts` on the backend
  to apply them.

## Build and test

```sh
CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo test --locked --manifest-path packages/bingux-portal/Cargo.toml
```

`make check` and `make daemons` build and test this crate with the other daemons.
