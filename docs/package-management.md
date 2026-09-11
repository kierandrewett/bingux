# Packaging

Bingux is distributed as a shell package. It does not define a system profile,
kernel, host, Flatpak policy or secret store.

## Fedora

The supported repository target is the [Bingux COPR](https://copr.fedorainfracloud.org/coprs/kierandrewett/bingux/).
The project is public, but builds are not advertised as ready until the first
clean package and install check passes.

After a build is published, install it with:

```sh
# Install a Quickshell 0.2.1 package that provides /usr/bin/qs first.
# It must be built against the same Qt 6 ABI as the host.
sudo dnf copr enable kierandrewett/bingux
sudo dnf install bingux
systemctl --user daemon-reload
systemctl --user enable --now bingux.target
```

If Gnoblin is installed, add the shipped integration fragment to the user's
Gnoblin TOML file. This enables the protocols Bingux uses, hands OSD rendering
to Bingux, and disables duplicate compositor motion for Bingux surfaces:

```toml
include = ["/usr/share/bingux/gnoblin.toml"]
```

The fragment is installed as package data and is never written over the user's
configuration. Source installs use the corresponding
`$USER_PREFIX/share/bingux/gnoblin.toml` path. Gnoblin watches the main file and
the included fragment, so shell/effect changes take effect through the same
hot-reload path. Protocol advertisement is fixed when the compositor session
starts; log out and back in after first enabling the fragment.

The package does not change the selected login session. Use Gnoblin for the
compositor/session, or start Bingux from another compatible Wayland session.
Quickshell is intentionally not bundled; the RPM depends on its executable
file so repositories can use names such as `quickshell-git` without making
Bingux depend on one repository's package name.
Remove the package with:

```sh
sudo dnf remove bingux
```

The RPM owns the user-systemd lifecycle: removal stops and unregisters Bingux's
units, while upgrades request a restart after the new files are installed.

## Source installation

For a personal source install, use `make install-user`; see the
[standalone shell](standalone.md) guide. It keeps the payload in one managed
root and provides `make uninstall-user` for a complete, configuration-preserving
removal.

`make install PREFIX=/usr DESTDIR=...` remains available for package builders.
That deterministic staging path writes only below `DESTDIR` and is intentionally
separate from the managed user install.

## Other distributions

APT and pacman packages are not published yet. Keep their packaging work
outside the shell source tree until the native install layout and runtime
dependencies are stable. Do not add distribution-specific host configuration
to Bingux.

## Runtime state

User configuration belongs under `$XDG_CONFIG_HOME/bingux`. The package owns
only its installed files and user-systemd units. Uninstalling it does not delete
notes, layouts, search indexes, extension settings or notification history.
