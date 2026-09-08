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

The window order stays fixed during each gesture. Each task window has its own
card, including multiple windows from one app, minimised windows, and listed
transient windows. Closed windows disappear without changing the identity of
the selected window when it remains open.

Quick taps switch without showing the chooser. Holding the shortcut shows
window previews after 40 ms. Opening takes 40 ms and closing takes 60 ms;
the selection highlight slides between cards in 70 ms. Activation does not wait for an
animation or thumbnail. `BINGUX_REDUCED_MOTION=1` disables these animations.

The shell prepares one cached thumbnail after window focus settles for 250 ms.
Further requests run while the chooser is shown, after navigation has
settled for 120 ms. Each visible window is captured at most once per gesture;
a cached thumbnail under two seconds old is reused. There is no continuous
capture loop. These are snapshots, not live video previews. The previous image
stays visible while its replacement decodes. A window without capture content
displays its app icon. Apps that discard buffers when minimised keep their last
valid cached image; if none was captured, they show an icon until an image is
available. Captures do not change focus or raise windows.

The compositor scales textures on the GPU before reading thumbnail pixels and
encodes PNGs asynchronously. Quick taps do not construct icon or preview
delegates. Window title and focus updates preserve existing delegates, so badge
animations do not restart. The continuous carousel scrolls at its edges and
rebases equivalent copies at wraparound; it does not replace pages.

## Shared presentation

The dock and switcher both instantiate `AppIcon.qml`. Its inputs are `group`
(`id`, `desktopEntry`, and `windows`), `activeStreams`, `notifications`, and
`implicitSize`. It owns the OS icon renderer, audio badge, notification badge,
app accent, and matching rules. Consumers can read `playingAudio`,
`notificationCount`, `appNotifications`, `tooltipText`, `accentColor`, and
`accentForeground`. The switcher uses the dock's existing audio stream source.

Both surfaces use `TooltipBubble.qml` for tooltip rendering. The switcher adds
the full window title to the shared app/activity text.

## Configuration

Create `~/.config/bingux/switcher.json` before starting Bingux:

```json
{
  "enabled": true,
  "showDelay": 40
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

`tests/window-switcher-ui.py`, run through the same launcher with `QS_TEST_BIN`,
checks badge matching, delegate identity during window updates, tooltip content,
rendered animation progress, and rapid carousel wraparound. The keyboard test
also checks preview pixels and that a held chooser stops capturing. Repeat
with `BINGUX_REDUCED_MOTION=1` to check the immediate presentation path.
