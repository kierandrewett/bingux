# Standalone shell

Bingux is a desktop shell. Gnoblin supplies the compositor and session, while
Bingux supplies the user interface. The shell also runs on another Wayland
compositor when that compositor provides the protocols used by the selected
features.

## Build

Install a matching Qt 6 development environment, Quickshell, a C compiler,
Cargo and the system libraries used by the optional helpers. Then run:

```sh
make
make check
```

The build produces the two QML plugins, the audio meter and the Rust search and
status daemons. Quickshell is a runtime dependency because its Qt build must
match the shell process.

## Install

Use a staging directory when creating a package or testing an install:

```sh
make install PREFIX=/usr DESTDIR="$PWD/dist/root"
```

The installer writes only below `DESTDIR`. It installs shell files under
`share/bingux/shell`, native QML plugins under `lib/bingux/qml`, helper programs
under `bin` and `libexec/bingux`, and user-systemd units under
`lib/systemd/user`. It does not enable a service or change an existing user
configuration.

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
Nix profile or generated machine configuration. Search, status and extension
settings are ordinary files and can be copied between machines.

For extension development, read [Extensions](extensions.md). For isolated
testing, use [portable testing](portable-testing.md).
