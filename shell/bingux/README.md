# Quickshell desktop

Run this directory with `qs --path shell/bingux`. Use a Quickshell package built
against its runtime Qt version. Mixing distro Qt updates with an older
Quickshell binary can cause crashes.

`Theme.qml` defines shared colours, spacing, icon sizes and typography.
Top bar controls use full-height hit targets and hover-only backgrounds.
Dock buttons are square, with icons centred independently of their indicators.
Drag an icon to reorder it, or focus it and press Ctrl+Left / Ctrl+Right.
The order is saved through Qt Settings in `~/.config/gnoblin/dock.ini`.

Menus use `ShellPopup.qml`, a transparent layer surface with a rounded content
card. This avoids the native xdg-popup grab ordering failure in older Gnoblin
builds. Outside clicks and Escape dismiss menus. Tray submenus use the same
surface and provide a Back action. `DockTooltip.qml` provides a non-interactive
application tooltip after a short hover delay.

Click the clock for the calendar, the system indicators for the control centre,
and the search icon for the floating search panel. Search needs bingux-searchd;
application launching also needs an applicationLauncher command in search.json.
The control centre uses PipeWire for volume and the existing system settings
applications for network and display configuration.

Gnoblin compositor effects are configured in `~/.config/gnoblin/gnoblin.toml`.
The rebuilt compositor supports alpha-masked layer blur and per-window rules.
Dock icon rectangles are published through foreign-toplevel management so
minimise animations can target each icon. Those native changes need one new
login after installation. QML edits reload in the running Quickshell process.

## Verification

On the live Gnoblin session, verify:

- Calendar opens from the full-height clock target and closes outside or with Escape.
- Tray right-click opens a menu, submenus can return, and the shell stays connected.
- Dock drag ordering persists across a Quickshell restart.
- Tooltips fit their text and do not capture pointer input.
- Search shows application icons without a dim background.
- Control centre reports the current volume and device state.
- The journal has no QML load errors or Qt version mismatch warning.

The compositor repository contains isolated native tests for dock rectangles,
layer entrance animations, live rules, and pixel verification of masked blur.
Run those against the same installed Mutter and Shell prefix.
