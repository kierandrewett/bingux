# Customise UI remaining work

The goal remains active. The compatibility contract is in
[customise-ui-compatibility.md](customise-ui-compatibility.md). Passing the current
layout tests does not prove that every existing desktop widget is editable.

## Gaps confirmed in the current source

- The control centre now imports its native sections, header and audio rows into
  `desktop.controlLayout`. These can be reordered, removed and restored with the
  original instances. Header action buttons can now move into the bar and dock,
  use presentation overrides, and keep their Shift-right-click actions. Volume and
  microphone sliders also move to these containers with their original nodes,
  mute, scrolling and device navigation. The battery display also moves to the
  bar and dock, supports appearance overrides and uses the same component in
  the palette. The media player now moves into both containers with its original
  artwork, title and transport buttons. Its full controls open in an anchored
  popup using those same instances. Group containers still need portability.
- `DesktopLayout.accepts()` confines sidebar panels to the sidebar. Status,
  label and icon widgets cannot enter the control centre or sidebar.
- `TerminalSidebar.qml` still offers ordinary position changes outside Customise
  UI. The requested exception is ordinary dock application rearrangement.
- `WidgetEditHandle.qml` inherits the control's disabled state. An unavailable
  control therefore cannot receive Shift-right-click through that handle.
- Palette previews block pointer actions but still need keyboard isolation from
  their internal controls. Sample media data must not expose callable controls.
- Restore defaults resets the arrangement. Container and widget options need a
  defined reset that is reversible through Undo and Cancel.

## Next implementation boundary

Extend group widgets into the other real containers
without flattening native header, audio, tile and media layouts. Use the same component instances
and service actions. Add presentation overrides and shared edit actions to these
controls. Improve the group drag affordance while retaining direct child drags.

## Current verification

The full native layout suite and seven reload cases pass with the current
compositor bridge staged by `tests/private_shell.py`. The fullscreen regression
checks actual compositor order, native palette input and cancellation without
changing the app's fullscreen state. These tests cover the implemented widgets;
the additional control-layout case covers native header/audio drags, removal,
Undo/Redo, previews, saving and cancellation. Battery checks cover native
movement, absent-device editing, display overrides, saved placement, status
updates and Shift-right-click. The dock-unpin case verifies direct
mouse actions in both normal and editing menus and their saved state. The audio
case checks native dragging, Undo/Redo, appearance overrides, saved placement,
mute after hovering, slider clicks, wheel adjustment and device popup anchoring.
Audio nodes are supplied by the test; physical device switching remains unverified.

Media checks cover native dragging, Undo/Redo, persistence, both bar placements,
playback, seeking, elapsed-time toggling, player switching, artwork expansion,
presentation overrides and popup edit anchoring, with normal and reduced motion.
The original full-card geometry and standalone video/player-switcher tests pass.
The D-Bus MPRIS regression verifies transport, keyboard and wheel seeking,
disconnection and album art fetched once across reopening and metadata changes.
Dock notification retention, badges and vertical dismissal also pass. The icon
fixture verifies Discord identity resolution, not the unavailable native icon
renderer.

Remaining broad checks include all existing widget actions, notification
interactions after layout changes, different scale factors,
multiple monitors, keyboard use and reduced motion. Record unavailable hardware
checks as unverified, rather than treating an isolated render as equivalent.
