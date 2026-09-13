# Panel blur fade coverage

`SurfaceFade` sends animation opacity with the rendered Wayland buffer. Its
optional native plugin is `Bingux.Effects`. A missing protocol leaves the UI
usable, but independent buffer fades require a compositor with this protocol
for correct blur. Build the plugin with the same Qt version as Quickshell.

The helper enables a temporary Qt layer only during a visible fade. This
applies opacity once to overlapping children. At rest it restores the original
layer setting. Shadowed panels use a padded composition item so the shadow and
card share the fade without clipping the normal shadow extent.

| Surface family                                                               | Fade path                                                                             |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Calendar, controls, metrics, keyboard, tray, menus, overflow                 | Shared `ShellPopup`; compositor owns whole-window transitions when policy matches     |
| Inline menus in floating windows                                             | `ShellPopup` buffer metadata, with decoration-coordinate correction                   |
| Emoji picker                                                                 | Shadowed `ShellPopup`; card and shadow share the composition                          |
| Bar and dock tooltips                                                        | Compositor fade when policy matches; `TooltipBubble` metadata for inline/fallback use |
| Search and file preview                                                      | Separate fade regions and grouped shadows in the shared search buffer                 |
| Window switcher                                                              | Compositor close fade; metadata for client-side fallback                              |
| Notification cards and group backgrounds                                     | Independent regions in `NotificationStack`                                            |
| Sidebar corners                                                              | Shared `ShellCorner` helper                                                           |
| Capture toolbar                                                              | Toolbar opacity metadata; capture options use `ShellPopup` where applicable           |
| Settings, bar, dock, attached sidebar, detached sidebar and capture feedback | No whole-item client fade; retain compositor animation and geometry paths             |

The protocol accepts 32 visible regions per window. The plugin excludes
off-window and ancestor-clipped regions and reports capacity overflow. Do not
register an ancestor and a child for the same opacity. Do not register ordinary
icon, text or hover opacity as a panel fade.

`tests/panel-blur-fade.py` checks whole-window close frames.
`tests/shared-buffer-fades.py` checks independent fade values and reversal on
real components, including a decorated floating host and shadowed panels.
Run them in the Gnoblin private-compositor harness, not Qt's offscreen platform.
`tests/popup-fade-input.py` also checks button input through native, inline and
shadowed popup compositions at full and partial opacity.
