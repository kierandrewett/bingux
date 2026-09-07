# Window switcher

Bingux implements Alt+Tab in `shell/bingux/WindowSwitcher.qml`. Quickshell owns
the UI, recent-window order, selection, and activation decisions. Gnoblin's
generic compositor bridge provides window IDs and input sessions. Gnoblin has
no separate switcher script.

- Alt+Tab selects the previous window. Further Tab presses move forward.
- Alt+Shift+Tab moves backwards. Super+Tab and Super+Shift+Tab also work.
- Release the held modifier to activate the selected window.
- Arrow keys change selection. Enter activates. Escape cancels.
- Click an icon to activate it. Click outside to cancel.

The window order stays fixed during each gesture. Closed windows disappear
without changing the identity of the selected window when it remains open.
Transient dialogs are represented by their parent. Quick taps switch without
showing the chooser; holding the shortcut shows icons after 80 ms. There is no
process launch or animation on the switching path.

## Configuration

Create `~/.config/bingux/switcher.json` before starting Bingux:

```json
{
  "enabled": true,
  "showDelay": 80
}
```

Changes reload automatically. `showDelay` accepts integer milliseconds from 0
to 500. Invalid settings retain the last valid configuration. The Nix desktop
module installs this file with these defaults; override its Home Manager
`xdg.configFile."bingux/switcher.json".text` value to change them declaratively.

The Nix module also installs Gnoblin's `compositor-bridge.js` script and clears
the four GNOME `switch-applications` / `switch-windows` forward and backward
bindings. For a manual installation, link the bridge into
`~/.config/gnoblin/scripts/`, run `gnoblinctl reload-scripts`, and clear these
keys in Gnoblin's TOML configuration:

```toml
[keybindings.wm]
switch-applications = []
switch-applications-backward = []
switch-windows = []
switch-windows-backward = []
```

If broad compositor rules apply blur or slide animations to layer surfaces,
exclude the transparent switcher overlay:

```toml
[[window-rules]]
match.layer = "^bingux-switcher$"
animation = "none"
opacity = 1.0
blur = 0
```

`ShortcutSession.qml` is reusable by other Quickshell components. It maintains
a persistent socket, registers accelerators, forwards input events, and
reconnects after script reload. It contains no switcher ordering or UI.

## Validation

Run the integration test in Gnoblin's private headless session, with adjacent
Bingux and Gnoblin checkouts:

```sh
GNOBLIN_COMPOSITOR_SOCKET=/tmp/bingux-switcher-test.sock \
GNOBLIN_PREFIX="$PWD/install" \
GNOBLIN_TEST_DBUS_CLIENT="$PWD/../bingux/tests/window-switcher.py" \
scripts/run-gnome-shell.sh
```

The test uses real virtual-keyboard events, three Foot windows, and Quickshell.
It checks recent-window order, reverse navigation, cancellation, rapid
switching, window closure, script reload, Quickshell restart, and configuration
recovery. `QUICKSHELL_BIN` can select a wrapper for the desired Quickshell
runtime. The test reports Alt-release-to-focus time from the compositor clock;
this measures focus changes, not display scanout latency.
