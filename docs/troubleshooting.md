# Troubleshooting

Start with `make doctor` in the source checkout. It checks the compiler, Cargo,
native libraries, Qt Quick, Quickshell and user systemd, and it names the
executable that failed. Then use the section below that matches the symptom.

## Quickshell warns about a Qt version mismatch

Quickshell uses private Qt interfaces. It must run against the same Qt version
it was built with. `make doctor` prints both versions and the executable path.

1. Rebuild or reinstall Quickshell for the Qt version on the host.
2. To use a different build, set `BINGUX_QUICKSHELL` to its path.
3. Run `make doctor` again. A clean run prints no Qt mismatch line.

`BINGUX_QUICKSHELL` is honoured by the installed launchers, by `binguxctl`
(`--quickshell` overrides it for one command) and by the desktop menu.

## The bar, dock or panels do not appear

Bingux draws its own surfaces with `zwlr_layer_shell_v1`. The compositor must
offer this protocol. Gnoblin does. Check another compositor with
`wayland-info | grep zwlr_layer_shell_v1`.

If the protocol is present, check that the shell is running:

```sh
systemctl --user status bingux.service
journalctl --user -u bingux.service -b
```

A failed unit usually names a missing QML module or a Qt mismatch (see above).

## Parts of the shell show no data or stay unavailable

Long-running helpers publish sockets that the shell reads. Each one has a
user service and a socket path under `$XDG_RUNTIME_DIR`.

| Data                       | Service                  | Socket                                                                                    |
| -------------------------- | ------------------------ | ----------------------------------------------------------------------------------------- |
| Search results             | `bingux-searchd.service` | `$XDG_RUNTIME_DIR/bingux/search-v1.sock`                                                  |
| CPU, memory, network, OSD  | `bingux-statusd.service` | `$XDG_RUNTIME_DIR/bingux/metrics-v1.sock` and `$XDG_RUNTIME_DIR/bingux/osd-v2.sock`       |
| Windows, workspaces, input | Gnoblin compositor       | `$XDG_RUNTIME_DIR/gnoblin/compositor-v1.sock`, or the path in `GNOBLIN_COMPOSITOR_SOCKET` |

1. Check that the socket file exists: `ls "$XDG_RUNTIME_DIR/bingux"`.
2. If it is missing, read the service log:
   `journalctl --user -u bingux-statusd.service -b`.
3. Restart it: `systemctl --user restart bingux-statusd.service`.

Each helper service restarts three seconds after a crash
(`Restart=on-failure`, `RestartSec=3`) and writes no core file
(`LimitCORE=0`). `bingux-lock.service` is a manual diagnostic unit and does not
restart.

The shell and the helpers retry with a short delay. When the socket returns,
the affected panel recovers without a shell restart. A missing compositor
socket on a compositor other than Gnoblin is expected. Window and workspace
features then stay unavailable.

## Settings pages show "Gnoblin is not running."

Bingux Settings reads compositor state through `gnoblinctl`. The message
means the compositor socket does not exist, so the session is not Gnoblin or
the compositor has not started. Pages that use only `bingux-config`, GSettings
or NetworkManager still work.

## Settings cannot find `bingux-config`

Settings looks for `bingux-config` in this order:

1. The path in `BINGUX_CONFIG_BIN`.
2. The first match on `PATH`.
3. A file next to the `bingux-settings` executable.

Set `BINGUX_CONFIG_BIN` when you run a build that is not installed.

## A helper reports a missing Python module or Night Light is unavailable

Some features use optional packages. When one is missing, the helper replies
with the package name instead of failing, and `make doctor` lists it under
"Optional features not available".

| Package                           | Feature that needs it                     | What you see without it                                          |
| --------------------------------- | ----------------------------------------- | ---------------------------------------------------------------- |
| `python3-gobject`                 | Control Centre, calendar and most helpers | "PyGObject (python3-gobject) is not installed."                  |
| `python3-cairo`, `python3-pillow` | Document and image previews               | "Python module cairo is missing; install python3-cairo."         |
| `python3-pyyaml`                  | YAML front matter in Markdown previews    | "YAML frontmatter needs python3-pyyaml, which is not installed." |
| `gnome-settings-daemon`           | Night Light                               | The Night Light control shows "Unavailable".                     |

The RPM requires the first two rows and recommends the last two.
