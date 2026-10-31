# Package repositories

Bingux's packages are published in signed repositories with a `stable` and a
`nightly` channel, at `https://repo.gnoblin.org/bingux/`. The layout, the version
rules, the signing, the retention and the hosting options are the same as for
Gnoblin and are written down once, in
[Gnoblin's design note](https://github.com/kierandrewett/gnoblin/blob/main/design/package-repositories.md).
This page records what is specific to Bingux.

Nothing is published yet. Set up so far: the R2 bucket `gnoblin-repo` behind
`repo.gnoblin.org`, and the signing key (fingerprint in
[package management](../docs/package-management.md#package-repositories)), whose
private key and passphrase are the CI secrets `REPO_GPG_PRIVATE_KEY` and
`REPO_GPG_PASSPHRASE` of this repository. Still missing: the upload credentials
(`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and the variables `REPO_BACKEND`,
`REPO_DESTINATION` and `AWS_ENDPOINT_URL`), and a first run of the workflows.

## What Bingux builds

| Format | Built by                             | Target             | Status                                        |
| ------ | ------------------------------------ | ------------------ | --------------------------------------------- |
| RPM    | `make rpm-package`, `build-rpms.sh`  | Fedora 43, x86_64  | Built and checked on a Fedora 43 host         |
| pacman | `make arch-package`, `build-arch.sh` | Arch Linux, x86_64 | Defined; see the packaging notes for what ran |
| deb    | `make deb-package`, `build-debs.sh`  | Debian 13, x86_64  | See [Debian packages](#debian-packages)       |

Nothing builds for `aarch64`: the PKGBUILD says `arch=('x86_64')` and the CI has
no ARM runner. Ubuntu has no packages because Ubuntu 24.04's Rust is older than
the 1.88 the programs need.

## Versions

Bingux has no release tag yet. `pkgrepo version --git .` then takes the version in
the Makefile (`0.1.0`) as the release that is coming, so the first nightlies are
`0.1.0~nightly.<date>.<count>.g<commit>` (RPM and Debian) and
`0.1.0nightly<date>.<count>` (Arch). After the first tag `v0.1.0`, nightlies are
`0.1.1~nightly...`. `make-source-archive.sh` accepts these spellings, and
`build-rpms.sh` writes the version it is given into a copy of the spec.

## CI

`.github/workflows/packages.yml` builds the three formats and installs the RPMs
and Debian packages on a clean system. `package-repository.yml` signs and
publishes. `nightly.yml` builds `main` daily when the commit in the published
nightly manifest is older. They are separate files from the desktop test
framework's `ci.yml`, which they do not touch.

The tool is `scripts/pkgrepo` of the Gnoblin repository. `packaging/pkgrepo.ref`
pins the commit that CI fetches, so a change to the tool reaches Bingux only when
someone raises the pin. A third repository for the tool would replace the one line
of that file and the `repository:` field of the workflows.

## Debian packages

`packaging/debian` is a Debian source package that splits `make install` into the
same seven packages as the spec and the PKGBUILD. `tests/rpm-package.py` checks
that the package list, the file lists and the PKGBUILD agree. `scripts/build-debs.sh`
builds it with `dpkg-buildpackage` in a `debian:trixie` container.

Built on 2026-10-31 in `debian:trixie` (log kept by the author of this change, not in the
repository):

- All seven packages built from commit `c316dbe8` plus these changes, with version
  `0.1.0~nightly.20261031.1692.gc316dbe-1`.
- They were signed into a nightly repository with the throw-away test key and installed
  from it in a clean Debian 13 container. `dpkg --verify` reported every file intact.
- Not checked: that the shell starts. That needs a desktop session.

Two findings from the build:

- The programs use `if let` chains, which need Rust 1.88. Debian 13 ships 1.85, and
  `Cargo.toml` says `rust-version = "1.85"`, which the code does not meet. The build takes
  Rust from `trixie-backports` (`scripts/debian-build-deps.sh`).
- The Makefile at `c316dbe8` could not run: a merge had turned `CARGO_TARGET ?=` into a
  self-reference. It is fixed in its own commit.

The Debian dependency names were chosen by hand from the RPM `Requires`; the clean
install shows they resolve.
