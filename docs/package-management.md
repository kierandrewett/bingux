# Packaging

Bingux is distributed as a set of packages: the shell, its configuration
command, and the apps that go with it. It does not define a system profile,
kernel, host, Flatpak policy or secret store.

## The packages

| Package                | What it installs                                                                                                                                | Install it when                                                                       |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `bingux`               | The shell, the search and status helper services, `binguxctl`, the user units                                                                   | You want the Bingux desktop                                                           |
| `bingux-config`        | The `bingux-config` command                                                                                                                     | Pulled in by the shell and Settings                                                   |
| `bingux-settings`      | Settings                                                                                                                                        | You want to change Bingux and system settings in one place                            |
| `bingux-files`         | Files, the file manager                                                                                                                         | You want a file manager that matches the shell                                        |
| `bingux-audio-effects` | The sound effects engine. It hosts published plugins (Linux Studio Plugins, Calf, ZamAudio, MDA, x42, Rubber Band) and installs none of its own | You want an effects stack (equaliser, compressor and the rest) under Settings > Sound |
| `bingux-taxonomy`      | A window that shows every Bingux UI component                                                                                                   | You build Bingux apps. Ordinary desktops do not need it                               |

All six have the same version. Settings, Files and Taxonomy each have a desktop
entry, an icon in the `hicolor` theme and AppStream metadata, so they show up in
the application list, in search and in software centres.

## Package repositories

Signed repositories for Fedora 43, Debian 13 and Arch Linux carry every Bingux
package. Add one and your package manager installs and updates Bingux. Each
repository has two channels. Add the one you want:

| Channel   | What it carries                                 | Repository name  |
| --------- | ----------------------------------------------- | ---------------- |
| `stable`  | Released versions, such as 0.2.0                | `bingux-stable`  |
| `nightly` | The newest build of the main branch, once a day | `bingux-nightly` |

A nightly build is numbered between two releases:
`0.1.1~nightly.20261031.5.gabc1234` is newer than release 0.1.0 and older than
release 0.1.1. Installing a release replaces the nightly build of the same
version.

The packages and the repository metadata are signed with one key, published at
`https://repo.gnoblin.org/gpg.key`. Its fingerprint is:

```text
REPLACE-WITH-KEY-FINGERPRINT
```

Compare the fingerprint your package manager shows with this one before you
accept the key.

### Fedora

```sh
sudo curl -fsSL -o /etc/yum.repos.d/bingux-stable.repo \
  https://repo.gnoblin.org/bingux/stable/fedora/bingux-stable.repo
sudo dnf install bingux
```

For the nightly channel, use `nightly` in both places of the first command.
`dnf` shows the key and asks you to accept it the first time it reads the
repository. The repository file turns on `gpgcheck` (package signatures) and
`repo_gpgcheck` (metadata signatures). The packages are built for Fedora 43.

### Debian 13

```sh
sudo install -d -m 0755 /etc/apt/keyrings
sudo curl -fsSL -o /etc/apt/keyrings/bingux.asc https://repo.gnoblin.org/gpg.key
sudo curl -fsSL -o /etc/apt/sources.list.d/bingux.sources \
  https://repo.gnoblin.org/bingux/stable/deb/bingux-stable-trixie.sources
sudo apt update
sudo apt install bingux
```

For the nightly channel, use `nightly` in both URLs. The sources file restricts
the repository to the key you downloaded. Ubuntu has no packages: the Rust
toolchain of Ubuntu 24.04 is older than the 1.88 that the apps need.

### Arch Linux

```sh
curl -fsSL -o bingux.key https://repo.gnoblin.org/gpg.key
sudo pacman-key --add bingux.key
sudo pacman-key --lsign-key REPLACE-WITH-KEY-FINGERPRINT
printf '\n' | sudo tee -a /etc/pacman.conf > /dev/null
curl -fsSL https://repo.gnoblin.org/bingux/stable/arch/bingux.conf | sudo tee -a /etc/pacman.conf
sudo pacman -Syu bingux
```

The `bingux.conf` file holds this section, which requires a valid signature on
every package and on the database:

```ini
[bingux]
SigLevel = Required DatabaseRequired
Server = https://repo.gnoblin.org/bingux/stable/arch/$arch
```

For the nightly channel, download `arch/bingux-nightly.conf` from the nightly
directory. Its section is named `[bingux-nightly]` because pacman names the
database file after the section. This repository is separate from the
`[bingux-os]` repository below, which belongs to the Bingux OS image.

### Both channels

- `dnf` and `apt` use the newest version of any enabled repository, so with both
  channels enabled you follow the nightly build.
- `pacman` uses the first repository in `/etc/pacman.conf` that has the package,
  not the newest. List `[bingux-nightly]` above `[bingux]` to follow the nightly
  build, or below it to stay on releases.

To go back to the release, remove the nightly repository and downgrade:

```sh
sudo rm /etc/yum.repos.d/bingux-nightly.repo
sudo dnf --refresh distro-sync bingux
```

On Debian, delete the nightly sources file, run `sudo apt update`, then
`sudo apt install --allow-downgrades bingux=VERSION` with the release version.
On Arch, delete the `[bingux-nightly]` section, run `sudo pacman -Sy`, then
`sudo pacman -S bingux`.

### Remove a repository

Remove the repository, then the key. Removing a repository does not remove
Bingux; see [Remove the packages](#remove-the-packages).

- Fedora: delete `/etc/yum.repos.d/bingux-*.repo`. List the imported keys with
  `rpm -qa 'gpg-pubkey*' --queryformat '%{NAME}  %{SUMMARY}\n'` and remove the
  one whose fingerprint matches with `sudo rpm -e NAME`.
- Debian: delete `/etc/apt/sources.list.d/bingux.sources` and
  `/etc/apt/keyrings/bingux.asc`, then run `sudo apt update`.
- Arch: delete the `[bingux]` and `[bingux-nightly]` sections, then run
  `sudo pacman-key --delete REPLACE-WITH-KEY-FINGERPRINT`.

## Fedora

The supported repository target is the [Bingux COPR](https://copr.fedorainfracloud.org/coprs/kierandrewett/bingux/).
The project is public, but builds are not advertised as ready until the first
clean package and install check passes.

After a build is published, install it with:

```sh
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

To install only Settings, Files and Taxonomy for your user, without the shell,
use `make install-apps` (remove with `make uninstall-apps`); see
[Install only the apps](standalone.md#install-only-the-apps-for-your-user).

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
