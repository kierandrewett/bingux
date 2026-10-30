# Services

Programs that run in the background or on behalf of the shell. Nobody starts
them by hand in normal use.

| Directory | Cargo package | What it does |
| --------- | ------------- | ------------ |
| `searchd/` | `bingux-searchd` | Local provider host for the desktop search surface. |
| `statusd/` | `bingux-statusd` | Low-overhead metrics feed for the shell. |
| `session/` | `bingux-session` | Account, session and biometric facts for the login screen. |
| `helper/` | `bingux-helper` | Helper commands for the shell, one binary with one subcommand each. |
| `portal/` | `bingux-portal` | The xdg-desktop-portal backend that enforces Bingux portal permissions, and the global shortcuts provider. |
| `preview-host/` | `bingux-preview-host` | The helper process that plays media (GStreamer) and renders documents (poppler) for file previews. |
| `audio-effects/` | `bingux-audio-effects` | The sound effects engine: presets and plugin chains. |

The session manager has its own directory, `bsm/`.
