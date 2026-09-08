# Customise UI remaining work

The goal remains active. The compatibility contract is in
[customise-ui-compatibility.md](customise-ui-compatibility.md). Passing the current
layout tests does not prove that every existing desktop widget is editable.

## Gaps confirmed in the current source

- The control centre now imports its native sections, header and audio rows into
  `desktop.controlLayout`. These can be reordered, removed and restored with the
  original instances. Header action buttons can now move into the bar and dock,
  use presentation overrides, and keep their Shift-right-click actions. The audio,
  media, battery and group containers still need that portability and editing support.
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

Extend audio, media, battery and group widgets into the other real containers
without flattening native header, audio, tile and media layouts. Use the same component instances
and service actions. Add presentation overrides and shared edit actions to these
controls. Improve the group drag affordance while retaining direct child drags.

## Current verification

The full native layout suite and seven reload cases pass with the current
compositor bridge staged by `tests/private_shell.py`. The fullscreen regression
checks actual compositor order, native palette input and cancellation without
changing the app's fullscreen state. These tests cover the implemented widgets;
the additional control-layout case covers native header/audio drags, removal,
Undo/Redo, previews, saving and cancellation. The dock-unpin case verifies direct
mouse actions in both normal and editing menus and their saved state.

Remaining broad checks include all existing widget actions, dock media and
notification interactions after layout changes, different scale factors,
multiple monitors, keyboard use and reduced motion. Record unavailable hardware
checks as unverified, rather than treating an isolated render as equivalent.
