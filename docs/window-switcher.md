# Window switcher

Bingux runs the chooser in `bingux-switcher-ui.service`, separate from the
main desktop and `bingux-search-ui.service`. Neither shortcut waits for the
main UI process. Both services restart independently when they exit.

Gnoblin's `src/scripts/lib/window-switcher-fallback.js` keeps Alt+Tab and
Super+Tab registered when no UI client is connected. It keeps a window list
and selection for each gesture. On modifier release, a working UI can commit
its selection immediately. If it does not respond within 80 ms, the script
activates the selected window. With no UI connected, it activates immediately.
Escape cancels. Messages carry a gesture serial, so a recovered UI cannot
apply an old selection. Alt+Tab also asks search to close.

The fallback runs in the compositor, outside every Bingux process. It requires
a responsive compositor. It does not provide a visual chooser while the
switcher UI is unavailable.

Super invokes `SearchShell.qml` directly through `binguxctl`. While search is
open, the compositor raises the existing top bar and dock buffers above
fullscreen windows. It restores their order when the reveal ends or the search
process disconnects.
This also works when the desktop UI is paused. The panels sit above search so
pointer input reaches them. Repeated Super presses toggle search while keeping
the top bar and dock revealed and clickable. Closing search with Escape or an
explicit search-close command ends the reveal. Re-entering fullscreen or clicking
back into the fullscreen window after hiding search slides the top bar upward
and dock downward before restoring their normal stacking. Super can interrupt
the exit and reveal them again. The compositor suppresses the reveal on the lock screen.

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
background thumbnails are reused for 30 seconds. The focused window refreshes
after two seconds so its latest content is ready when switching away. Cached
images remain available until their window closes, including after a failed
refresh. There is no continuous
capture loop. These are snapshots, not live video previews. The previous image
stays visible while its replacement decodes. A window without capture content
displays its app icon. Previews fill their rounded frame without an inner inset
or letterbox bars. Scaling preserves proportions and crops overflow, aligned to
the top to retain window headings. Apps that discard buffers when minimised keep their last
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

## Process isolation regression

Run `tests/popout-isolation.py` as `GNOBLIN_TEST_DBUS_CLIENT` through Gnoblin's
`scripts/run-gnome-shell.sh`. Set `GNOBLIN_PREFIX` to the patched installation
and `QUICKSHELL_BIN` to its matching Quickshell runtime. The test uses a private
compositor, private bus and copied QML configuration. It sends real Super and
Alt+Tab events, pauses the desktop and popout processes, checks fullscreen
stacking and restoration, and checks switching after all UI processes exit.
`tests/window-switcher.py` covers the chooser's normal navigation and previews.
