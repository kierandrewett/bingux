# Configuration

Bingux keeps its settings in one TOML file:

```
~/.config/bingux/bingux.toml
```

`$XDG_CONFIG_HOME/bingux/bingux.toml` when `XDG_CONFIG_HOME` is set.

You can edit the file by hand, use the `bingux-config` command, or use the
Settings application. All three go through the same validation, and the
shell applies a change as soon as the file is saved. Comments and key order
that you add by hand survive graphical edits.

## Edit by hand

Keys you leave out keep their defaults, so a file can be as short as:

```toml
[desktop]
clock_seconds = true
dock_size = 64

[search]
disabled_providers = ["web"]
```

Print a complete, commented file with every key at its default:

```sh
bingux-config defaults
```

Check a file after editing it:

```sh
bingux-config check
# ~/.config/bingux/bingux.toml: ok
# or: ~/.config/bingux/bingux.toml: desktop.dock_size: Dock icons must be between 32 and 80 pixels.
```

When the running shell reads a file with a problem, it keeps the last good
settings and shows one notification with the same message.

## Edit with the command

```sh
bingux-config show                              # effective configuration as TOML
bingux-config get desktop.dock_size             # 56
bingux-config set desktop.dock_size 64          # desktop.dock_size = 64
bingux-config set search.disabled_providers '["web", "files"]'
bingux-config unset desktop.sidebar_edge        # back to the default
bingux-config explain desktop.dock_click        # accepted values, default, current value
bingux-config edit                              # open in $VISUAL or $EDITOR, then validate
bingux-config path                              # print the file path
```

`set` parses the value as TOML (`true`, `64`, `"left"`, `["a", "b"]`);
anything that is not valid TOML is stored as a string, so quotes around a
plain path are optional. `edit` restores the previous file when the edited
one does not validate and keeps your edit next to it as
`bingux.toml.rejected`.

Commands that print JSON exist for the shell and the Settings application:
`dump`, `save`, `import-layout` and `wallpaper`. `wallpaper` prints a
`file://` URI the shell can load: `desktop.wallpaper` when it names a readable
file, otherwise `org.gnome.desktop.background`, where GNOME Settings and the
wallpaper portal write. Formats Qt cannot decode are converted once into a
cached PNG under `~/.cache/bingux/wallpaper`. `bingux-config --help` lists
everything.

## Migration from settings.json

Earlier versions stored settings in `~/.config/bingux/settings.json`. The
first time any Bingux component reads the configuration, that file is
converted to `bingux.toml` and renamed to `settings.json.migrated`. Nothing
else changes; the keys below are the same settings with snake_case names.

## Keys

### `[desktop]`

| Key                     | Type   | Default     | Accepted values                                                |
| ----------------------- | ------ | ----------- | -------------------------------------------------------------- |
| `dock`                  | bool   | `true`      | Show the dock.                                                 |
| `sidebar`               | bool   | `true`      | Show the edge sidebar.                                         |
| `metrics`               | bool   | `true`      | Show system monitors in the top bar.                           |
| `clock_seconds`         | bool   | `false`     | Show seconds in the clock.                                     |
| `wallpaper`             | string | `""`        | Absolute image path, or empty to use the GNOME background.     |
| `sidebar_edge`          | string | not set     | `"left"`, `"right"` or `"top"`. Not set lets the shell choose. |
| `dock_size`             | int    | `56`        | Icon size in pixels, 32 to 80.                                 |
| `dock_alignment`        | string | `"center"`  | `"left"`, `"center"` or `"right"`.                             |
| `dock_click`            | string | `"toggle"`  | `"toggle"`, `"focus"` or `"launch"`: click on a running app.   |
| `dock_middle_click`     | string | `"launch"`  | `"launch"`, `"close"` or `"none"`.                             |
| `dock_scroll`           | string | `"cycle"`   | `"cycle"` or `"none"`: scroll over the dock.                   |
| `dock_scroll_direction` | string | `"natural"` | `"natural"` or `"reverse"`.                                    |
| `layout_version`        | int    | `0`         | `0` or `1`. Managed by the shell.                              |

Tables under `[desktop]` are written by the shell when you pin apps, create
dock folders or arrange widgets. They are validated like everything else,
but you normally do not edit them: `dock_apps` (`pinned_apps`, `order`),
`dock_folders` (array of `id`, `name`, `color`, `apps`), `layout`,
`control_centre` (`vpn`, `dnd`, `night_light`, `power`, `awake`),
`control_order`, `control_layout`, `containers` and `widget_options`.

### `[search]`

| Key                  | Type   | Default        | Accepted values                                                                                   |
| -------------------- | ------ | -------------- | ------------------------------------------------------------------------------------------------- |
| `disabled_providers` | array  | `[]`           | Any of `applications`, `files`, `calculation`, `conversions`, `web`, `web-shortcuts`, `external`. |
| `default_engine`     | string | `"duckduckgo"` | Identifier of an enabled engine.                                                                  |
| `file_roots`         | array  | not set        | Up to 32 absolute directories for file search. Not set uses the built-in home folders.            |

Website engines are an array of tables:

```toml
[[search.engines]]
id = "duckduckgo"
name = "DuckDuckGo"
shortcut = "ddg"
url = "https://duckduckgo.com/?q={query}"
enabled = true
```

`id` and `shortcut` use lowercase letters, digits and hyphens and must be
unique; `wiki`, `gh`, `maps` and `yt` are reserved shortcuts. `url` is an
HTTP or HTTPS URL with exactly one `{query}` outside the host name. Keep
between 1 and 32 engines, and the default engine enabled.

The optional AI assistant is a table:

```toml
[search.ai]
harness = "claude"        # "pi" or "claude"
model = "sonnet"          # optional, passed to the harness
# executable = "/usr/local/bin/claude"   # optional; found on PATH when omitted
```

`bingux-config set` and `save` refuse a change to `search.ai` when the chosen
program is not installed. Loading a file whose harness has since been removed
still works, and other writes are unaffected; the search service reports the
missing program when you ask the assistant a question.

### `[previews]`

| Key             | Type | Default | Accepted values                      |
| --------------- | ---- | ------- | ------------------------------------ |
| `enabled`       | bool | `true`  | Preview files in search results.     |
| `prewarm`       | bool | `true`  | Prepare previews for nearby results. |
| `max_megabytes` | int  | `20`    | Largest previewed file, 1 to 20.     |

### `[privacy]`

| Key                | Type | Default | Accepted values                                                             |
| ------------------ | ---- | ------- | --------------------------------------------------------------------------- |
| `public_ip_lookup` | bool | `false` | Let the Control Centre ask ipapi.co for the public address. Off by default. |

## Where the shell reads it

- The shell watches `bingux.toml` and reloads through `bingux-config dump`.
- `bingux-searchd` reads the `search` section on start; a change to that
  section restarts the service.
- File previews read `previews` on each request.
- `bingux-config` is installed next to the other Bingux commands. Set
  `BINGUX_CONFIG_BIN` to point a component at a different build.

## Tests

```sh
make check   # builds bingux-config and runs its tests
BINGUX_CONFIG_BIN=build/cargo/release/bingux-config python3 tests/bingux-config.test.py
```
