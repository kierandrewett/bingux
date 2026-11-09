# Installed files

Files that are installed as they are: units, policy, PAM services, AppStream
files, man pages and the registration files of the portal backend. They are not
built. `scripts/install-shell.py` (`make install`) puts them in place,
`scripts/build-prefix.sh` collects the result in a prefix, and `pkgsmith/*.star`
split the prefix into packages ([packaging](../docs/packaging.md)). The packages are
for Fedora, Debian and Arch systems that keep their own display manager. What only the
Bingux OS image needs is in `os/` and `bsm/`: `make install OS_IMAGE=1` and
`scripts/install-bsm.sh` (`make install-bsm`) install it, and no package of the
repository does.

| Folder         | What it holds                                                                                                          |
| -------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `systemd/`     | User units, `bingux.target`, `bingux-effects@.service` and the preset `90-bingux.preset`                               |
| `pam/`         | The PAM services of the lock screen                                                                                    |
| `os/`          | Bingux OS only: the greetd configuration, the greeter's polkit rules and directories, the login-screen wallpaper units |
| `bsm/`         | Bingux OS only: bsm units, sysusers, tmpfiles, D-Bus and polkit policy, PAM services, scripts; `image/` is image-only  |
| `portal/`      | The portal backend registration and the D-Bus activation files                                                         |
| `metainfo/`    | AppStream files of Settings, Files and Taxonomy                                                                        |
| `man/`         | Man pages: `bingux-settings`, `bingux-files`, `bingux-calendar`, `bingux-config`, `bingux-effects`, `bingux-taxonomy`  |
| `search-apps/` | Search-app manifests that `bingux-searchd` reads                                                                       |
