# Bingux documentation

Design rationale and rules live in `../design/`. This directory documents the
product and how to build it.

## Using the desktop

| Page                                                 | Topic                                      |
| ---------------------------------------------------- | ------------------------------------------ |
| [settings.md](settings.md)                           | The Settings application and its addresses |
| [files.md](files.md)                                 | The Files application and the file chooser |
| [calendar.md](calendar.md)                           | The calendar popout                        |
| [capture.md](capture.md)                             | Screenshots and screen recording           |
| [emoji-picker.md](emoji-picker.md)                   | The emoji picker                           |
| [window-switcher.md](window-switcher.md)             | The window switcher                        |
| [notes-slash-commands.md](notes-slash-commands.md)   | Slash commands in the sidebar notes        |
| [desktop-customisation.md](desktop-customisation.md) | Desktop layout and preferences             |
| [login-screen.md](login-screen.md)                   | The login screen                           |
| [locking.md](locking.md)                             | Session locking                            |
| [audio-effects.md](audio-effects.md)                 | Sound effects                              |
| [troubleshooting.md](troubleshooting.md)             | Common problems                            |

## How the shell works

| Page                                                                                                                         | Topic                                        |
| ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| [architecture.md](architecture.md)                                                                                           | Layers, processes and what owns what         |
| [desktop-shell.md](desktop-shell.md)                                                                                         | The shell contract                           |
| [configuration.md](configuration.md)                                                                                         | The configuration store                      |
| [binguxctl.md](binguxctl.md)                                                                                                 | The `binguxctl` command                      |
| [portals.md](portals.md)                                                                                                     | Portal backends and permissions              |
| [window-frames.md](window-frames.md)                                                                                         | Window frames                                |
| [background-effects.md](background-effects.md)                                                                               | The background blur protocol                 |
| [panel-blur-fades.md](panel-blur-fades.md)                                                                                   | Panel blur during fades                      |
| [activity-indicators.md](activity-indicators.md)                                                                             | Recording and privacy indicators             |
| [control-centre-design.md](control-centre-design.md)                                                                         | The Control Centre                           |
| [dock-menus.md](dock-menus.md)                                                                                               | Dock application menus                       |
| [popup-consistency.md](popup-consistency.md), [popup-loading.md](popup-loading.md), [sidebar-loading.md](sidebar-loading.md) | Popup and panel behaviour                    |
| [search-presentation.md](search-presentation.md)                                                                             | How search results are presented             |
| [extensions.md](extensions.md), [extensions-plan.md](extensions-plan.md)                                                     | Writing extensions, and the plan behind them |

## Design system and session manager

| Page                         | Topic                                           |
| ---------------------------- | ----------------------------------------------- |
| [bingux-ui.md](bingux-ui.md) | The design system: components, tokens and rules |
| [bsm/](bsm/architecture.md)  | The Bingux Session Manager                      |

## Building and releasing

| Page                                           | Topic                                                |
| ---------------------------------------------- | ---------------------------------------------------- |
| [standalone.md](standalone.md)                 | Installing the shell without a package               |
| [installer.md](installer.md)                   | Installing Bingux OS                                 |
| [package-management.md](package-management.md) | Packaging                                            |
| [release.md](release.md)                       | Cutting a release                                    |
| [desktop-testing.md](desktop-testing.md)       | The isolated desktop test framework, coverage and CI |
| [portable-testing.md](portable-testing.md)     | Testing outside Gnoblin                              |
| [code-quality.md](code-quality.md)             | Code quality checks                                  |

`archive/` keeps dated measurement reports.
