# Standalone shell

Bingux is a desktop shell. Gnoblin supplies the compositor and session, while
Bingux supplies the user interface. The shell also runs on another Wayland
compositor when that compositor provides the protocols used by the selected
features.

## Build

Install a matching Qt 6 development environment, Quickshell, a C compiler,
Cargo and the system libraries used by the optional helpers. Then run:

On Fedora, use the spec as the dependency source instead of maintaining a
second package list:

```sh
sudo dnf install rpmdevtools
sudo dnf builddep packaging/rpm/bingux.spec
```

Install Quickshell from the package source that matches the Qt version on the
host. The shell does not bundle or replace Quickshell.

```sh
make
make check
```

Before a source install, `make doctor` checks the compiler, Cargo, native
libraries, Qt Quick/QML, Quickshell and user-systemd prerequisites. The managed
install target runs this check automatically and explains missing dependencies
before starting the build.

The check queries the selected Quickshell without opening the shell. If its build
and runtime Qt versions differ, it reports both versions and the executable.
Rebuild or reinstall that runtime for your Qt version, or set `BINGUX_QUICKSHELL`
to a matching executable before running `make doctor` again.

`make doctor` also checks the selected Quickshell's desktop-action IDs through
an isolated offscreen QML probe. A runtime that truncates IDs, fails to start
the probe or does not finish within ten seconds blocks `make install-user`.
Use a corrected Quickshell build; the
[Gnoblin parser patch](https://github.com/kierandrewett/gnoblin/tree/main/patches/quickshell)
includes preparation instructions. Set `BINGUX_QUICKSHELL` to select that
executable, then rerun `make doctor`.

Use `make install-user` for a checked source installation. Calling
`scripts/install-shell.py` directly bypasses prerequisite validation and is
not a safe way to resolve a failed doctor check. Package staging through
`make install` does not validate the eventual desktop's Quickshell runtime.

The build produces the two QML plugins, the audio meter and the Rust search and
status daemons. Quickshell is a runtime dependency because its Qt build must
match the shell process.

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

The installer prefers Gnoblin's matching Quickshell runtime when it is
installed below `$XDG_DATA_HOME/gnoblin`.

Otherwise it detects `gnoblin-quickshell`, `qs`, or `quickshell`. Set
`BINGUX_QUICKSHELL` to select a matching executable explicitly. The
`bingux-uninstall` command removes the managed installation without the source
checkout.

### Reclaim unused memory

When built with jemalloc, the patched Quickshell runtime reclaims unused
allocator pages asynchronously after panels close. It keeps the existing decay
intervals and respects an explicit `MALLOC_CONF` environment value.

See jemalloc's [configuration reference](https://jemalloc.net/jemalloc.3.html#tuning)
for its option format. Other Quickshell builds retain their own memory behavior;
installing Bingux alone does not patch that executable.

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

- Shell files in `share/bingux/shell`.
- Native QML plugins in `lib/bingux/qml`.
- Helper programs in `bin` and `libexec/bingux`.
- The Gnoblin module in `share/gnoblin/conf.d/bingux.lua`.
- User systemd units in `lib/systemd/user`.

It does not enable services or change user configuration. Use the managed
`make install-user` flow for a source installation.

The optional systemd target starts only the search and status helper services.
Gnoblin's Lua autostart starts the Bingux desktop, search, switcher, capture,
and emoji layer-shell clients. Removing the Bingux module from the Gnoblin
configuration stops those session-owned clients on the next reload.

The shell launcher clears core dumps. Services restart after a failure and use
the same QML import path as the shell.

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

## Runtime requirements

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

Set `desktop.wallpaper` in `$XDG_CONFIG_HOME/bingux/settings.json` through
**Bingux Settings > Desktop > Wallpaper**. The value is an absolute image path.
An empty or unreadable path leaves Gnoblin's black background visible. Bingux
creates one background layer-shell surface per screen and converts image formats
that Qt cannot decode into a cached PNG.
