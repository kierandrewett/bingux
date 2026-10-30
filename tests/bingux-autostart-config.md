# Bingux autostart configuration fixture

Run this check after changing `gnoblin/bingux.lua`:

```sh
GNOBLIN_SOURCE=/path/to/gnoblin python3 tests/bingux-autostart-config.py
```

The fixture builds Gnoblin's native configuration loader in a temporary
folder. It extracts Bingux's packaged `autostart` declaration, applies a named
`bingux` override, and asserts the resulting list is wallpaper, one replaced
desktop entry, then popouts. It does not start a compositor or install files.
