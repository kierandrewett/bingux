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
different absolute location. The shell runtime is built from source as
`bingux-shell` (see `docs/standalone.md`); no separate Quickshell install is
needed.

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

Signed package repositories for Fedora, Debian 13 and Arch, each with a `stable`
and a `nightly` channel, are described in
[package management](docs/package-management.md#package-repositories).

A Fedora package is being prepared in the
[Bingux COPR](https://copr.fedorainfracloud.org/coprs/kierandrewett/bingux/).
The repository does not publish a ready package until a clean Fedora build and
a fresh install test pass. The shell, Settings, Files, the sound effects engine
and the configuration command are separate packages for Fedora
(`make rpm-package`) and Arch Linux (`make arch-package`); see
[package management](docs/package-management.md).

For an isolated source run, see [portable testing](docs/portable-testing.md).

See [dock application menus](docs/dock-menus.md) for window and application actions.

See [sidebar panels](docs/sidebar-loading.md) for loading and retained content.

See [desktop panels](docs/popup-loading.md) for first opening and dismissal.

See [portals](docs/portals.md) for the Screenshot backend, its permission rules and the known state of each portal interface.

See the [login screen](docs/login-screen.md) and [secure session locking](docs/locking.md)
for the greeter and the lock screen, [Bingux OS](distro/README.md) for the live image,
and [installing Bingux OS](docs/installer.md) for the installer.
For shell behaviour and configuration, see the [shell guide](shell/bingux/README.md).
If something does not start or shows no data, see [troubleshooting](docs/troubleshooting.md).

[Bingux Settings](docs/settings.md) is the graphical front for shell and
system settings; [configuration](docs/configuration.md) covers the file and CLI.
[Bingux Files](docs/files.md) is the file manager: tabs, drag and drop, drives,
and the same file view as the file chooser.

## Source layout

| Directory | What lives there |
| --------- | ---------------- |
| `apps/` | Programs you launch: Settings (`apps/settings`), Files (`apps/files`) and the component catalogue (`apps/taxonomy`). |
| `crates/` | Rust libraries the apps and services share: the design system (`crates/ui`), the configuration store (`crates/config`), the file model, search, previews, data usage and the DSP plugins. |
| `services/` | Background programs and helpers: search and status daemons, the portal backend, the session and shell helpers, the preview host and the sound effects engine. |
| `bsm/` | The Bingux Session Manager: one directory per crate (`core`, `manager`, `compositor`, `host`, `helper`, `appearance`, `proto`). |
| `shell/` | The desktop shell: the QML (`shell/bingux`), the Quickshell-based runtime (`shell/runtime`), its patches and the terminal widget. |
| `gnoblin/` | What Bingux installs into the Gnoblin compositor: the Lua modules. |
| `native/` | C and C++ components: QML plugins, Wayland helpers and the GTK frame helper. |
| `tools/` | Command-line tools: `binguxctl` and the inventory script. |
| `vendor/` | Forked third-party Rust crates (Freya and winit). |
| `packaging/`, `distro/` | RPM, Arch and systemd packaging; the Bingux OS image and installer. |
| `docs/`, `design/` | User and developer documentation; design rationale and rules. |
| `scripts/`, `tests/` | Build, install and lint scripts; component, input and integration checks. `scripts/test-desktop` starts an isolated nested desktop. |

A complete install includes the native text plugin and QML files. Quickshell and every Qt plugin must use a compatible Qt build.

## Development

Run an isolated desktop with the dependencies listed in the testing guide:

```sh
scripts/test-desktop --check
scripts/test-desktop
```

The preview uses temporary settings. Tests that access system services can
still change the host if you use their controls. See the testing guide for
which features work on other compositors.
