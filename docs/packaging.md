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

| Package           | What it is                                                                                  |
| ----------------- | ------------------------------------------------------------------------------------------- |
| `bingux`          | The whole desktop: the shell and every core service, and the system integration around them |
| `bingux-settings` | The Settings app                                                                            |
| `bingux-files`    | The Files app                                                                               |
| `bingux-preview`  | The Preview app (pictures and PDF documents)                                                |
| `bingux-taxonomy` | The Taxonomy app (a reference window for the UI library, a development tool)                |

**Why this split.** The desktop does not work without its services, so they are one
package: the shell (`bingux-shell`, Quickshell with the Bingux patches, and its QML
plugins), the search and status daemons, the privileged helper, the portal backend
and global shortcuts, the lock screen program, `bingux-config`, the
sound effects engine and the preview helper that Files and Preview run (it links poppler and libheif, so `bingux` depends on them). Each of those is started by the shell, by a
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

**What is not in the packages.** `bingux` is for Fedora, Debian and Arch systems that
log in through GDM or another display manager that is already there. It creates no
account and changes no login manager. The pieces that exist only for the Bingux OS
image (bsm, the login screen and its greetd configuration, the boot splash and the
greeter account) are not in any package of this repository; the image builds them
from `distro/` ([Bingux OS](../distro/README.md), [bsm packaging](bsm/packaging.md)).
`scripts/install-shell.py` installs them only with `--os-image` (`make install
OS_IMAGE=1`), which the PKGBUILD of the image passes and `scripts/build-prefix.sh`
does not, so they never reach the prefix.

### Dependencies

`bingux` depends on `gnoblin`, `xdg-desktop-portal-gnoblin` and
`xdg-desktop-portal`. The first two are published at `https://repo.gnoblin.org`
(see [package management](package-management.md#package-repositories)); Gnoblin's
CI builds them and Bingux's does not. The sound effects plugins, the film decoders
for previews, Settings and Files are recommended.

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
`bingux-preview-host`. `bingux` takes those names over in the way of each format:

| Format      | Fields                                                     |
| ----------- | ---------------------------------------------------------- |
| `rpm`       | `Obsoletes:` and `Provides:` of each old name              |
| `deb`       | `Replaces:`, `Conflicts:` and `Provides:` of each old name |
| `archlinux` | `replaces`, `conflicts` and `provides` of each old name    |

`scripts/verify-packages.sh` installs stand-ins for the old names first and checks
that installing the new packages removes them. `bingux-settings`, `bingux-files`, `bingux-preview` and
`bingux-taxonomy` keep their names, so they upgrade in place.

## Build a prefix

Install the build dependencies, then build. Run this on the distribution you
package for: the Qt plugins and the shell link its private API, and the Rust
programs link its C library.

```sh
sudo scripts/install-build-deps.sh fedora    # or debian, or arch
scripts/build-prefix.sh                      # ./prefix, ./prefix.env
```

`scripts/build-prefix.sh` runs `make all`, installs with `make install`
(`PREFIX=/usr`, `DESTDIR=prefix`), adds the PAM services of the lock screen,
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
| `pkgsmith/apps.star`   | `bingux-settings`, `bingux-files`, `bingux-preview`, `bingux-taxonomy`             |
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
unit, the user presets and the logind session assume systemd and logind, and Alpine ships neither (it
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
puts them in place:

| Folder                        | What it holds                                                                            |
| ----------------------------- | ---------------------------------------------------------------------------------------- |
| `data/systemd/`               | User units, `bingux.target`, `bingux-effects@.service` and the preset `90-bingux.preset` |
| `data/pam/`                   | The PAM services of the lock screen                                                      |
| `data/portal/`                | The portal backend registration and the D-Bus activation files                           |
| `data/metainfo/`, `data/man/` | AppStream files and man pages of the apps and the commands                               |
| `data/search-apps/`           | Search-app manifests that `bingux-searchd` reads                                         |

`data/os/` and `data/bsm/` are for Bingux OS only and are not packaged here; see
[the image's packages](#the-bingux-os-image).

### The Bingux OS image

The Bingux OS image (`distro/`) builds its own `bingux`, `bsm` and `bingux-os`
packages from PKGBUILDs, from the same source archive, because its dependency set
and its Gnoblin build differ. It does not use the repository packages or pkgsmith.
Its `bingux` is built with `make install OS_IMAGE=1`, which adds what the repository
package leaves out: the greeter programs and `gnoblin/greeter.lua`, the greetd,
polkit and tmpfiles files and the wallpaper units for the login screen (`data/os/`).
`bsm` has its own PKGBUILD and `scripts/install-bsm.sh` (`make bsm`,
`make install-bsm`); `bingux-os` holds the image defaults. The same sources therefore
stay in the tree, and nothing for the image is in a package that ordinary
distributions install.

## How it was verified

Run on 2026-11-01 on a Fedora 43 host. pkgsmith was built with `scripts/install-pkgsmith.sh` at `1041e4c`. That run
was of the package as it was before bsm and the login screen files were taken out of `bingux`; the numbers and the account
checks below describe that package. `scripts/verify-packages.sh` now checks that none of those files is installed.

- **Fedora 43.** `make all bsm` and `scripts/build-prefix.sh` made a prefix of 156 files. The scripts wrote rpm and tar for
  `0.1.0` and rpm for `0.1.1-nightly.20261031.142.gabc1234`. Every `%files` entry of the old spec (69) is in the right
  new package. `scripts/verify-packages.sh` ran in a clean `fedora:43` container for both versions.
- **Debian 13 and Arch Linux.** `scripts/install-build-deps.sh` and `scripts/build-prefix.sh` ran in `debian:trixie` (Qt 6.8)
  and `archlinux:base` (Qt 6.12), as CI runs them, and wrote a 156-file prefix each. pkgsmith wrote the deb and the
  Arch packages, and `scripts/verify-packages.sh` ran in a clean container of each image.
- **What `verify-packages.sh` checked, in each:** stand-ins for the replaced packages (and `bsm` on Arch) were installed first and
  were gone after installing the four packages; the exact `bingux` pins resolved; the package manager's own verification
  was clean (`rpm -V`, `dpkg -V`, `pacman -Qkk`); `ldd` found every library of 24 programs; the programs printed usage and the
  shell printed its version; the accounts `bsm` and `bingux-greeter` existed; installing did not change the display
  manager; removal left no file behind. On Fedora the preset enabled `bingux.target`.
- The first Debian run found two missing dependencies (`libqt6widgets6`, `libadwaita-1-0`), which are now in the list.
- `tests/packaging.py` passes (it reads back the rpm, deb and Arch metadata). `actionlint` and the lint hooks pass.

Not run: any arm64 build, the GitHub workflows, publishing, a build with the real `gnoblin` packages (stand-ins were used),
and a login to a session with the packaged shell. There is no Alpine run because there is no apk (see above).
