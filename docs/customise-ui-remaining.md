# Customise UI remaining work

The goal remains active. The compatibility contract is in
[customise-ui-compatibility.md](customise-ui-compatibility.md). Passing the current
layout tests does not prove that every existing desktop widget is editable.

## Gaps confirmed in the current source

- `ControlCentre.qml` still fixes the account, battery, settings, session and lock
  controls in a header row. The output and microphone sliders and the media card
  also have fixed placement. Only the seven quick-control tiles are registered
  in `movableWidgets`.
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

Import the remaining control-centre content into the saved layout before making
it movable. Preserve the existing header, audio rows, tile grid and media card
on migration. Their native grouping and spacing must be represented explicitly;
adding names to the tile order must not flatten or rearrange the current UI.
Use the existing component instances and service actions in the new containers.
Cover moving, removing, restoring, saving and reloading these items with real
native input and the compositor bridge connected.

## Current verification

The full native layout suite and seven reload cases pass with the current
compositor bridge staged by `tests/private_shell.py`. The fullscreen regression
checks actual compositor order, native palette input and cancellation without
changing the app's fullscreen state. These tests cover the implemented widgets;
they do not cover the fixed components listed above.

Remaining broad checks include all existing widget actions, dock media and
notification interactions after layout changes, different scale factors,
multiple monitors, keyboard use and reduced motion. Record unavailable hardware
checks as unverified, rather than treating an isolated render as equivalent.
