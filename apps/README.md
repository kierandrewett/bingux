# Apps

Programs a person launches from the dock or the application grid. Each one is a
Freya application built on the design system in `crates/ui`. The directory name
is short; the Cargo package and the installed program keep their full names.

| Directory      | Cargo package           | Program              | What it is                                                    |
| -------------- | ----------------------- | -------------------- | ------------------------------------------------------------- |
| `settings/`    | `bingux-settings`       | `bingux-settings`    | The Settings application. See `docs/settings.md`.             |
| `files/`       | `bingux-files-app`      | `bingux-files`       | The Files application. See `docs/files.md`.                   |
| `calculator/`  | `bingux-calculator-app` | `bingux-calculator`  | The Calculator application. See `docs/calculator.md`.         |
| `weather/`     | `bingux-weather-app`    | `bingux-weather`     | The Weather application. See `docs/weather.md`.               |
| `taxonomy/`    | `bingux-taxonomy`       | `bingux-taxonomy`    | Every component, icon and token of the design system, live.   |
| `text-editor/` | `bingux-text-editor`    | `bingux-text-editor` | The Text Editor application. See `docs/text-editor.md`.       |
| `media/`       | `bingux-media-app`      | `bingux-media`       | The Media application: music and videos. See `docs/media.md`. |
| `preview/`  | `bingux-preview`   | `bingux-preview`  | The Preview application: pictures, PDF documents, text, archives, fonts, songs, films and folders. See `docs/preview-app.md`. |
| `calendar/` | `bingux-calendar`  | `bingux-calendar` | The Calendar application. See `docs/calendar-app.md`.       |

Background programs belong in `services/`, and code that apps share belongs in
`crates/`.
