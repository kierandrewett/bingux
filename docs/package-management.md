# Packaging

Bingux is distributed as a shell package. It does not define a system profile,
kernel, host, Flatpak policy or secret store.

## Fedora

The supported repository target is the [Bingux COPR](https://copr.fedorainfracloud.org/coprs/kierandrewett/bingux/).
The project is public, but builds are not advertised as ready until the first
clean package and install check passes.

After a build is published, install it with:

```sh
sudo dnf copr enable kierandrewett/bingux
sudo dnf install bingux
systemctl --user daemon-reload
systemctl --user enable --now bingux.target
```

The package does not change the selected login session. Use Gnoblin for the
compositor/session, or start Bingux from another compatible Wayland session.

## Source installation

Build and stage the shell with `make` and `make install`; see
[standalone shell](standalone.md). The installer is deterministic and writes
only below `DESTDIR`. This is the path used by package builders.

## Other distributions

APT and pacman packages are not published yet. Keep their packaging work
outside the shell source tree until the native install layout and runtime
dependencies are stable. Do not add distribution-specific host configuration
to Bingux.

## Runtime state

User configuration belongs under `$XDG_CONFIG_HOME/bingux`. The package owns
only its installed files and user-systemd units. Uninstalling it does not delete
notes, layouts, search indexes, extension settings or notification history.
