# Packaging

| Path | What it holds |
| --- | --- |
| `rpm/bingux.spec` | One source package, six RPMs (`rpm/README.md`) |
| `arch/PKGBUILD`, `arch/bingux.install` | The same six packages for Arch Linux and Bingux OS (`arch/README.md`) |
| `metainfo/` | AppStream files of Settings, Files and Taxonomy |
| `man/` | Man pages: `bingux-settings`, `bingux-files`, `bingux-config`, `bingux-effects`, `bingux-taxonomy` |
| `systemd/` | User units, `bingux.target`, `bingux-effects@.service` and the preset `90-bingux.preset` |
| `search-apps/` | Search-app manifests that `bingux-searchd` reads |
| `gnoblin/`, `pam/` | The Gnoblin drop-in and the lock screen PAM service |

`scripts/install-shell.py` (`make install`) is the single list of what gets
installed; the spec and the PKGBUILD only split it into packages. The contents,
the open decisions and the image changes are in
[Bingux userspace](../design/userspace.md); how to cut a release is in
[releases](../docs/release.md).

Checks and builds:

```sh
make rpm-check      # spec, units, desktop entries, AppStream, man pages, licence tags
make rpm-package    # all RPMs, into /var/tmp/BinguxPkg/rpmbuild
make arch-package   # all Arch packages, into /var/tmp/BinguxPkg/arch/out
```

## Build notes

- `bingux-dsp` is built with `panic = "abort"`, the programs with `unwind`.
  Building both in one Cargo target directory fails the link of
  `bingux-settings` ("the crate `libc` requires panic strategy `abort`"), so the
  Makefile builds the plugin library in `$(DSP_TARGET)` and copies it next to
  the other programs.
- The Rust flags of the spec use line-table debug info so the debuginfo
  packages of the statically linked Freya programs stay a usable size.
- `make` passes `CFLAGS` and `LDFLAGS` to the C helpers, so distribution
  hardening flags (PIE, full RELRO) apply to them.
- `CARGO` overrides the cargo program and `CARGO_TARGET` the target directory;
  `make rpm-package` passes both through to `rpmbuild`.

## Verification record

Built and checked on 2026-10-28 on a Fedora 43 host (`rpmbuild --nodeps`, host
toolchain, `BINGUX_ARCHIVE_WORKTREE=1`: the archive held uncommitted work).

**RPM build.** `make rpm-package` ended with `exit=0` and wrote the source RPM,
six RPMs and seven debuginfo/debugsource RPMs. `rpmbuild` ran its
unpackaged-file, buildroot and `%check` steps (desktop-file-validate and
appstreamcli on every entry).

**Contents** (`rpm -qlp`, abbreviated):

```text
bingux-settings   /usr/bin/bingux-settings  /usr/share/applications/bingux-settings.desktop
                  /usr/share/icons/hicolor/scalable/apps/bingux-settings.svg
                  /usr/share/metainfo/dev.drewett.bingux.Settings.metainfo.xml
                  /usr/share/man/man1/bingux-settings.1.gz
                  /usr/share/bingux/search-apps/bingux-settings.json  + doc, licences
bingux-files      /usr/bin/bingux-files  /usr/share/applications/bingux-files.desktop  icon, metainfo, man page
bingux-taxonomy   /usr/bin/bingux-taxonomy  desktop entry, icon, metainfo, man page
bingux-config     /usr/bin/bingux-config  /usr/share/man/man1/bingux-config.1.gz
bingux-audio-effects  /usr/bin/bingux-effects  /usr/lib64/ladspa/libbingux_dsp.so
                  /usr/lib/systemd/user/bingux-effects@.service  man page
bingux            /usr/bin/bingux*  /usr/lib64/bingux/qml  /usr/libexec/bingux  /usr/share/bingux/{shell,icons,gnoblin}
                  /usr/lib/systemd/user/bingux*.service, bingux.target  /usr/lib/systemd/user-preset/90-bingux.preset
                  /usr/share/gnoblin/conf.d/bingux.lua  /etc/pam.d/bingux-lock
```

**Requires** (`rpm -qp --requires`, `bingux-settings`): `bingux-config = 0.1.0-1`,
`dconf`, `fontconfig`, `gsettings-desktop-schemas`, `hicolor-icon-theme`,
`mesa-libEGL`, `mesa-dri-drivers`, `shared-mime-info`, `systemd`, `xdg-utils`,
`/usr/bin/gio`, `/usr/bin/gsettings`, `libwayland-client.so.0()(64bit)`,
`libwayland-egl.so.1()(64bit)`, `libxkbcommon.so.0()(64bit)` and the sonames rpm
finds (`libEGL`, `libGL`, `libfontconfig`, `libfreetype`).

**rpmlint 2.8.0** (Fedora 43 container): 0 errors, 32 warnings. They are: 15
`no-manual-page-for-binary` (the shell's wrappers and daemons have no man page),
about 15 `files-duplicate` (identical SVG icons in the shell's own icon folder), `position-independent-executable-suggested` for `bingux-frame` (its
`packages/bingux-frame/build.sh` ignores `CFLAGS`), `invalid-url Source0` (the
source archive is made locally) and the `setlocale` noise of the bare container.

**desktop-file-validate**: exit 0; one hint: `bingux-taxonomy.desktop` has two main
categories (`Development;Utility;`).

**appstreamcli validate-tree --no-net** on the extracted packages: "Validation was
successful: pedantic: 3" (the pedantic note is `cid-contains-uppercase-letter`).

**Throwaway root.** `unshare -r rpm --root ROOT --dbpath /usr/lib/sysimage/rpm -ivh --nodeps`
installed all six packages; `rpm -V --nodeps --noscripts` printed nothing. (The
shell's `%post` reported exit 127 because the empty root has no `/bin/sh`.)

**Fedora 43 guest** (Gnoblin QEMU guest from `qemu-e2e`, a child overlay under
`/var/tmp`): `dnf install` of `bingux-config`, `bingux-settings`, `bingux-files`,
`bingux-audio-effects` and `bingux-taxonomy` resolved with no extra packages. The
file triggers wrote `inode/directory=bingux-files.desktop` and the three
`x-scheme-handler/bingux-*` entries into `mimeinfo.cache`; `gio mime` listed
them; `appstreamcli get dev.drewett.bingux.Settings` found the component;
`bingux-effects scan` listed the `bingux_dsp` plugins from `/usr/lib64/ladspa`;
`rpm -V` was clean. Settings (`bingux-settings network`), Files
(`bingux-files ~/Documents`) and Taxonomy opened in the Gnoblin session with
empty logs (screenshots: `/var/tmp/BinguxPkg/shots/`), and `xdg-open` on a folder
opened Files. `dnf remove` left no `bingux` files and cleaned `mimeinfo.cache`.
The shell package `bingux` was **not** installed in the guest: it requires
`/usr/bin/qs` (Quickshell), which Fedora does not ship. The guest was stopped
afterwards.

**Arch.** In an `archlinux` container, `makepkg --printsrcinfo`, checksum and
extraction passed; the staging step of `build()` ran `make install` with the
binaries of the RPM build; `makepkg --repackage` ran every `package_*()` function and
produced the six packages (plus `bingux-debug`). Every dependency name resolves
in the Arch repositories except `gnoblin` (it comes from `[bingux-os]`).
`namcap` reports: no empty directories; `ELF file outside of a valid path` for
`/usr/libexec/bingux` (the image's existing convention); uncommon licence
identifiers need per-licence files (the packages ship `COPYING` and a
`*.LICENSE.dependencies` file with all crate licence texts); "Split PKGBUILD needs
additional makedepends". **Not run:** a full compile on Arch (`make arch-package`
end to end) and an install of the Arch packages.

**Not run:** a clean mock/COPR build, an install on a real Fedora host with the shell
package, a login to a Bingux session with the packaged shell.
