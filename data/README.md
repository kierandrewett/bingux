# Installed files

Files that are installed as they are: units, policy, PAM services, AppStream
files, man pages and the registration files of the portal backend. They are not
built. `scripts/install-shell.py` (`make install`) and `scripts/install-bsm.sh`
(`make install-bsm`) put them in place, `scripts/build-prefix.sh` collects the
result in a prefix, and `pkgsmith/*.star` split the prefix into packages
([packaging](../docs/packaging.md)).

| Folder         | What it holds                                                                                                            |
| -------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `systemd/`     | User units, `bingux.target`, `bingux-effects@.service` and the preset `90-bingux.preset`                                 |
| `bsm/`         | bsm: units, sysusers, tmpfiles, D-Bus and polkit policy, PAM services, scripts; `image/` is for the Bingux OS image only |
| `pam/`         | The PAM services of the lock screen                                                                                      |
| `polkit/`      | The greeter's polkit rules                                                                                               |
| `tmpfiles/`    | The directories of the login screen                                                                                      |
| `greetd/`      | The greetd configuration of the login screen                                                                             |
| `portal/`      | The portal backend registration and the D-Bus activation files                                                           |
| `metainfo/`    | AppStream files of Settings, Files and Taxonomy                                                                          |
| `man/`         | Man pages: `bingux-settings`, `bingux-files`, `bingux-config`, `bingux-effects`, `bingux-taxonomy`                       |
| `search-apps/` | Search-app manifests that `bingux-searchd` reads                                                                         |
