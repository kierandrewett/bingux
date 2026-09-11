# Bingux RPM

The spec builds the native shell, its QML plugins, the search and status
daemons, helper programs and user-systemd units. It does not install Gnoblin;
Gnoblin is a separate package and session requirement.

Build a source archive from a clean checkout:

```sh
make source-archive VERSION=0.1.0
rpmbuild -tb dist/bingux-0.1.0.tar.xz
```

The COPR project is `kierandrewett/bingux`. Submit a source package only after
the local RPM build and staged install checks pass. The repository does not
claim a ready package while those checks are pending.

The runtime package must provide `/usr/bin/qs` from a Quickshell 0.2.1 build
using the host's Qt 6 ABI. The Bingux RPM deliberately depends on that file,
not on a repository-specific Quickshell package name.

Installed packages are removed with `sudo dnf remove bingux`; the RPM
scriptlets handle the user-systemd units during removal and upgrade.
