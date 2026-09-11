# Standalone shell

Bingux is a desktop shell. Gnoblin supplies the compositor and session, while
Bingux supplies the user interface. The shell also runs on another Wayland
compositor when that compositor provides the protocols used by the selected
features.

## Build

Install a matching Qt 6 development environment, Quickshell, a C compiler,
Cargo and the system libraries used by the optional helpers. Then run:

On Fedora, use the spec as the dependency source instead of maintaining a
second package list:

```sh
sudo dnf install rpmdevtools
sudo dnf builddep packaging/rpm/bingux.spec
```

Install Quickshell from the package source that matches the Qt version on the
host. The shell does not bundle or replace Quickshell.

```sh
make
make check
```

The build produces the two QML plugins, the audio meter and the Rust search and
status daemons. Quickshell is a runtime dependency because its Qt build must
match the shell process.

## Install for your user

For a normal source install, use the managed user target:

```sh
make install-user
```

This builds the native pieces if needed, installs the complete Bingux payload
under `$XDG_DATA_HOME/bingux` (normally `~/.local/share/bingux`), and starts
`bingux.target`. The only files outside that root are symlinks in
`$XDG_BIN_HOME` (normally `~/.local/bin`) and `$XDG_CONFIG_HOME/systemd/user`;
they are recorded in
`.bingux-install.json` and are removed safely by the matching uninstall command.
Make sure `~/.local/bin` is on `PATH` if you want to run `binguxctl` and the
other convenience commands directly. The installer detects `qs` or
`quickshell`; set `BINGUX_QUICKSHELL=/path/to/qs` when the matching Quickshell
build is not on `PATH`. It also installs `bingux-uninstall`, so the source
checkout is not needed later to remove the managed installation.

Remove the managed install with:

```sh
make uninstall-user
```

Once `~/.local/bin` is on `PATH`, the equivalent standalone command is:

```sh
bingux-uninstall
```

This stops Bingux, removes its integration symlinks, and deletes only the
managed installation root. It deliberately keeps `$XDG_CONFIG_HOME/bingux`,
so layouts, search settings, extensions and notification history survive a
reinstall. Use `USER_PREFIX=/some/other/root` with both commands when a
different central location is required.

## Package or stage an install

Use a staging directory when creating a package or testing an install:

```sh
make install PREFIX=/usr DESTDIR="$PWD/dist/root"
```

The package-oriented installer writes only below `DESTDIR`. It installs shell files under
`share/bingux/shell`, native QML plugins under `lib/bingux/qml`, helper programs
under `bin` and `libexec/bingux`, and user-systemd units under
`lib/systemd/user`. It does not enable a service or change an existing user
configuration. This layout is for RPM and other package builders; source users
should prefer the managed `make install-user` flow above.

Enable the target after a package install:

```sh
systemctl --user daemon-reload
systemctl --user enable --now bingux.target
```

The shell launcher clears core dumps. Services restart after a failure and use
the same QML import path as the shell.

## Runtime requirements

The full shell uses Qt 6, Quickshell, PipeWire or PulseAudio, NetworkManager,
systemd user services, GNOME settings helpers, `wl-clipboard`, `xdg-utils`,
PyGObject and the Python libraries used by document previews. The terminal
sidebar additionally needs a Qt 6 QMLTermWidget build that matches Quickshell.

The terminal plugin is optional. If it is absent, the rest of the shell stays
available and the sidebar shows a retry message.

## Configuration

User state lives below `$XDG_CONFIG_HOME/bingux`. The shell does not require a
distribution-specific profile or generated machine configuration. Search,
status and extension settings are ordinary files and can be copied between
machines.

For extension development, read [Extensions](extensions.md). For isolated
testing, use [portable testing](portable-testing.md).
