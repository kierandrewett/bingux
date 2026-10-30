# Gnoblin integration

What Bingux installs into the Gnoblin compositor.

| Path | Installed as | Purpose |
| ---- | ------------ | ------- |
| `bingux.lua` | `share/gnoblin/conf.d/bingux.lua` | The Gnoblin module for the desktop session: animations, window rules, autostart and shortcuts. |
| `greeter.lua` | `share/bingux/greeter/gnoblin.lua` | The Gnoblin configuration for the login greeter session. |
| `scripts/` | `share/bingux/gnoblin/` | Scripts Gnoblin runs: text input and clipboard paste. |

`scripts/install-shell.py` rewrites command paths in the Lua files for the
chosen prefix.
