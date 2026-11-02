# Packaging

Bingux is four packages: the desktop, and the three apps that go with it.

## The packages

| Package           | What it installs                                                                                                                                                                                                             | Install it when                                            |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| `bingux`          | The whole desktop: the shell, every core service (search, status, the lock screen, the portal backend, the sound effects engine, the preview helper Files runs, the `bingux-config` command), `binguxctl` and the user units | You want the Bingux desktop                                |
| `bingux-settings` | Settings                                                                                                                                                                                                                     | You want to change Bingux and system settings in one place |
| `bingux-files`    | Files, the file manager                                                                                                                                                                                                      | You want a file manager that matches the shell             |
| `bingux-taxonomy` | A window that shows every Bingux UI component                                                                                                                                                                                | You build Bingux apps. Ordinary desktops do not need it    |

Installing `bingux` also installs Gnoblin, the compositor it runs on. The apps need `bingux` at the same version.

Bingux changes nothing about how you log in. Your display manager (GDM, SDDM or whichever you use) stays, and the
packages create no accounts.

## Package repositories

Add the Bingux package repository and your package manager installs and updates Bingux. Bingux needs Gnoblin, so
also follow the [Gnoblin install guide](https://gnoblin.org/installation) to add its repository.

The repository has a `nightly` channel, which follows the main branch.

Packages and repository metadata are signed with one key, published at `https://repo.gnoblin.org/gpg.key`. Its
fingerprint is:

```text
91BA A379 0941 B9B3 BBA6  3BC4 F1B8 D899 DF3D 786E
```

Check that the fingerprint your package manager shows matches this one before you accept the key.

### Fedora

```sh
sudo dnf config-manager addrepo --from-repofile=https://repo.gnoblin.org/bingux/nightly/fedora/bingux-nightly.repo
sudo dnf install bingux
```

This also installs Settings and Files. To install the shell alone, run
`sudo dnf install --setopt=install_weak_deps=False bingux`.

### Debian

```sh
sudo install -d -m 0755 /etc/apt/keyrings
sudo curl -fsSL -o /etc/apt/keyrings/bingux.asc https://repo.gnoblin.org/gpg.key
sudo curl -fsSL -o /etc/apt/sources.list.d/bingux-nightly.sources \
  "https://repo.gnoblin.org/bingux/nightly/deb/bingux-nightly-$(. /etc/os-release && echo "$VERSION_CODENAME").sources"
sudo apt update
sudo apt install bingux
```

Check the key fingerprint with `gpg --show-keys /etc/apt/keyrings/bingux.asc` before you run `apt update`.

### Arch Linux

```sh
curl -fsSL -o bingux.key https://repo.gnoblin.org/gpg.key
sudo pacman-key --add bingux.key
sudo pacman-key --lsign-key 91BAA3790941B9B3BBA63BC4F1B8D899DF3D786E
curl -fsSL https://repo.gnoblin.org/bingux/nightly/arch/bingux-nightly.conf | sudo tee -a /etc/pacman.conf
sudo pacman -Syu bingux bingux-settings bingux-files
```

pacman does not install recommended packages. Name the apps you want, as in the command above.

### After installing

Log out, choose **Gnoblin** at the login screen and log in. Bingux starts with the session. The package works after
the install: Gnoblin loads the Bingux integration, `/usr/share/gnoblin/conf.d/bingux.lua`, from its system drop-in
directory, so you add nothing to your Gnoblin configuration.

## Open the apps

- Settings: search for "Settings", or run `bingux-settings`. Open a page with
  `bingux-settings network` or `xdg-open bingux-settings://network`.
- Files: search for "Files", or run `bingux-files [FOLDER]`. To open folders in
  Files from every application:

    ```sh
    xdg-mime default bingux-files.desktop inode/directory
    ```

    You can also choose it under **Settings > Applications > Default applications**.

- Sound effects: turn them on in **Settings > Sound > Effects**. Settings enables
  the user services `bingux-effects@output.service` and
  `bingux-effects@input.service`. Check them with
  `systemctl --user status bingux-effects@output.service`.

## Remove the packages

```sh
# Fedora
sudo dnf remove bingux bingux-settings bingux-files bingux-taxonomy
# Debian
sudo apt remove bingux bingux-settings bingux-files bingux-taxonomy
# Arch Linux
sudo pacman -Rns bingux bingux-settings bingux-files bingux-taxonomy
```

Removing the packages does not delete your data. See [Runtime state](#runtime-state).

## Build from source

To install for your own user from a source checkout, use the [standalone shell](standalone.md) guide. To build the
packages yourself, see [Packaging](packaging.md).

## Runtime state

User configuration belongs under `$XDG_CONFIG_HOME/bingux`. The package owns
only its installed files and user-systemd units. Uninstalling it does not delete
notes, layouts, search indexes, extension settings, notification history, the
network data usage records (`$XDG_DATA_HOME/bingux/data-usage.sqlite3`), sound
effect presets or the settings of Files.
