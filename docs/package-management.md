# Packaging

Bingux is distributed as four packages: the desktop, and the three apps that
go with it. It does not define a system profile, kernel, host, Flatpak policy
or secret store.

## The packages

| Package           | What it installs                                                                                                                                                                                                                                                    | Install it when                                            |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| `bingux`          | The whole desktop: the shell, every core service (search, status, the login and lock screens, the portal backend, the sound effects engine, the preview helper Files runs, the `bingux-config` command), `binguxctl`, `bsm` (installed disabled) and the user units | You want the Bingux desktop                                |
| `bingux-settings` | Settings                                                                                                                                                                                                                                                            | You want to change Bingux and system settings in one place |
| `bingux-files`    | Files, the file manager                                                                                                                                                                                                                                             | You want a file manager that matches the shell             |
| `bingux-taxonomy` | A window that shows every Bingux UI component                                                                                                                                                                                                                       | You build Bingux apps. Ordinary desktops do not need it    |

All four have the same version, and each app depends on exactly that version of
`bingux`. `bingux` depends on the Gnoblin packages `gnoblin` and
`xdg-desktop-portal-gnoblin`, which come from the Gnoblin repository on the same
host. Settings, Files and Taxonomy each have a desktop entry, an icon in the
`hicolor` theme and AppStream metadata, so they show up in the application list,
in search and in software centres.

Until 0.1.0 the desktop was split into `bingux`, `bingux-config`,
`bingux-audio-effects` and `bingux-preview-host` (and `bsm` on Arch). `bingux`
now replaces those packages: it obsoletes, conflicts with and provides each of
their names, so an upgrade removes them and installs the one package. The sound
effects plugins (`lsp-plugins` and the rest) are recommended, not required.

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
91BA A379 0941 B9B3 BBA6  3BC4 F1B8 D899 DF3D 786E
```

Compare the fingerprint your package manager shows with this one before you
accept the key.

### Fedora

Install from the Bingux repository above. The [Bingux COPR](https://copr.fedorainfracloud.org/coprs/kierandrewett/bingux/)
is no longer built: the packages are written with pkgsmith from a built prefix
and there is no source RPM to submit.

```sh
sudo dnf install bingux
```

`bingux` recommends Settings and Files, and `dnf` installs recommended packages
by default, so this installs both. To install the shell alone:

```sh
sudo dnf install --setopt=install_weak_deps=False bingux
```

Add the apps you want later by name, for example
`sudo dnf install bingux-taxonomy`.

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
sudo dnf remove bingux bingux-settings bingux-files bingux-taxonomy
# Debian:
sudo apt remove bingux bingux-settings bingux-files bingux-taxonomy
# Arch Linux and Bingux OS:
sudo pacman -Rns bingux bingux-settings bingux-files bingux-taxonomy
```

On Fedora, removal stops and unregisters the units of the packages, and an
upgrade restarts the helper services and any running effects. Removing the
packages does not delete your data: see [Runtime state](#runtime-state).

## Build the packages yourself

Two steps build the packages, and `pkgsmith` writes them; nothing else builds a
package. Build everything into a prefix with this machine's toolchain, then write
the packages from the prefix:

```sh
sudo scripts/install-build-deps.sh fedora    # or debian, or arch
scripts/install-pkgsmith.sh                  # builds pkgsmith into ./pkgsmith-bin
make prefix                                  # builds all of Bingux into ./prefix
PKGSMITH=$PWD/pkgsmith-bin/pkgsmith FORMATS=rpm,tar make packages
```

The packages are in `dist/`. Build on the distribution you package for: the shell
links the Qt of that machine. [Packaging](packaging.md) explains the scripts, the
formats and what the packages depend on. To check the definitions, the desktop
entries and the AppStream files without building:

```sh
make package-check
```

The build downloads Rust crates and a prebuilt Skia archive, so it needs the
network. How a release is cut is in [releases](release.md); the contents of each
package and the open decisions are in [Bingux userspace](../design/userspace.md).

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
files go into which package; `pkgsmith/` holds the lists (see
[Packaging](packaging.md)).

## Other distributions

Debian 13 packages are written by the same scripts. There are no packages for
Ubuntu (the Rust of Ubuntu 24.04 is older than the 1.88 that the apps need) or for
Alpine (Bingux needs systemd; see [Packaging](packaging.md#why-there-is-no-apk)).
Do not add distribution-specific host configuration to Bingux.

## Runtime state

User configuration belongs under `$XDG_CONFIG_HOME/bingux`. The package owns
only its installed files and user-systemd units. Uninstalling it does not delete
notes, layouts, search indexes, extension settings, notification history, the
network data usage records (`$XDG_DATA_HOME/bingux/data-usage.sqlite3`), sound
effect presets or the settings of Files.
