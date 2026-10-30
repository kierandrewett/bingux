# Apps

Programs a person launches from the dock or the application grid. Each one is a
Freya application built on the design system in `crates/ui`. The directory name
is short; the Cargo package and the installed program keep their full names.

| Directory | Cargo package | Program | What it is |
| --------- | ------------- | ------- | ---------- |
| `settings/` | `bingux-settings` | `bingux-settings` | The Settings application. See `docs/settings.md`. |
| `files/` | `bingux-files-app` | `bingux-files` | The Files application. See `docs/files.md`. |
| `taxonomy/` | `bingux-taxonomy` | `bingux-taxonomy` | Every component, icon and token of the design system, live. |

Background programs belong in `services/`, and code that apps share belongs in
`crates/`.
