# Bingux RPMs

One source package, `bingux`, builds six binary packages (and their debuginfo):

| Package | Contents |
| --- | --- |
| `bingux` | Shell, search and status daemons, `binguxctl`, helpers, QML plugins, Gnoblin drop-in, user units and preset |
| `bingux-config` | `bingux-config` |
| `bingux-settings` | Settings: program, desktop entry, icon, metainfo, man page, search-app manifest |
| `bingux-files` | Files: program, desktop entry, icon, metainfo, man page |
| `bingux-audio-effects` | `bingux-effects`, the LADSPA library `libbingux_dsp.so`, the `bingux-effects@.service` template |
| `bingux-taxonomy` | The Bingux UI reference window |

The spec does not install Gnoblin; Gnoblin is a separate package and session
requirement (`Recommends: gnoblin`).

Build every package from the source archive:

```sh
make rpm-package
```

The command writes the packages to `/var/tmp/BinguxPkg/rpmbuild`; see
[releases](../../docs/release.md) for the options and the release steps. To check the
spec, desktop entries, AppStream files and man pages without building:

```sh
make rpm-check
```

The spec has no scriptlets for the desktop database, the MIME database or the
icon cache: Fedora's file triggers (`desktop-file-utils`, `shared-mime-info`,
`gtk-update-icon-cache`) handle them. Its only scriptlets are the systemd user
unit macros. `bingux.target` is enabled through
`%{_userpresetdir}/90-bingux.preset`.

The COPR project is `kierandrewett/bingux`. Submit a source package only after
the local RPM build and the clean-install check pass. The repository does not
claim a ready package while those checks are pending.

The runtime package must provide `/usr/bin/qs` from a Quickshell 0.2.1 build
using the host's Qt 6 ABI. The Bingux RPM deliberately depends on that file,
not on a repository-specific Quickshell package name.

Installed packages are removed with `sudo dnf remove bingux`; the RPM
scriptlets handle the user-systemd units during removal and upgrade.
