# Desktop layout and preferences

Bingux loads saved desktop placement and presentation from
`~/.config/bingux/bingux.toml` (see [configuration](configuration.md)).
The shell continues to use saved top bar, dock, sidebar and Control Centre layouts.
New installations use the built-in layout defaults.

`bingux-config` validates and writes the file. Search configuration, preview
preferences, capture options and desktop preferences share this file; capture options
remain available from the capture controls. There is currently no desktop layout editor.

The extension registry preserves placements for disabled or missing extension widgets.
Re-enable an extension to make its saved placement available to the shell again. See
[Extensions](extensions.md) for enabling and reloading extensions.

## Checks

- `BINGUX_CONFIG_BIN=build/cargo/release/bingux-config python3 tests/bingux-config.test.py` checks preference validation and persistence.
- `node --test tests/desktop-layout.test.mjs tests/control-layout.test.mjs` checks layout readers.
- `python3 tests/extensions.test.py` checks extension discovery and saved placements.
