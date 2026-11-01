# Packaging

How the Bingux packages are made. This page is for maintainers. To install Bingux,
see [package management](package-management.md); to cut a release, see
[releases](release.md).

pkgsmith writes every package. It builds nothing: Bingux builds itself into a
**prefix** (a folder laid out like the installed system), and a short script says
which files of the prefix make which package.

```text
scripts/build-prefix.sh      builds ALL of Bingux into ./prefix, on the distribution you package for
scripts/build-packages.sh    runs pkgsmith/bingux.star and pkgsmith/apps.star on the prefix, writes ./dist
scripts/verify-packages.sh   installs the packages in a clean container, checks them, removes them
```

`make prefix` and `make packages` run the first two. Gnoblin has its own build and
its own CI; Bingux builds none of Gnoblin and depends on the packages Gnoblin
publishes.

## The packages

| Package           | What it is                                                                                         |
| ----------------- | -------------------------------------------------------------------------------------------------- |
| `bingux`          | The whole desktop: the shell and every core service, the system integration around them, and `bsm` |
| `bingux-settings` | The Settings app                                                                                   |
| `bingux-files`    | The Files app                                                                                      |
| `bingux-taxonomy` | The Taxonomy app (a reference window for the UI library, a development tool)                       |

**Why this split.** The desktop does not work without its services, so they are one
package: the shell (`bingux-shell`, Quickshell with the Bingux patches, and its QML
plugins), the search and status daemons, the privileged helper, the portal backend
and global shortcuts, the greeter and lock screen programs, `bingux-config`, the
sound effects engine, the preview helper Files runs, and `bsm` with its helper,
host bridge and image converter. Each of those is started by the shell, by a
systemd unit of `bingux`, or by D-Bus activation, and the shell cannot be installed
without them. Settings, Files and Taxonomy are applications: a user can run a
desktop without any of them, each has a desktop entry, an icon and AppStream
metadata, and each is started by the user. They depend on `bingux` of exactly their
own version (for example `bingux = 0.1.0-1`), because they call the configuration
command, the search daemon and the effects engine that `bingux` ships.

`bingux` holds everything in the prefix that no application claims, so a file that
the build adds is packaged without editing a script. The file lists of the
applications are in `pkgsmith/common.star` (`APPS`). `tests/packaging.py` checks
that no file is in two packages and none is in no package.

**What `bsm` installs.** `bsm` is the system compositor and session manager. Its
files are part of `bingux`, but installing them enables nothing: the package
creates the accounts `bsm` and `bingux-greeter` and the directories under `/run`
and `/var/lib`, and leaves the login manager as it is. An administrator turns it on
with `bsm-set-display-manager enable`; removing `bingux` removes that alias again
and enables greetd. [bsm packaging](bsm/packaging.md) has the files, the boot order
and the recovery steps.

### Dependencies

`bingux` depends on `gnoblin`, `xdg-desktop-portal-gnoblin` and
`xdg-desktop-portal`. The first two are published at `https://repo.gnoblin.org`
(see [package management](package-management.md#package-repositories)); Gnoblin's
CI builds them and Bingux's does not. The sound effects plugins, the film decoders
for previews, Settings, Files and greetd are recommended.

pkgsmith does not look inside programs, so the dependency lists in `pkgsmith/*.star`
are written by hand for Fedora 43, Debian 13 and Arch Linux. Two things keep them
honest:

- `scripts/check-package-libs.sh` runs `ldd` on every program of the installed
  packages in a clean container and fails on a library that is not found
  (`scripts/verify-packages.sh` runs it). Libraries a program opens with `dlopen`
  are not visible to it; the Freya apps load Wayland, xkbcommon and EGL that way, so
  `pkgsmith/apps.star` names them.
- The shell links Qt's private API, which changes with every minor release. The
  prefix records the Qt minor version it was built with, and the package depends on
  that minor and no other (`qt6-qtbase >= 6.10, < 6.11` on Fedora,
  `libqt6core6t64 (>= 6.8) (<< 6.9)` on Debian, `qt6-base>=6.10 qt6-base<6.11` on
  Arch). A Qt update therefore holds the package back until Bingux is rebuilt.

### Upgrades from the earlier packages

Before 0.1.0 the desktop was `bingux`, `bingux-config`, `bingux-audio-effects` and
`bingux-preview-host`, and the Bingux OS image had a separate `bsm`. `bingux` takes
those names over in the way of each format:

| Format      | Fields                                                             |
| ----------- | ------------------------------------------------------------------ |
| `rpm`       | `Obsoletes:` and `Provides:` of each old name                      |
| `deb`       | `Replaces:`, `Conflicts:` and `Provides:` of each old name         |
| `archlinux` | `replaces`, `conflicts` and `provides` of each old name, and `bsm` |

`scripts/verify-packages.sh` installs stand-ins for the old names first and checks
that installing the new packages removes them. `bingux-settings`, `bingux-files` and
`bingux-taxonomy` keep their names, so they upgrade in place.

## Build a prefix

Install the build dependencies, then build. Run this on the distribution you
package for: the Qt plugins and the shell link its private API, and the Rust
programs link its C library.

```sh
sudo scripts/install-build-deps.sh fedora    # or debian, or arch
scripts/build-prefix.sh                      # ./prefix, ./prefix.env
```

`scripts/build-prefix.sh` runs `make all bsm`, installs with `make install` and
`make install-bsm` (`PREFIX=/usr`, `DESTDIR=prefix`), adds the PAM services,
documents and licences, compresses the man pages, strips the programs and fails
when a text file names the build machine's paths. It writes `prefix.env` beside the
prefix with the facts of the machine (architecture, Qt minor version, Fedora dist
tag, distribution) so the packages can be written on another machine. Variables
(`PREFIX_DIR`, `BUILD_DIR`, `CARGO_TARGET`, `JOBS`, `VERSION`, `LIBDIR_NAME`,
`SKIP_BUILD`) are listed in the header of the script.

The libraries live in `/usr/lib64/bingux` on Fedora and `/usr/lib/bingux` on Debian
and Arch. The build needs the network for the Rust crates and for the prebuilt Skia
archive, and about 6 GB.

## Write the packages

```sh
scripts/install-pkgsmith.sh                  # builds pkgsmith at .github/pkgsmith.ref into ./pkgsmith-bin
export PKGSMITH=$PWD/pkgsmith-bin/pkgsmith
FORMATS=rpm,tar scripts/build-packages.sh    # ./dist
```

pkgsmith has no published release, so `install-pkgsmith.sh` builds the commit named
in `.github/pkgsmith.ref`. `scripts/build-packages.sh` runs one script for each
family:

| Script                 | Writes                                                                             |
| ---------------------- | ---------------------------------------------------------------------------------- |
| `pkgsmith/bingux.star` | `bingux`                                                                           |
| `pkgsmith/apps.star`   | `bingux-settings`, `bingux-files`, `bingux-taxonomy`                               |
| `pkgsmith/common.star` | Shared by both: versions, licences, dependency helpers, the file lists of the apps |

Each script writes `rpm`, `deb`, `archlinux` and `tar`; `FORMATS` (or `-F` of
pkgsmith) picks some. A prefix holds one distribution's libraries, so CI writes one
format from each prefix, plus the tar of that prefix, which is named by distribution
(`bingux-0.1.0-amd64-fedora43.tar.xz`). Look at a plan or the files of a package
without writing anything:

```sh
pkgsmith plan  -f pkgsmith/bingux.star --prefix prefix
pkgsmith files -f pkgsmith/apps.star   --prefix prefix -F deb --where name=bingux-files
```

Versions follow `docs/versions.md` of pkgsmith: a release is `0.1.0`, a nightly is
`0.1.1-nightly.20261031.142.gabc1234`, and pkgsmith writes each in the spelling its
format orders (`0.1.1~nightly...` for rpm and deb, `0.1.1nightly20261031.142` for
Arch). The pin of an application on `bingux` uses the same spelling;
`tests/packaging.py` compares it with what pkgsmith writes.

### Why there is no apk

The scripts write no `apk`. A glibc prefix does not run on Alpine, which uses musl,
and a musl prefix is not enough: Bingux needs systemd. `bingux.target`, every user
unit, `bsm.service` and its sysusers and tmpfiles files, the user presets and the
polkit and logind rules assume systemd and logind, and Alpine ships neither (it
runs OpenRC). To add an `apk`, make a prefix on Alpine or with a musl toolchain
(the Freya apps would need a Skia build for musl) and a session that does not need
systemd, then add `"apk"` to `FORMATS` and a dependency table to the scripts.

## CI

| Workflow                 | Runs                                                            | What it does                                                                                          |
| ------------------------ | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `ci.yml`                 | every push and pull request                                     | Lint, `make check-static`, the package definitions without a build, the Settings snapshot suites      |
| `packages.yml`           | a tag `v*`, a pull request that changes packaging, a manual run | Builds pkgsmith, builds a prefix in each distribution, writes and verifies the packages, publishes    |
| `package-repository.yml` | called by `packages.yml`                                        | Signs the packages and the metadata, merges with the published channel, verifies the tree, uploads it |
| `nightly.yml`            | daily and on demand                                             | Builds a nightly version of `main` when it changed since the published nightly                        |

`packages.yml` builds in `fedora:43`, `debian:trixie` and `archlinux:base`, each on
amd64 and arm64 (Arch has no official arm64 image, so it is amd64 only). Every
`package` job runs `scripts/install-build-deps.sh`, `scripts/build-prefix.sh`,
`tests/packaging.py` and `scripts/build-packages.sh`; every `verify` job installs
the result in a clean container of the same image with `scripts/verify-packages.sh`.
A tag also makes a GitHub release with the source archive, the packages, the tars
and checksums.

### Publishing

Packages go to `https://repo.gnoblin.org/bingux/<channel>/`, the host Gnoblin's
repositories use, with the same tool: `scripts/pkgrepo` of the Gnoblin repository at
the commit named in `.github/pkgrepo.ref`. Bingux's CI does not build or modify
Gnoblin. A tag publishes `stable` (and must equal `VERSION ?=` of the Makefile);
`nightly.yml` publishes `nightly`; every other run builds the repository with a
throw-away key and publishes nothing. Publishing needs

| Kind     | Name                                         | What it is                                                      |
| -------- | -------------------------------------------- | --------------------------------------------------------------- |
| secret   | `REPO_GPG_PRIVATE_KEY`                       | The ASCII-armoured private key of the repository                |
| secret   | `REPO_GPG_PASSPHRASE`                        | Its passphrase, when it has one                                 |
| variable | `REPO_BACKEND`                               | `rsync`, `s3` or `pages`                                        |
| variable | `REPO_DESTINATION`                           | `user@host:/srv/repo`, `s3://bucket/prefix` or a Pages Git URL  |
| variable | `REPO_URL`                                   | Public base URL; `https://repo.gnoblin.org` by default          |
| secrets  | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | For `s3` (R2 or MinIO also set the variable `AWS_ENDPOINT_URL`) |
| secrets  | `REPO_RSYNC_KEY`, `REPO_RSYNC_KNOWN_HOSTS`   | For `rsync`                                                     |
| secret   | `REPO_PAGES_TOKEN`                           | For `pages`                                                     |

The header of `package-repository.yml` repeats this list. The variable
`VERIFY_GNOBLIN_REPOSITORY=true` makes the `verify` jobs install `gnoblin` and
`xdg-desktop-portal-gnoblin` from repo.gnoblin.org instead of from stand-in
packages; leave it unset until Gnoblin publishes those packages for Fedora 43 and
Debian 13.

## Files that are not built

`data/` holds the files that are installed as they are, and `scripts/install-shell.py`
and `scripts/install-bsm.sh` put them in place:

| Folder                                           | What it holds                                                                                                                        |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| `data/systemd/`                                  | User units, `bingux.target`, `bingux-effects@.service` and the preset `90-bingux.preset`                                             |
| `data/bsm/`                                      | The units, accounts, directories, D-Bus and polkit policy, PAM services and scripts of bsm; `image/` is for the Bingux OS image only |
| `data/pam/`                                      | The PAM services of the lock screen                                                                                                  |
| `data/polkit/`, `data/tmpfiles/`, `data/greetd/` | The greeter's polkit rules, directories and greetd configuration                                                                     |
| `data/portal/`                                   | The portal backend registration and the D-Bus activation files                                                                       |
| `data/metainfo/`, `data/man/`                    | AppStream files and man pages of the apps and the commands                                                                           |
| `data/search-apps/`                              | Search-app manifests that `bingux-searchd` reads                                                                                     |

The Bingux OS image (`distro/`) still builds its own `bingux` and `bsm` packages
from PKGBUILDs, from the same source archive and the same `data/` files, because its
dependency set and its Gnoblin build differ. It does not use the repository
packages.
