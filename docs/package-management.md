# Packaging

Bingux is distributed as a set of packages: the shell, its configuration
command, and the apps that go with it. It does not define a system profile,
kernel, host, Flatpak policy or secret store.

## The packages

| Package | What it installs | Install it when |
| --- | --- | --- |
| `bingux` | The shell, the search and status helper services, `binguxctl`, the user units | You want the Bingux desktop |
| `bingux-config` | The `bingux-config` command | Pulled in by the shell and Settings |
| `bingux-settings` | Settings | You want to change Bingux and system settings in one place |
| `bingux-files` | Files, the file manager | You want a file manager that matches the shell |
| `bingux-audio-effects` | The sound effects engine and its LADSPA plugins | You want equaliser, compressor and other effects under Settings > Sound |
| `bingux-taxonomy` | A window that shows every Bingux UI component | You build Bingux apps. Ordinary desktops do not need it |

All six have the same version. Settings, Files and Taxonomy each have a desktop
entry, an icon in the `hicolor` theme and AppStream metadata, so they show up in
the application list, in search and in software centres.

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
```

`bingux` recommends Settings and Files, and `dnf` installs recommended packages
by default, so this installs both. To install the shell alone:

```sh
sudo dnf install --setopt=install_weak_deps=False bingux
```

Add the apps you want later by name, for example
`sudo dnf install bingux-audio-effects`.

The package enables `bingux.target` for every user. The target starts the search
and status helper services in Gnoblin sessions. Log out and back in after the
first install.

If Gnoblin uses an `init.lua` file, load package and user drop-in directories.
The managed source installer does this automatically. For a system package,
add these lines manually; this loads the shipped Bingux module, enables the
protocols Bingux uses, hands OSD rendering to Bingux, and disables duplicate
compositor motion for Bingux surfaces:

```lua
local g = require("gnoblin")
g.load("/usr/share/gnoblin/conf.d/*.lua")
g.load("~/.config/gnoblin/conf.d/*.lua")
```

Source installs use the corresponding `$USER_PREFIX/share/gnoblin/conf.d/`
directory. Gnoblin watches the main file and matching modules, so shell and
effect changes use the same reload path. Protocol advertisement is fixed when
the compositor session starts. Log out and back in after the first enable.
Removing Bingux removes only its package drop-in; the glob remains valid.

The package does not change the selected login session. Use Gnoblin for the
compositor/session, or start Bingux from another compatible Wayland session.
Quickshell is intentionally not bundled; the RPM depends on its executable
file so repositories can use names such as `quickshell-git` without making
Bingux depend on one repository's package name.

## Arch Linux and Bingux OS

Bingux OS is an Arch-based image that installs these packages from the
`[bingux-os]` repository. On Arch Linux, add the same repository to
`/etc/pacman.conf`:

```ini
[bingux-os]
SigLevel = Optional TrustAll
Server = https://github.com/kierandrewett/bingux/releases/download/pacman-x86_64
```

The packages are not signed yet, so `SigLevel = Optional TrustAll` accepts
whatever the release contains. Use it only on a machine you are willing to test
on. Then install:

```sh
sudo pacman -Syu bingux bingux-settings bingux-files
```

Arch has no weak dependencies. `pacman` shows the optional dependencies
(network, Bluetooth, sound and so on) after the install; install the ones whose
Settings pages you use.

## Open the apps

- Settings: search for "Settings", or run `bingux-settings`. Open a page with
  `bingux-settings network` or `xdg-open bingux-settings://network`.
- Files: search for "Files", or run `bingux-files [FOLDER]`. To open folders in
  Files from every application:

  ```sh
  xdg-mime default bingux-files.desktop inode/directory
  ```

  You can also choose it under **Settings > Applications > Default applications**.
- Sound effects: turn them on in **Settings > Sound > Effects**. Settings enables
  the user services `bingux-effects@output.service` and
  `bingux-effects@input.service`. Check them with
  `systemctl --user status bingux-effects@output.service`.

## Remove the packages

```sh
sudo dnf remove bingux bingux-settings bingux-files bingux-audio-effects bingux-config
# Arch Linux and Bingux OS:
sudo pacman -Rns bingux bingux-settings bingux-files bingux-audio-effects bingux-config
```

On Fedora, removal stops and unregisters the units of the packages, and an
upgrade restarts the helper services and any running effects. Removing the
packages does not delete your data: see [Runtime state](#runtime-state).

## Build the packages yourself

From a clean checkout, build every RPM from the source archive:

```sh
sudo dnf install rpm-build rpmdevtools
make rpm-package
```

`make rpm-package` builds `bingux-VERSION.tar.xz`, runs `rpmbuild`, and leaves
the packages in `/var/tmp/BinguxPkg/rpmbuild/RPMS` and the source package in
`/var/tmp/BinguxPkg/rpmbuild/SRPMS`. Change the folder with
`make rpm-package PACKAGE_DIR=/some/folder`. Install the result with
`sudo dnf install /var/tmp/BinguxPkg/rpmbuild/RPMS/x86_64/*.rpm`. To check the
spec, the desktop entries and the AppStream files without building:

```sh
make rpm-check
```

The Arch packages build from `packaging/arch/PKGBUILD`:

```sh
make arch-package
sudo pacman -U /var/tmp/BinguxPkg/arch/out/*.pkg.tar.zst
```

Without `makepkg`, the command builds inside an `archlinux` container with
`podman` or `docker`.

Both builds download Rust crates and a prebuilt Skia archive, so they need the
network. To build uncommitted changes as a packaging check, set
`BINGUX_ARCHIVE_WORKTREE=1`; never publish what that builds. How a release is
cut is in [releases](release.md); the contents of each package and the open
decisions are in [Bingux userspace](../design/userspace.md).

## Source installation

For a personal source install, use `make install-user`; see the
[standalone shell](standalone.md) guide. It keeps the payload in one managed
root and provides `make uninstall-user` for a complete, configuration-preserving
removal.

`make install PREFIX=/usr DESTDIR=...` remains available for package builders.
That deterministic staging path writes only below `DESTDIR` and is intentionally
separate from the managed user install. Package builders choose which staged
files go into which package; `packaging/rpm/bingux.spec` and
`packaging/arch/PKGBUILD` are the reference lists.

## Other distributions

APT packages are not published. Keep their packaging work outside the shell
source tree until the native install layout and runtime dependencies are
stable. Do not add distribution-specific host configuration to Bingux.

## Runtime state

User configuration belongs under `$XDG_CONFIG_HOME/bingux`. The package owns
only its installed files and user-systemd units. Uninstalling it does not delete
notes, layouts, search indexes, extension settings, notification history, the
network data usage records (`$XDG_DATA_HOME/bingux/data-usage.sqlite3`), sound
effect presets or the settings of Files.
