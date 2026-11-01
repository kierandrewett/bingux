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

pkgsmith writes every package from a prefix that Bingux builds into a folder
([packaging](../docs/packaging.md)). The prefix is built in a container of each
distribution, because the shell links that distribution's Qt.

| Format | Built by                                       | Target                        | Status                                        |
| ------ | ---------------------------------------------- | ----------------------------- | --------------------------------------------- |
| RPM    | `make prefix`, `make packages`, `packages.yml` | Fedora 43, x86_64 and aarch64 | Defined; see the packaging notes for what ran |
| pacman | the same                                       | Arch Linux, x86_64            | Defined; see the packaging notes for what ran |
| deb    | the same                                       | Debian 13, amd64 and arm64    | Defined; see the packaging notes for what ran |

There is no `apk`: Bingux needs systemd, and Alpine has none
([why](../docs/packaging.md#why-there-is-no-apk)). Arch Linux has no official
arm64 image, so Arch is x86_64 only. Ubuntu has no packages because Ubuntu 24.04's
Rust is older than the 1.88 the programs need.

## Versions

Bingux has no release tag yet. `pkgrepo version --git .` then takes the version in
the Makefile (`0.1.0`) as the release that is coming, so the first nightlies are
`0.1.0~nightly.<date>.<count>.g<commit>` (RPM and Debian) and
`0.1.0nightly<date>.<count>` (Arch). After the first tag `v0.1.0`, nightlies are
`0.1.1~nightly...`. `packages.yml` hands pkgsmith the version in its own spelling
(`0.1.1-nightly.20261031.142.gabc1234`), and pkgsmith writes each format's
spelling. `make-source-archive.sh` accepts these spellings too.

## CI

`.github/workflows/packages.yml` builds pkgsmith, then a prefix and the packages in
a container of each distribution, installs them in a clean container and hands them
to `package-repository.yml`, which signs and publishes. `nightly.yml` builds `main`
daily when the commit in the published nightly manifest is older. They are separate
files from the desktop test framework's `ci.yml`, which only runs the static
checks of the package definitions.

The tool is `scripts/pkgrepo` of the Gnoblin repository. `.github/pkgrepo.ref`
pins the commit that CI fetches, so a change to the tool reaches Bingux only when
someone raises the pin. A third repository for the tool would replace the one line
of that file and the `repository:` field of the workflows. pkgsmith is pinned the
same way in `.github/pkgsmith.ref`.

Bingux CI and Gnoblin CI are separate. The `bingux` package depends on `gnoblin` and
`xdg-desktop-portal-gnoblin` as names; Bingux builds none of Gnoblin.
