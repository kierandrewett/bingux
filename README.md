# Bingux

Bingux is a Wayland desktop shell. It provides a top bar, dock, application
search, notifications, media controls and a sidebar for notes, a terminal
and system information.

Bingux runs on [Gnoblin](https://github.com/kierandrewett/gnoblin). Gnoblin
manages windows and the login session; Bingux draws the desktop controls.
Other compositors with layer-shell support can run the general interface,
but some window controls and shortcuts require Gnoblin.

## Install

The supported source-install path is a managed per-user install:

```sh
make doctor
make install-user
```

Everything is kept under `$XDG_DATA_HOME/bingux` (normally
`~/.local/share/bingux`). Only convenience symlinks in `~/.local/bin` and
`~/.config/systemd/user` point into that directory. Set `USER_PREFIX` to use a
different absolute location, or set `BINGUX_QUICKSHELL` when the matching
Quickshell executable is not on `PATH`.

Remove the complete managed payload with:

```sh
make uninstall-user
# or, when ~/.local/bin is on PATH:
bingux-uninstall
```

Removal stops Bingux and deletes the install root while preserving
`$XDG_CONFIG_HOME/bingux` user settings. For package builds, use the separate
staging flow documented in [package management](docs/package-management.md).

If Gnoblin uses `init.lua`, load the package and user drop-in directories:

```lua
local g = require("gnoblin")
g.load("/usr/share/gnoblin/conf.d/*.lua")
g.load("~/.config/gnoblin/conf.d/*.lua")
```

The managed installer prints its exact package drop-in directory. Bingux user
settings remain under `$XDG_CONFIG_HOME/bingux`.

A Fedora package is being prepared in the [Bingux COPR](https://copr.fedorainfracloud.org/coprs/kierandrewett/bingux/).
The repository does not publish a ready package until a clean Fedora build and
a fresh install test pass.

For an isolated source run, see [portable testing](docs/portable-testing.md).
For shell behaviour and configuration, see the [shell guide](shell/bingux/README.md).

## Source layout

- `shell/bingux/`: desktop UI, settings and Python helpers.
- `packages/`: search and metrics daemons, native plugins and helper programs.
- `tests/`: component, input and integration checks.
- `scripts/test-desktop`: starts an isolated nested desktop for testing.
- `packaging/`: native RPM and user-systemd packaging files.

A complete install includes the native text and settings plugins as well as the
QML files. Quickshell and every Qt plugin must use a compatible Qt build.

## Development

Run an isolated desktop with the dependencies listed in the testing guide:

```sh
scripts/test-desktop --check
scripts/test-desktop
```

The preview uses temporary settings. Tests that access system services can
still change the host if you use their controls. See the testing guide for
which features work on other compositors.
