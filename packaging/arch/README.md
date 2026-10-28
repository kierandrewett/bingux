# Bingux Arch packages

`PKGBUILD` builds six packages from one source archive: `bingux`,
`bingux-config`, `bingux-settings`, `bingux-files`, `bingux-audio-effects` and
`bingux-taxonomy`. The contents match the RPMs (see `../rpm/README.md`).

`build()` runs `make install` once into a stage; each `package_*()` moves its
files out of the stage and `package_bingux()` takes what is left. A file that
`install-shell.py` starts installing therefore lands in the shell package until
you give it a package of its own.

Build:

```sh
make arch-package
```

On a host without `makepkg` this runs `makepkg` in an `archlinux` container with
`podman` or `docker`. The packages land in `/var/tmp/BinguxPkg/arch/out`.

The checked-in `PKGBUILD` has `sha256sums=('SKIP')`: it is a template. The build
script copies it next to the source archive and writes the archive's checksum
into the copy. Do not publish a package built from a `SKIP` checksum.

`bingux.install` applies the user preset so that `bingux.target` is enabled for
every user, as the RPM does. Arch has no weak dependencies, so the shell lists
Settings and Files as optional dependencies; Bingux OS names them in its package
list. Bingux OS (`~/dev/bingux-os`, `distro/`) is built from PKGBUILDs of this
shape; see [Bingux userspace](../../design/userspace.md#5-image-changes-devbingux-os).
