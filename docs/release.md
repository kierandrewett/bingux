# Releases

This page is for maintainers. To install Bingux, see
[package management](package-management.md). The contents of each package and
the open packaging decisions are in [Bingux userspace](../design/userspace.md).

A release is one version number, one source archive and the same version in
every package: `bingux`, `bingux-settings`, `bingux-files`, `bingux-calendar` and `bingux-taxonomy`.
How the packages are made is in [packaging](packaging.md).

## Cut a release

1. Start from a clean checkout of the commit to release. `make source-archive`
   refuses a tree with changes.
2. Set the version everywhere it is written:

    | File                                      | What to change                                                       |
    | ----------------------------------------- | -------------------------------------------------------------------- |
    | `Makefile`                                | `VERSION ?=`                                                         |
    | `pkgsmith/common.star`                    | the default of `arg("VERSION", ...)`                                 |
    | `{apps,crates,services,bsm}/*/Cargo.toml` | `version =` of every crate, then `cargo update -w --offline` in each |
    | `data/metainfo/*.metainfo.xml`            | a new `<release version= date=>` first in `<releases>`               |
    | `data/man/*.1`                            | the version and date in the `.TH` line                               |

    `make package-check` fails when the Makefile, the script default, the metainfo
    files and the man pages disagree.

3. Run the checks that need no build: `make package-check`. Then run
   `python3 scripts/cargo-licenses.py --summary` and update the `LICENSES` table of
   `pkgsmith/common.star` if the dependency set changed; `make package-check`
   compares it.
4. Commit, tag and push:

    ```sh
    git commit -am "release: 0.2.0"
    git tag -s v0.2.0 -m "Bingux 0.2.0"
    git push origin main v0.2.0
    ```

5. Build the source archive from the tag if you want to publish it by hand:

    ```sh
    make source-archive VERSION=0.2.0
    sha256sum dist/bingux-0.2.0.tar.xz
    ```

    The archive is reproducible: the same commit gives the same bytes. It holds
    the tracked tree and the pinned Quickshell submodule, which
    `git archive` does not follow.

6. The tag starts `packages.yml`: it builds the packages for Fedora 43, Debian 13
   and Arch Linux, installs them in clean containers, publishes the stable
   repository channel and makes a GitHub release with the source archive, the
   packages, the tars and the checksums (see [CI](#ci)). Build and check the
   packages by hand as described in [packaging](packaging.md#build-a-prefix) when
   you want to see them first.

Then install the packages on a clean system, log in to a Gnoblin session and open
Settings and Files. `TODO.md` lists the clean-install check as a release blocker.

## Bingux OS

Bingux OS builds its pacman repository in `distro/` of the image repository, from
its own PKGBUILDs (`distro/packages/`):

```sh
distro/build.sh --step packages      # builds every package into distro/out/repo
distro/publish-repo.sh               # uploads the repository to the pacman-x86_64 release
```

The repository is unsigned today: the pacman configuration uses
`SigLevel = Optional TrustAll`. Sign the packages and the database before you
point real machines at it.

## Package repositories

A signed repository for each package format has a `stable` and a `nightly`
channel. The tool that builds and signs it is `scripts/pkgrepo` of the Gnoblin
repository; `.github/pkgrepo.ref` names the commit CI fetches. Raise it when
the tool changes. [Package management](package-management.md#package-repositories)
tells users how to add a repository, and
[the design note](../design/package-repositories.md) has the layout, the version
rules and the key handling.

## CI

The workflows in `.github/workflows` do what the steps above do by hand:

| Workflow                 | Runs                                                            | What it does                                                                                                                                                                                                                       |
| ------------------------ | --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `packages.yml`           | a tag `v*`, a pull request that changes packaging, a manual run | Builds pkgsmith, builds a prefix and writes the packages in Fedora 43, Debian 13 and Arch containers (amd64 and arm64), installs them in clean containers, hands them to the repository stage, and on a tag makes a GitHub release |
| `package-repository.yml` | called by `packages.yml`                                        | Signs the packages and the metadata, merges with the published channel, verifies the tree and uploads it                                                                                                                           |
| `nightly.yml`            | daily, and on demand                                            | Builds a nightly version of `main` when `main` changed since the published nightly, and publishes the nightly channel                                                                                                              |

A tag publishes the stable channel and must equal `VERSION ?=` in the Makefile.
Any other run builds and verifies the repositories with a throw-away key and
publishes nothing. Publishing needs the secrets `REPO_GPG_PRIVATE_KEY` and
`REPO_GPG_PASSPHRASE`, the variables `REPO_BACKEND` and `REPO_DESTINATION`, and
the credentials of the back end; [packaging](packaging.md#publishing) and the header
of `package-repository.yml` list them all. A fork without them only builds.

The Rust and Skia build takes tens of minutes per job. The workflows have not
run on GitHub yet; [packaging](packaging.md) says what was run locally.
