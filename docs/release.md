# Releases

This page is for maintainers. To install Bingux, see
[package management](package-management.md). The contents of each package and
the open packaging decisions are in [Bingux userspace](../design/userspace.md).

A release is one version number, one source archive and the same version in
every package: `bingux`, `bingux-config`, `bingux-settings`, `bingux-files`,
`bingux-audio-effects` and `bingux-taxonomy`.

## Cut a release

1. Start from a clean checkout of the commit to release. `make source-archive`
   refuses a tree with changes.
2. Set the version everywhere it is written:

   | File | What to change |
   | --- | --- |
   | `Makefile` | `VERSION ?=` |
   | `packaging/rpm/bingux.spec` | `Version:`, and a new `%changelog` entry |
   | `packaging/arch/PKGBUILD` | `pkgver=`, and `pkgrel=1` |
   | `{apps,crates,services,bsm}/*/Cargo.toml` | `version =` of every crate, then `cargo update -w --offline` in each |
   | `packaging/metainfo/*.metainfo.xml` | a new `<release version= date=>` first in `<releases>` |
   | `packaging/man/*.1` | the version and date in the `.TH` line |

   `make rpm-check` fails when the Makefile, the spec, the PKGBUILD, the metainfo
   files and the man pages disagree.
3. Run the checks that need no build: `make rpm-check`. Then run
   `python3 scripts/cargo-licenses.py --summary` and update the `License:` tags
   of the spec and the `license=` arrays of the PKGBUILD if the dependency set
   changed; `make rpm-check` compares the spec.
4. Commit, tag and push:

   ```sh
   git commit -am "release: 0.2.0"
   git tag -s v0.2.0 -m "Bingux 0.2.0"
   git push origin main v0.2.0
   ```

5. Build the source archive from the tag:

   ```sh
   make source-archive VERSION=0.2.0
   sha256sum dist/bingux-0.2.0.tar.xz
   ```

   The archive is reproducible: the same commit gives the same bytes. It holds
   the tracked tree and the pinned Quickshell submodule, which
   `git archive` does not follow.
6. Build and check the packages, below.
7. Publish the packages and attach the archive, checksums, the source RPM and
   the RPMs to a GitHub release.

## Fedora

```sh
make rpm-package VERSION=0.2.0
```

The command builds the archive, runs `rpmbuild --nodeps` and leaves six RPMs, the
debuginfo and debugsource packages and a source RPM in
`/var/tmp/BinguxPkg/rpmbuild`. It uses the toolchain of the host, so build on a
Fedora 43 machine or in a `fedora:43` container with the spec's `BuildRequires`
installed. `BINGUX_RPM_CHECK_DEPS=1` makes `rpmbuild` check them. A build needs
the network for Rust crates and the prebuilt Skia archive, and the build tree
needs about 5 GB. `CARGO_TARGET=/some/dir` reuses one Cargo target between
builds, and `JOBS=n` limits parallelism.

Check the result before publishing:

```sh
rpm -qlp /var/tmp/BinguxPkg/rpmbuild/RPMS/x86_64/bingux-settings-*.rpm
rpm -qp --requires /var/tmp/BinguxPkg/rpmbuild/RPMS/x86_64/bingux-settings-*.rpm
rpmlint /var/tmp/BinguxPkg/rpmbuild/RPMS/x86_64/*.rpm
```

Then install the packages on a clean Fedora system, log in to a Gnoblin session
and open Settings and Files. `TODO.md` lists the clean-install check as a release
blocker.

To publish, submit the source RPM to the COPR project:

```sh
copr-cli build kierandrewett/bingux /var/tmp/BinguxPkg/rpmbuild/SRPMS/bingux-0.2.0-1.*.src.rpm
```

COPR needs network access in the builds (the Rust crates, and Skia unless the
spec builds it): enable it in the project settings.

## Arch Linux and Bingux OS

```sh
make arch-package VERSION=0.2.0
```

The command builds the archive, writes its checksum into a copy of
`packaging/arch/PKGBUILD` and runs `makepkg` in an `archlinux` container (or
directly on Arch). It leaves the packages in `/var/tmp/BinguxPkg/arch/out`.

Bingux OS builds its pacman repository in `distro/` of the image repository.
After the packages are merged there (see
[the image changes](../design/userspace.md#5-image-changes-devbingux-os)):

```sh
distro/build.sh --step packages      # builds every package into distro/out/repo
distro/publish-repo.sh               # uploads the repository to the pacman-x86_64 release
```

The repository is unsigned today: the pacman configuration uses
`SigLevel = Optional TrustAll`. Sign the packages and the database before you
point real machines at it.

## CI

The repository has no workflows yet. These are the jobs a workflow needs, each
with the command it runs:

| Job | Runs in | Command | When |
| --- | --- | --- | --- |
| Package definitions | any container with Python and `desktop-file-utils` and `appstream` | `make rpm-check` | every pull request |
| RPM build | `fedora:43` with `rpm-build` and the spec's `BuildRequires` | `make rpm-package` | every pull request and tag |
| RPM install | a clean `fedora:43` | `dnf install ./RPMS/x86_64/*.rpm`, then `rpm -V` and `desktop-file-validate /usr/share/applications/bingux-*.desktop` | after the RPM build |
| Arch build | `archlinux` | `make arch-package` | every pull request and tag |
| Release | any | upload the archive, checksums, source RPM, RPMs and Arch packages to the GitHub release of the tag | tag `v*` |

Each build job should upload `/var/tmp/BinguxPkg` as an artifact. The Rust and
Skia build takes tens of minutes; cache the Cargo registry and a target
directory (`CARGO_TARGET`).

## Verification record

The packaging in this change was built and checked on 2026-10-28. The commands,
what they printed and what could not be run are in
[the packaging notes](../packaging/README.md#verification-record).
