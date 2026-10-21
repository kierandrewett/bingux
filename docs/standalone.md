# Standalone shell

Bingux is a desktop shell. Gnoblin supplies the compositor and session, while
Bingux supplies the user interface. The shell also runs on another Wayland
compositor when that compositor provides the protocols used by the selected
features.

## Build

Install a matching Qt 6 development environment, CMake, Ninja, a C and C++
compiler, Cargo and the system libraries used by the shell and the optional
helpers. Then run:

On Fedora, use the spec as the dependency source instead of maintaining a
second package list:

```sh
sudo dnf install rpmdevtools
sudo dnf builddep packaging/rpm/bingux.spec
```

Fetch the pinned submodules once; the source archive used by packages already
contains them:

```sh
git submodule update --init
```

```sh
make
make check
```

Before a source install, `make doctor` checks the compilers, CMake, Cargo,
native libraries, Qt Quick/QML, the submodules and user-systemd
prerequisites. The managed install target runs this check automatically and
explains missing dependencies before starting the build.

Use `make install-user` for a checked source installation. Calling
`scripts/install-shell.py` directly bypasses prerequisite validation and is
not a safe way to resolve a failed doctor check.

The build produces:

- `bingux-shell`: Quickshell 0.2.1, with the runtime patches from
  `packages/bingux-quickshell` applied, compiled into one binary whose entry
  point knows the Bingux shells (`packages/bingux-shell`). No separate
  Quickshell install is used; the Qt build is always the one the shell was
  compiled against.
- `bingux-shell.sxa`: the shell archive. `scripts/pack-shell.py` packs the QML,
  JavaScript and assets of `shell/bingux` into one tar.xz with a manifest of
  entry files per mode.
- The three QML plugins, the audio meter, the Rust daemons and the Settings
  application.

### Shell modes

`bingux-shell MODE` starts one shell surface; the installed command names are
symlinks to the binary and select the same modes:

| Command              | Mode        | Entry file           |
| -------------------- | ----------- | -------------------- |
| `bingux`             | `desktop`   | `shell.qml`          |
| `bingux-lock`        | `lock`      | `LockShell.qml`      |
| `bingux-greeter`     | `greeter`   | `GreeterShell.qml`   |
| `bingux-popouts`     | `popouts`   | `PopoutsShell.qml`   |
| `bingux-wallpaper`   | `wallpaper` | `WallpaperShell.qml` |
| `bingux-search-ui`   | `search`    | `SearchShell.qml`    |
| `bingux-switcher-ui` | `switcher`  | `SwitcherShell.qml`  |
| `bingux-emoji-ui`    | `emoji`     | `EmojiShell.qml`     |
| `bingux-capture-ui`  | `capture`   | `CaptureShell.qml`   |

On first start the binary unpacks the archive into
`$XDG_CACHE_HOME/bingux/shell/<content id>/` (or `/tmp/bingux-shell-<uid>`
when the cache is not writable, as for the greeter account) and runs the entry
file from there, so Qt's compiled-QML cache works on ordinary files. A new
archive has a new content id; old directories can be deleted at any time.

| Option or variable     | Effect                                                                |
| ---------------------- | --------------------------------------------------------------------- |
| `--source DIR`         | Run the mode from a checkout (`shell/bingux`) instead of the archive. |
| `--archive FILE`       | Use this archive instead of the installed one.                        |
| `--shell-dir`          | Print the unpacked shell directory (used by `binguxctl` for IPC).     |
| `BINGUX_SHELL_SOURCE`  | Default for `--source`.                                               |
| `BINGUX_SHELL_ARCHIVE` | Default for `--archive`.                                              |
| `bingux-shell -p FILE` | Plain Quickshell with the Bingux environment, for tests and tools.    |

The binary sets the environment the shell expects when it is not already set:
`QML_IMPORT_PATH` (the Bingux QML plugins), `PATH` and `XDG_DATA_DIRS` (the
install prefix), `BINGUX_HELPER_DIR` (the Python helpers under
`libexec/bingux/shell`), `BINGUX_CONFIG_BIN`, `BINGUX_SETTINGS_BIN`,
`BINGUX_AUDIO_METER` and `BINGUX_ADWAITA_ICON_DIR`. Values already in the
environment win, so a developer can point an installed binary at a checkout.

## Install for your user

For a normal source install, use the managed user target:

```sh
make install-user
```

The installer builds native pieces when needed and puts the Bingux payload in
`$XDG_DATA_HOME/bingux`, normally `~/.local/share/bingux`.

Gnoblin starts the shell and its layer-shell clients from Lua. The helper-service
target starts the search and status helpers.

The installer records the following managed links in `.bingux-install.json`:

- `$XDG_BIN_HOME`, normally `~/.local/bin`, for convenience commands.
- `$XDG_CONFIG_HOME/systemd/user` for optional helper units.
- `$XDG_CONFIG_HOME/gnoblin/conf.d` for the Bingux Gnoblin module.
- `$XDG_DATA_HOME/gnoblin/scripts` for the text-input integration.

The matching uninstall command removes those links safely. Existing personal
settings stay in place. The installer puts the `gnoblin.load(...)` loader at
the end of `init.lua`, after generic frame and window rules, without replacing
the rest of the file.
Installed autostart commands use absolute paths; convenience commands still
need the PATH entry below.

The module enables edge snapping and applies Bingux's frame, corner and
shortcut defaults through `gnoblin.configure`. It keeps earlier autostarts
and window rules: autostarts merge by name, and Bingux's ordered window rules
append after existing rules. Put personal overrides after the Bingux loader.

Load the module once per configuration. Including it twice appends its window
rules twice; named autostarts remain single entries. A configuration reload
rebuilds the declarations, so repeated reloads do not accumulate rules.

The installed Gnoblin module also starts `bingux-frame` by its absolute path
inside the Bingux prefix: `$XDG_DATA_HOME/bingux/libexec/bingux/bingux-frame`
for a managed user install, or `/usr/libexec/bingux/bingux-frame` for a system
package. This works when GDM does not include `$XDG_BIN_HOME` in the session
`PATH`.

Add `~/.local/bin` to `PATH` to use Bingux convenience commands.

The installer disables and backs up the historical Bingux `quickshell.service`.
That stops an old session target from starting a second desktop shell. Search and
status helpers remain attached to `bingux.target`, which follows
`gnoblin-session.target`.

Bingux bundles its symbolic Adwaita icons. Launch it through the installed
`bingux` command so its asset paths and QML plugins are available. Application
icons use the system theme; an unavailable icon uses a bundled generic icon.

The `bingux-uninstall` command removes the managed installation and the
unpacked shell cache without the source checkout.

### Reclaim unused memory

`bingux-shell` is built with jemalloc and reclaims unused allocator pages
asynchronously after panels close. It keeps the existing decay intervals and
respects an explicit `MALLOC_CONF` environment value. See jemalloc's
[configuration reference](https://jemalloc.net/jemalloc.3.html#tuning) for its
option format.

### Uninstall Bingux

Remove the managed install with:

```sh
make uninstall-user
```

Once `~/.local/bin` is on `PATH`, the equivalent standalone command is:

```sh
bingux-uninstall
```

The Gnoblin package drop-in disappears with the managed install. The uninstall
then stops Bingux, removes its integration symlinks, and deletes only the
managed installation root. It deliberately keeps `$XDG_CONFIG_HOME/bingux`,
so layouts, search settings, extensions and notification history survive a
reinstall. Use `USER_PREFIX=/some/other/root` with both commands when a
different central location is required.

## Package or stage an install

Use a staging directory when creating a package or testing an install:

```sh
make install PREFIX=/usr DESTDIR="$PWD/dist/root"
```

The package-oriented installer writes only below `DESTDIR`. It installs:

- `bingux-shell` and the mode symlinks in `bin`.
- The shell archive `bingux-shell.sxa` and the native QML plugins in
  `lib/bingux` (`lib64` on Fedora).
- The Python shell helpers in `libexec/bingux/shell`; other helper programs
  in `bin` and `libexec/bingux`.
- The Gnoblin module in `share/gnoblin/conf.d/bingux.lua`.
- User systemd units in `lib/systemd/user`.

It does not enable services or change user configuration. Use the managed
`make install-user` flow for a source installation.

The optional systemd target starts only the search and status helper services.
Gnoblin's Lua autostart starts the Bingux desktop, search, switcher, capture,
and emoji layer-shell clients. Removing the Bingux module from the Gnoblin
configuration stops those session-owned clients on the next reload.

`bingux-shell` clears its core-dump limit at start. Services restart after a
failure and use the same QML import path as the shell.

## Application scheduling

The default desktop-entry launcher drops an inherited negative nice value to
0 before directly starting an app, including the dock's fallback when the user
manager rejects a launch. It preserves positive nice values and scheduler
policy. External scheduling rules can change an app's priority afterwards.

Calendar launches from the top bar and Settings launches from the control
centre also drop inherited negative nice. Calendar's date and event commands
use the same rule. If a priority reset fails, the helper writes a diagnostic
to stderr and continues launching. Custom search launcher and file-opener
commands retain their own launch behavior.

Settings starts with `XDG_CURRENT_DESKTOP=GNOME` in its own environment.
The Bingux session retains its existing desktop identity.

Dock menu application actions use the desktop entry's action ID through GIO.
This preserves field-code expansion, the entry's working directory and D-Bus
activation. An unavailable action reports an error instead of starting the
entry's default command.

## Application focus

Dock clicks, keyboard launches and dock-menu actions request activation through
the compositor. Its focus policy still decides whether the app receives focus
or requests attention. Scripted and IPC launches do not borrow recent clicks.
After installing a rebuilt native plugin, start a new Bingux shell process to
load it; reloading QML alone does not load the new native type.

For D-Bus application launches, GIO includes activation data only when the
desktop entry declares `StartupNotify=true`. Entries without that flag retain
GIO's existing D-Bus activation behavior.

## Runtime requirements

Camera and screen-sharing application details use a persistent PipeWire graph
monitor. Monitoring an idle graph does not run periodic full-graph queries or
open a camera.

If the connection fails, Bingux clears those details and retries after two
seconds. It reads a fresh graph before restoring application details.

The full shell uses Qt 6, Quickshell, PipeWire or PulseAudio, NetworkManager,
a systemd user manager for search and status helpers, GNOME settings helpers,
`wl-clipboard`, `xdg-utils`, PyGObject and the Python libraries used by document
previews. GTK 3 is used by the emoji text-entry helper for XWayland clients.
The terminal sidebar additionally needs a Qt 6 QMLTermWidget build that
matches Quickshell.

The terminal plugin is optional. If it is absent, the rest of the shell stays
available and the sidebar shows a retry message.

## Configuration

User state lives below `$XDG_CONFIG_HOME/bingux`. The shell does not require a
distribution-specific profile or generated machine configuration. Search,
status and extension settings are ordinary files and can be copied between
machines.

For extension development, read [Extensions](extensions.md). For isolated
testing, use [portable testing](portable-testing.md).

The packaged Gnoblin defaults enable blur for layer surfaces and set opacity
only on named Bingux panels. Other layer surfaces keep their own opacity, so a
wallpaper is not faded by the shell defaults.

Set the wallpaper with `bingux-config set desktop.wallpaper /path/to/image.png` or in Settings. The value is an absolute image path.
An empty or unreadable path leaves Gnoblin's black background visible. Bingux
creates one background layer-shell surface per screen and converts image formats
that Qt cannot decode into a cached PNG.
