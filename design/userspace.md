# Bingux userspace: base, app model and packaging

Contributor document. It records how Bingux userspace is put together today,
what the packages contain, and which decisions are still open. Everything here
was read from the repositories on 2026-10-28 unless it is marked `[INFERENCE]`.
The user-facing instructions are in [package management](../docs/package-management.md);
how to cut a release is in [releases](../docs/release.md).

## Summary

- Bingux is a **desktop shell plus apps**, not a distribution. The shell
  (`bingux`) runs on Gnoblin, a Mutter-based compositor with its own session.
  **Bingux OS** is the product image: Arch Linux packages, `greetd`, the Gnoblin
  greeter and the Bingux packages. Fedora is the other target: an RPM spec and a
  planned COPR project, and the QEMU test guest is Fedora 43.
- Both targets are built from one source archive and one `make install`
  staging tree. Each package format only decides which staged files go into
  which package. This change splits the single `bingux` package into six
  packages in both formats (RPM spec, Arch PKGBUILD).
- Apps are plain Wayland programs with a desktop entry, an icon, AppStream
  metainfo and a man page. They install under `/usr`, follow XDG for their data,
  and the only daemons are the search and status helpers (user units under
  `bingux.target`) and the optional sound effects engine (one user unit per
  direction, started from Settings).
- The Freya UI toolkit, Skia and every other Rust dependency are compiled into
  each app binary. There are no shared Bingux libraries to package:
  `bingux-ui` is a library crate, not a package.
- Not decided: eleven questions at the end, each with the conservative default
  that this change applies.

## 1. The base

### 1.1 What exists

| Item | Bingux OS (Arch) | Fedora |
| --- | --- | --- |
| Where it is defined | `~/dev/bingux-os`, `distro/` on branch `feat/bsm` | `packaging/rpm/bingux.spec` here and in `~/dev/bingux` |
| Package format | Arch packages built by `makepkg` from `PKGBUILD`s in `distro/packages/` (`gnoblin`, `bingux`, `bingux-os`, `bsm`, `bingux-installer`, `python-blivet`) | RPM; the Bingux COPR project `kierandrewett/bingux` is named but not published (`TODO.md`: "Publish the first COPR build only after the clean install check passes") |
| Repository | pacman repository `[bingux-os]` published as GitHub release assets `https://github.com/kierandrewett/bingux/releases/download/pacman-x86_64`, unsigned (`SigLevel = Optional TrustAll`), by `distro/publish-repo.sh` | COPR (planned); Gnoblin has its own COPR `kierandrewett/gnoblin` |
| Image | `distro/build.sh` builds the packages and an archiso live image in an `archlinux` container; `bingux-os-install` (archinstall based) copies it to disk | none; the QEMU guest (`scripts/qemu-e2e` in Gnoblin) is a Fedora 43 golden image with Gnoblin and Bingux added |
| Init | systemd (system and user managers) | systemd |
| Login | `greetd` starts `bingux-greeter-session`: a Gnoblin greeter session with the Bingux login screen | GDM (autologin in the test guest); the Gnoblin RPM ships `/usr/share/wayland-sessions/gnoblin.desktop` |
| Session | Gnoblin (`XDG_CURRENT_DESKTOP=Gnoblin`), systemd user target `gnoblin-session.target` | same |
| Image defaults | package `bingux-os`: greetd config, `/etc/skel` Gnoblin `init.lua`, wallpaper, Plymouth theme, os-release, repository config, `systemctl --global enable bingux.target` | none (the user installs the packages) |

Other formats and what happened to them:

- **Nix/NixOS.** Bingux began as a NixOS distribution (April 2026) and its
  `flake.nix`, `hosts/` and `profiles/` were removed on 2026-09-10 (`82ffd4b`,
  "remove distro configuration from bingux"). Gnoblin still ships a flake
  package; Bingux does not.
- **Flatpak.** Not used. The apps drive host services (`nmcli`, `bluetoothctl`,
  `systemctl --user`, D-Bus system services, `gsettings`), so a sandbox would
  need broad holes. `docs/package-management.md` says Flatpak policy is not
  part of Bingux.
- **Debian/Ubuntu.** Not packaged ("APT and pacman packages are not published
  yet" in `docs/package-management.md`, which this change updates).

### 1.2 Two lines of history

`feat/bingux-os` (Arch image, `distro/`, installer, `bsm` session manager) and
`feat/settings-app` (the Freya apps and `bingux-ui`) have diverged: the merge base
is `10b98f03`, `feat/settings-app` is 140 commits ahead and `feat/bingux-os` 17.
The image branch's `bingux` PKGBUILD already builds `bingux-settings` from the
source archive, but it does not contain Files, Taxonomy, the sound effects
engine or the split packages. Its shell is also built differently: one
`bingux-shell` binary with a vendored Quickshell, where this branch requires
`/usr/bin/qs`. The packaging here is written for this branch's layout and does
not need the image branch to adopt anything except the merge; section 5 lists
the changes the image needs after it.

### 1.3 Decision: one stage, two formats, same version

- `make install PREFIX=/usr DESTDIR=...` (`scripts/install-shell.py`) is the single
  description of what the software installs. `packaging/rpm/bingux.spec` and
  `packaging/arch/PKGBUILD` only choose which staged files belong to which
  package, so a new file cannot be installed by one format and forgotten by the
  other. RPM fails the build on an unclaimed file; the PKGBUILD gives the
  leftovers to the shell package.
- Every package has the same version (`VERSION` in the `Makefile`, `Version:` in
  the spec, `pkgver` in the PKGBUILD, `version` in each `Cargo.toml`).
  `tests/rpm-package.py` checks the first three. Packages depend on each other
  with exact versions (`= %{version}-%{release}`) wherever they share a private
  interface (`bingux-config`'s schema).
- Release flow, repositories and signing are in [releases](../docs/release.md).

## 2. The app model

### 2.1 Packages

`bingux-ui` and the other crates under `packages/` that are not listed below are
**libraries**: they are linked into the programs and are never installed.
Confirmed from the manifests: `bingux-ui`, `bingux-files` (the filesystem model
shared by the file chooser and Files), `bingux-search`, `bingux-data-usage`
(`rusqlite` with bundled SQLite) and `bingux-dsp` as an rlib are `[lib]` crates;
`bingux-files-app` is the Files program (its crate is `bingux-files-app`, its
binary and desktop id are `bingux-files`).

| Package | Contents | Why separate |
| --- | --- | --- |
| `bingux` | The shell: Quickshell launchers (`bingux`, `bingux-search-ui`, `bingux-switcher-ui`, `bingux-capture-ui`, `bingux-emoji-ui`, `bingux-popouts`, `bingux-wallpaper`, `bingux-lock`), `bingux-searchd`, `bingux-statusd`, `binguxctl`, helpers, QML plugins, the Gnoblin drop-in, user units and preset, the `bingux-lock` PAM service, the bundled Adwaita symbolic icons | The desktop itself. Pulls in Qt and Quickshell |
| `bingux-config` | `bingux-config` command and its man page | Needed by the shell, Settings and by hand; has no GUI dependencies |
| `bingux-settings` | Settings, desktop entry, icon, metainfo, man page, search-app manifest | An app a user may remove, replace or install on another desktop |
| `bingux-files` | Files, desktop entry, icon, metainfo, man page | Same |
| `bingux-preview-host` | The helper process Files runs to preview films, songs and PDFs: GStreamer decoding and poppler rendering behind a socket, so a damaged file crashes it and not the file manager | Files links neither library. `bingux-files` recommends it; without it those previews show the file's card and Open. It needs GStreamer base and good plugins and poppler-glib, and recommends libav, bad-free and openh264 for more codecs |
| `bingux-audio-effects` | `bingux-effects` engine, `libbingux_dsp.so` LADSPA plugins, the `bingux-effects@.service` template, man page | Needs PipeWire, not the shell. The plugin library is usable from any LADSPA host |
| `bingux-taxonomy` | The Bingux UI reference window | A developer tool: nobody needs it on an ordinary desktop |

Names are the program names. The RPM subpackage names and the Arch package
names are identical.

`bingux` **recommends** Settings and Files on Fedora (weak dependencies are
installed by default, so `dnf install bingux` brings both). Arch has no weak
dependencies: `bingux` lists them as `optdepends`, and the image's package list
names them explicitly (section 5).

### 2.2 Where files go

| Kind | Path | Owner package |
| --- | --- | --- |
| Programs | `/usr/bin/bingux-settings`, `bingux-files`, `bingux-preview-host`, `bingux-taxonomy`, `bingux-effects`, `bingux-config` | the package of that name |
| Shell launchers and daemons | `/usr/bin/bingux*`, `/usr/bin/binguxctl` | `bingux` |
| Private programs | `/usr/libexec/bingux/` (`bingux-frame`, `bingux-image-clipboard`, `search-service`, `binguxctl.py`) | `bingux` |
| QML plugins | `/usr/lib64/bingux/qml` (Fedora), `/usr/lib/bingux/qml` (Arch) | `bingux` |
| Shell files | `/usr/share/bingux/shell`, `/usr/share/bingux/gnoblin`, `/usr/share/bingux/search.json` | `bingux` |
| Bundled Adwaita symbolic icons | `/usr/share/bingux/icons/Adwaita` (private; never `/usr/share/icons/Adwaita`, which `adwaita-icon-theme` owns) | `bingux` |
| Desktop entries | `/usr/share/applications/bingux-{settings,files,taxonomy}.desktop` | each app |
| Icons | `/usr/share/icons/hicolor/scalable/apps/bingux-{settings,files,taxonomy}.svg` | each app |
| AppStream | `/usr/share/metainfo/dev.drewett.bingux.{Settings,Files,Taxonomy}.metainfo.xml` | each app |
| Man pages | `/usr/share/man/man1/bingux-{settings,files,config,effects,taxonomy}.1` | each package |
| Search-app manifest | `/usr/share/bingux/search-apps/bingux-settings.json` | `bingux-settings` |
| User units | `/usr/lib/systemd/user/bingux*.service`, `bingux.target` | `bingux`; `bingux-effects@.service` in `bingux-audio-effects` |
| User preset | `/usr/lib/systemd/user-preset/90-bingux.preset` | `bingux` |
| Gnoblin drop-in | `/usr/share/gnoblin/conf.d/bingux.lua`, `/usr/share/gnoblin/scripts/bingux-text-input.js` | `bingux` |
| LADSPA plugins | `/usr/lib64/ladspa/libbingux_dsp.so` (Fedora), `/usr/lib/ladspa/` (Arch) | `bingux-audio-effects` |
| PAM service | `/etc/pam.d/bingux-lock` (`%config(noreplace)`) | `bingux` |
| Licences | `/usr/share/licenses/<package>/`, plus `COPYING` and a `*.LICENSE.dependencies` file per program | each package |

There is no polkit rule, D-Bus service file, gsettings schema or GSettings
override in this branch's packages. (The image branch adds polkit and portal
files for the greeter and the portal backend; they belong to the shell package
there.)

### 2.3 Integration points

- **MIME and URL schemes.** Settings registers `x-scheme-handler/bingux-settings`,
  Files registers `inode/directory` and `x-scheme-handler/bingux-files`,
  Taxonomy registers `x-scheme-handler/bingux-taxonomy`. The desktop database
  (`mimeinfo.cache`) is rebuilt by file triggers in Fedora
  (`desktop-file-utils`) and libalpm hooks in Arch, so no package has a
  scriptlet for it. Icon caches work the same way (`gtk-update-icon-cache`).
- **Default applications.** Packages do not claim a default. Files is a
  candidate for `inode/directory`; the user picks it with
  `xdg-mime default bingux-files.desktop inode/directory` or in
  Settings > Applications > Default applications. Question 5 covers shipping a
  default for the Gnoblin desktop.
- **Search.** `bingux-searchd` reads `$XDG_DATA_DIRS/bingux/search-apps/*.json`
  (`design/search.md` section 8). The manifest is `{kind, id, displayName,
  command}`; the installer fills `@BINDIR@`. Settings keeps a snapshot index in
  `$XDG_STATE_HOME/bingux/search/bingux-settings/` while it runs, and serves
  `$XDG_RUNTIME_DIR/bingux/search/bingux-settings.sock` when it is open or
  started with `--search-provider`. Files has no search-app manifest: it does not
  expose documents to Spotlight.
- **Single instance.** Each app opens `$XDG_RUNTIME_DIR/<app-id>.sock`; a second
  launch passes its address to the running window and exits. No D-Bus activation
  file is needed or shipped.
- **Systemd user units.** `bingux.target` is wanted by `gnoblin-session.target`
  and pulls in `bingux-searchd.service` and `bingux-statusd.service`.
  `90-bingux.preset` enables the target (`systemd-update-helper` on Fedora and
  `bingux.install` on Arch apply it at installation). The shell surfaces
  (`bingux`, `bingux-popouts`, wallpaper) start from Gnoblin's Lua autostart,
  not from systemd. `bingux-effects@output.service` and
  `bingux-effects@input.service` are enabled by Settings when the user turns
  effects on; the preset keeps them disabled.
- **Shell to apps.** The shell opens Settings by `bingux-settings PANEL`. The
  search manifest and `BINGUX_SETTINGS_BIN` (set by the installed launchers)
  locate the binary.
- **Gnoblin.** `bingux-settings` calls `gnoblinctl` and reads
  `org.gnome.Mutter.DisplayConfig`, which Gnoblin serves. On another compositor
  those pages report that the backend is missing.

### 2.4 Build and licence notes

- The three GUI apps are built with Freya 0.5 (`freya-skia-bindings`, a fork of
  rust-skia) on OpenGL. Skia is compiled into the binary. By default the
  bindings **download a prebuilt Skia archive during the build**; there is no
  system Skia on Fedora or Arch. Fedora's own build farm forbids that, so the
  apps cannot enter the main Fedora repository without building Skia from source
  (`FORCE_SKIA_BUILD`) or a distro Skia package (question 8). A COPR project
  with network enabled, and `makepkg`, can build as is.
- Rust crates come from crates.io (`cargo build --locked`). A build needs
  the network or a populated `CARGO_HOME` (`CARGO_NET_OFFLINE=true`).
- The programs link `libEGL`, `libGL`, `libfontconfig`, `libfreetype`,
  `libharfbuzz`, `libX11` and `libxcb` directly. They `dlopen` `libxkbcommon`
  and `libwayland-client`/`-egl`; rpm and makepkg cannot see those, so the
  packages require them by name.
- Licences: the project is GPL-3.0-or-later. The statically linked crates add
  Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, MIT, Unicode-3.0 and Zlib
  (and CDLA-Permissive-2.0 in `bingux-searchd`).
  `scripts/cargo-licenses.py` lists them per program (it chooses a licence a
  crate already shares with another when a crate offers a choice) and writes
  `<program>.LICENSE.dependencies`, which each package installs as `%license`.
  `tests/rpm-package.py` fails when a spec `License:` tag is out of date. The
  bundled Adwaita icons are LGPL-3.0-or-later and CC-BY-SA-3.0; the shell package
  carries their texts. Skia's own licence (BSD-3-Clause) is already in the set;
  the third-party libraries inside Skia's prebuilt archive are not itemised
  `[INFERENCE: the archive's licence file is not in the crate]`.

### 2.5 Configuration and data

Everything follows XDG. Packages own only installed files; removal never
deletes user data.

| Path | What | Writer |
| --- | --- | --- |
| `$XDG_CONFIG_HOME/bingux/bingux.toml` | Shell and desktop configuration; edited by hand, `bingux-config` or Settings; the shell watches it | Settings, `bingux-config`, user |
| `$XDG_CONFIG_HOME/bingux/search.json` | Search commands and providers (the packaged `/usr/share/bingux/search.json` is the default) | user |
| `$XDG_CONFIG_HOME/bingux/effects/` | Effects `state.json` and presets | Settings, `bingux-effects` |
| `$XDG_CONFIG_HOME/systemd/user/bingux-effects@.service` | A copy of the unit that Settings writes before it enables effects, naming the program it runs from | Settings |
| `$XDG_DATA_HOME/bingux/data-usage.sqlite3` | Network data usage (SQLite 3) | `bingux-statusd` writes, Settings reads |
| `$XDG_STATE_HOME/bingux/search/<app-id>/` | Search snapshots | each app |
| `$XDG_STATE_HOME/bingux/files/view.json` | Files view and window state | Files |
| `$XDG_CACHE_HOME/bingux/files/v1/`, `…/bingux/effects/plugins.json` | Caches | Files, `bingux-effects` |
| `$XDG_DATA_HOME/Trash`, `recently-used.xbel` | Trash and recent files (freedesktop.org) | Files |
| `$XDG_RUNTIME_DIR/bingux/` | `search-v1.sock`, `metrics-v1.sock`, `search/<app-id>.sock`, effects meters | daemons, apps |
| `$XDG_RUNTIME_DIR/<app-id>.sock` | Single-instance hand-off | apps |

`$XDG_DATA_HOME/bingux` is also the root of a managed per-user install
(`make install-user`). A package install and a user install can coexist; the user
install's launchers set `XDG_DATA_DIRS` first and win.

`make install-apps` is the shell-free variant: Settings, Files, Taxonomy and what
Settings runs go below `~/.local` (`bin`, `share`, `lib/ladspa`), recorded in
`~/.local/share/bingux-apps/manifest.json`. It does not touch the session or the
files of a managed install in `$XDG_DATA_HOME/bingux` (it adds only
`search-apps/bingux-settings.json` there; a later `make install-user` replaces that
root, so run `make install-apps` again after it). `bingux-apps-uninstall` reverses
the install exactly.

### 2.6 Updates

- Fedora: `dnf upgrade` (from COPR). `%systemd_user_postun_with_restart` marks the
  helper services and the effects instances for restart in running user
  sessions. The shell and the apps are separate processes: an updated app starts
  on its next launch, and the shell restarts on the next login (Gnoblin starts it
  from Lua; a package upgrade does not restart the running desktop).
- Bingux OS: `pacman -Syu` against `[bingux-os]`. The image's alpm hook
  re-applies the os-release files after transactions.
- Versions are in lockstep, so a partial upgrade (new `bingux-settings` with an
  old `bingux-config`) is blocked by the exact-version dependencies.
- Configuration is migrated by the programs (`bingux-config` renames and
  imports `settings.json` on first run), not by packages.

## 3. What is in the worktree

`packages/` holds every crate and native helper. This table names each one,
what it installs and whether it runs without being opened.

| Crate or file | Program | Package | Desktop id | Installs | Runtime dependencies | Runs by itself |
| --- | --- | --- | --- | --- | --- | --- |
| `bingux-settings` | `bingux-settings` | `bingux-settings` | `bingux-settings.desktop` (`x-scheme-handler/bingux-settings`) | `/usr/bin`, desktop entry, hicolor SVG, metainfo, man page, `search-apps/bingux-settings.json` | `bingux-config`; `nmcli` (NetworkManager), `bluetoothctl` (bluez), `wpctl` (wireplumber), `pactl` (pulseaudio-utils), `pw-dump`/`pw-record`/`pw-cli` (pipewire-utils), `upower`, `powerprofilesctl` (power-profiles-daemon), `timedatectl`/`hostnamectl` (systemd), AccountsService, `udisksctl` (udisks2), `mmcli` (ModemManager), `wg` (wireguard-tools), `gsettings`/`gio` (glib2, dconf, gsettings-desktop-schemas), `magick` (ImageMagick), `fc-match`/`fc-list` (fontconfig), `xdg-open` (xdg-utils), `gnoblinctl` (Gnoblin), Adwaita Sans (adwaita-sans-fonts / adwaita-fonts); every backend is optional and its page shows an error without it | No. One window; `--search-provider` runs it without a window and nothing starts that mode |
| `bingux-files-app` | `bingux-files` | `bingux-files` | `bingux-files.desktop` (`inode/directory`, `x-scheme-handler/bingux-files`) | `/usr/bin`, desktop entry, hicolor SVG, metainfo, man page | `tar`, `unzip`, `udisksctl` (udisks2), `umount` (util-linux), `eject`, `xdg-open`, shared-mime-info, gvfs (Trash on other mounts), Adwaita Sans; recommends `bingux-preview-host` | No |
| `bingux-preview-host` | `bingux-preview-host` | `bingux-preview-host` | none | `/usr/bin` (next to `bingux-files`, where Files looks first; then `PATH`; `BINGUX_PREVIEW_HOST` overrides both) | GStreamer base and good plugins, poppler-glib; recommends the libav, bad-free and openh264 plugins and PipeWire's GStreamer plugin | No; Files starts it per preview (at most two at once) and it dies with Files |
| `bingux-taxonomy` | `bingux-taxonomy` | `bingux-taxonomy` | `bingux-taxonomy.desktop` (`x-scheme-handler/bingux-taxonomy`) | `/usr/bin`, desktop entry, hicolor SVG, metainfo, man page | The GUI libraries only | No |
| `bingux-config` | `bingux-config` | `bingux-config` | none | `/usr/bin`, man page | none | No; the shell calls it |
| `bingux-audio-effects` | `bingux-effects` | `bingux-audio-effects` | none | `/usr/bin`, `bingux-effects@.service`, man page | PipeWire (the `pipewire` program runs the filter chain), `pw-cli` for live control, `systemctl --user` | Yes, after the user turns effects on: one unit per direction |
| `bingux-dsp` | `libbingux_dsp.so` (cdylib) | `bingux-audio-effects` | none | `$libdir/ladspa/` | none | No; PipeWire loads it |
| `bingux-searchd` | `bingux-searchd` (`bingux-search-service` wrapper) | `bingux` | none | `/usr/bin`, `/usr/libexec/bingux/search-service`, `bingux-searchd.service` | `xdg-open`, `wl-copy` (wl-clipboard), `bingux-config`, plocate (optional) | Yes: user service |
| `bingux-statusd` | `bingux-statusd` | `bingux` | none | `/usr/bin`, `bingux-statusd.service` | `pactl` (PulseAudio, or PipeWire's PulseAudio interface), Gnoblin's compositor feed | Yes: user service; also writes the data-usage database |
| `binguxctl` | `binguxctl` (Python) | `bingux` | none | `/usr/bin`, `/usr/libexec/bingux/binguxctl.py` | python3 | No |
| `bingux-audio-meter` | `bingux-audio-meter` (C) | `bingux` | none | `/usr/bin` | libpulse | Started by the shell |
| `bingux-frame` | `bingux-frame` (C, GTK) | `bingux` | none | `/usr/libexec/bingux/bingux-frame` | GTK 4, libadwaita; Gnoblin's window-frame protocol | Started by Gnoblin (`frame-renderers`) |
| `bingux-image-clipboard` | helper (C) | `bingux` | none | `/usr/libexec/bingux/` | libwayland, `ext-data-control` | Started by the shell |
| `bingux-effects` (Qt plugin) | `Bingux.Effects` QML module (background blur) | `bingux` | none | `$libdir/bingux/qml/Bingux/Effects` | Qt 6 | No. Not the sound effects engine: it shares the name |
| `bingux-wayland-sync`, `bingux-text-layout` | QML modules | `bingux` | none | `$libdir/bingux/qml/Bingux/{Wayland,Text}` | Qt 6 | No |
| `bingux-inventory` | `bingux-inventory.py` | not packaged | none | not installed by `install-shell.py` | python3 | `[INFERENCE]` a development helper; question 11 |
| `bingux-quickshell`, `bingux-qmltermwidget` | patched Quickshell and terminal widget sources | not packaged here | none | the packages require `/usr/bin/qs` instead | Qt 6 | No |
| `bingux-ui`, `bingux-files`, `bingux-preview-core`, `bingux-search`, `bingux-data-usage` | libraries | linked in, not installed | none | none | none | No |
| `vendor/` | patched `freya-*` crates (scroll feel) | linked in, not installed | none | none | none | No |
| `shell/bingux`, `shell/gnoblin` | QML shell, Python helpers, Gnoblin scripts | `bingux` | none | `/usr/share/bingux/shell`, `/usr/share/bingux/gnoblin` | Quickshell, Python 3, PyGObject, Pillow, ffmpeg | The shell runs from Gnoblin's autostart |

## 4. Open questions

Each question has a default. The packaging in this change applies the default,
so none of them blocks anything.

### 1. Which base is primary: Bingux OS (Arch) or Fedora?

- **Both from one stage (default).** The spec and the PKGBUILD both exist and
  are tested the same way. Pro: Fedora COPR and the Bingux OS image can ship
  the same release; the Fedora guest keeps working for tests. Con: two formats
  to keep in step, with only a test and the shared stage to help.
- **Arch only.** Pro: one format, and the image is the product. Con: drops the
  Fedora test guest's install path and the COPR plan in `TODO.md`.
- **Fedora only.** Pro: COPR signs and hosts for free. Con: Bingux OS already
  exists and installs from pacman.

### 2. Split packages or one `bingux` package?

- **Six packages (default).** Pro: Settings and Files can be installed, removed
  and replaced alone; Taxonomy stays off ordinary desktops; the effects engine
  does not drag Qt in. Con: more metadata to maintain; exact-version
  dependencies tie them together anyway.
- **One package.** Pro: simplest. Con: Taxonomy (a developer tool) and the
  effects plugin library install everywhere; Settings cannot be installed
  without the shell.
- **Plus a metapackage** (`bingux-desktop` requiring all but Taxonomy). Pro: one
  name for "the whole desktop". Con: another package to version. Not added:
  Fedora's `Recommends` and the image's package list already do this job.

### 3. AppStream component ids and the domain

The ids are `dev.drewett.bingux.Settings`, `…Files` and `…Taxonomy`
(reverse-DNS of `drewett.dev`, a domain Kieran owns), linked to the desktop
entries with `<launchable type="desktop-id">`. The desktop ids stay
`bingux-settings.desktop` and so on, because the window's `app_id` is
`bingux-settings` and the dock groups windows by `StartupWMClass`.
`appstreamcli validate --pedantic` reports `cid-contains-uppercase-letter` (a
pedantic note only).

- **Keep (default).** Pro: validates; no rename of desktop ids. Con: the ids differ
  from the desktop ids.
- **Lower-case ids** (`dev.drewett.bingux.settings`). Pro: no pedantic note.
  Con: not the usual look in Flathub and GNOME software.
- **`org.bingux.*`.** Pro: shorter. Con: the project does not own `bingux.org`
  `[INFERENCE: not checked]`.

### 4. Is Taxonomy shipped at all?

- **Packaged, not installed by default (default).** `bingux-taxonomy` exists in the
  repository; no image or `Recommends` pulls it in. Pro: developers can
  `pacman -S`/`dnf install` it. Con: it appears in app lists of whoever installs it.
- **Source only** (`make install-user`). Pro: users never see it. Con: it is
  the visual test of the Bingux UI and is harder to get.
- **Rename to `bingux-devtools`.** Pro: matches what it is. Con: the program,
  desktop id and window id would change.

Note: `desktop-file-validate` accepts `bingux-taxonomy.desktop` but hints that
`Categories=Development;Utility;` has two main categories. The file belongs to
the Taxonomy crate and is left as is.

### 5. Which file manager and which settings app does the Bingux desktop default to?

Bingux OS's package list still installs `nautilus` and `gnome-control-center`
(`distro/profile/packages.x86_64`), and the image branch's `bingux` PKGBUILD
depends on `gnome-control-center`. Nautilus and Files both claim
`inode/directory`, and the order the desktop database picks is not specified.

- **Ship nothing; the user chooses (default).** Pro: no surprise for Fedora
  users. Con: on the image, which one opens a folder is accidental.
- **A desktop-specific list** `/usr/share/applications/gnoblin-mimeapps.list`
  with `inode/directory=bingux-files.desktop` and the Settings scheme. Gnoblin
  sets `XDG_CURRENT_DESKTOP=Gnoblin`, which GLib and `xdg-mime` read. Pro: only
  the Gnoblin session changes, and a user's own `mimeapps.list` still wins. Con:
  a file to keep correct per desktop name `[INFERENCE: not tested here]`.
- **Remove Nautilus and `gnome-control-center` from the image.** Pro: no
  ambiguity. Con: loses fallbacks while Settings and Files are new.

### 6. Where do the effects units live?

Settings writes `~/.config/systemd/user/bingux-effects@.service` before it
enables effects. The package now ships the same template in `/usr/lib/systemd/user`.
The user copy overrides it and names the program it was written from.

- **Both for now (default).** Pro: no code change; the packaged unit is there for
  `systemctl --user enable` and for `bingux-effects restore`. Con: two copies;
  the user copy can go stale after the program moves.
- **Settings stops writing the unit when the packaged one exists.** Pro: one
  copy. Con: a change to `bingux-audio-effects` (`service.rs`, not in this
  change's scope).
- **Run effects without a unit** (`bingux-effects restore` from the Gnoblin
  autostart). Pro: no unit lifecycle. Con: no restart on failure.

### 7. One effects package or two?

`bingux-audio-effects` holds the engine and `libbingux_dsp.so`.

- **One package (default).** Pro: the engine needs the plugins; one install.
- **Split the LADSPA library** (`bingux-dsp-ladspa`). Pro: EasyEffects or Ardour
  users can take only the plugins. Con: one more package for a 500 KB file.

### 8. How do Fedora and COPR builds get Rust crates and Skia?

The build downloads crates and Skia binaries (section 2.4).

- **Network in COPR (default).** Pro: works today. Con: not allowed in Fedora's
  own build farm; builds are not reproducible from the SRPM alone.
- **Vendored crates in the SRPM** (`cargo vendor` per crate) plus
  `SKIA_BINARIES_URL` pointing at a mirror. Pro: offline, auditable. Con: a
  large SRPM (several hundred MB) and five lock files to vendor.
- **Build Skia from source** (`FORCE_SKIA_BUILD`). Pro: the only route to Fedora
  proper. Con: hours of build time, needs `clang`, `ninja`, `python` and `gn`.

### 9. Signing and where releases are published

Pacman packages and the database are unsigned today. RPMs from COPR are signed
by COPR.

- **Unsigned until the repository is public (default).** Con: anyone who can
  change the GitHub release can change what machines update to.
- **Sign with a project key** (`makepkg --sign`, `repo-add --sign`, key in the
  image's `pacman-key` setup). Pro: closes the gap. Con: a key to guard and rotate.

### 10. CI

The Bingux repository has no GitHub workflows. Gnoblin has fifteen
(`release.yml`, `copr.yml`, `arch-package.yml`, …).

- **Document the steps, add no workflow yet (default).** Pro: nothing untested
  lands. Con: nothing runs on a push.
- **Add a workflow** that runs `make rpm-check` and `make arch-package` in
  containers on every pull request and publishes on a tag. Pro: catches a
  broken spec before a release. Con: the Rust and Skia build is long and needs
  the network.

### 11. What about `bingux-inventory` and other unpackaged files?

`packages/bingux-inventory/bingux-inventory.py` is not installed by any target.

- **Leave it out (default).** `[INFERENCE: it is a development helper.]`
- **Install it under `/usr/libexec/bingux`** if the shell calls it. Not checked.

## 5. Image changes (`~/dev/bingux-os`)

Not applied. The image branch cannot build the new packages until the apps
are merged into it: its tree has no `packages/bingux-files-app`,
`packages/bingux-audio-effects`, `packages/bingux-dsp` or
`packages/bingux-taxonomy`, no split PKGBUILD and a different shell build
(`bingux-shell`, no `qs`). Adding the package names to `packages.x86_64` today
would make `pacman` fail with "target not found".

After the merge, replace `distro/packages/bingux/PKGBUILD` with
`packaging/arch/PKGBUILD` from this branch, extended by the image branch's
additions (portal, greeter and bsm files go to the shell package, which takes
the stage's leftovers), and:

```diff
--- a/distro/profile/packages.x86_64
+++ b/distro/profile/packages.x86_64
 # Session: Gnoblin compositor, Bingux shell, greetd login manager
 gnoblin
 bingux
+bingux-config
+bingux-settings
+bingux-files
+bingux-audio-effects
 bingux-os
```

```diff
--- a/distro/container-build.sh
+++ b/distro/container-build.sh
 build_packages() {
     build_package gnoblin
-    build_package bingux
+    build_package bingux   # one PKGBUILD, six packages: makepkg writes them all
```

```diff
--- a/distro/packages/bingux-os/PKGBUILD
+++ b/distro/packages/bingux-os/PKGBUILD
-depends=('bingux' 'gnoblin' 'greetd' 'networkmanager' 'glib2' 'plymouth')
+depends=('bingux' 'bingux-settings' 'bingux-files' 'gnoblin' 'greetd' 'networkmanager' 'glib2' 'plymouth')
```

`bingux-os` is installed by both the live image and `bingux-installer`, so
the dependency puts Settings and Files on the installed system too. Whether
`gnome-control-center` and `nautilus` stay in the image is question 5.
`build_package` copies the Bingux source archive for the names `bingux | bsm`;
the new packages come from the same archive, so no new case is needed.

## 6. How the packages were verified

The commands, their output and the limits (what could not be run) are in
[the packaging notes](../packaging/README.md#verification-record).
