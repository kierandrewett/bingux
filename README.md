# Bingux

Bingux is a Wayland desktop shell. It provides a top bar, dock, application
search, notifications, media controls and a sidebar for notes, a terminal
and system information.

Bingux runs on [Gnoblin](https://github.com/kierandrewett/gnoblin). Gnoblin
manages windows and the login session; Bingux draws the desktop controls.
Other compositors with layer-shell support can run the general interface,
but some window controls and shortcuts require Gnoblin.

## Install

The native build and install path is the supported path: [standalone shell](docs/standalone.md).
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
