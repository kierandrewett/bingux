# Customise UI compatibility contract

Status: implementation in progress. This document records the required behaviour,
not a claim that these checks have passed.

## Existing desktop is the baseline

Import the effective runtime layout before changing how the shell loads it.
Keep the original settings files and runtime snapshot as a migration backup.
The import must preserve top-bar order, resolved dock application identities,
pin order, sidebar edge and panel order, and enabled control-centre widgets.
Do not replace the user's layout with the widget registry defaults.

The persisted layout must have a schema version. The shell and editor must read
the same saved layout. Import is a one-time operation, not a fallback that can
overwrite subsequent changes. Reject unknown versions and invalid layouts without
replacing the last valid file. Cancel must not write the preview.

## Editing model

The wallpaper-backed editor uses the desktop edges as drop targets. An open
control centre sits beside the central widget palette, like Firefox's overflow
panel. Drag and drop is the primary action. Keep secondary controls compact.

Installed applications can be dragged into the dock. Existing status widgets,
including the control-centre button, can move between supported containers.
Their existing menus must follow their actual anchor, including a dock anchor.
Container display modes are icons, text, or both. Individual widgets can inherit
the container mode or override it. Labels and icons are editable, with an icon
grid. Shift-right-click exposes Customise, Move and Remove actions.

Container layout editing is restricted to Customise UI. Ordinary dock application
reordering and pinning remain available outside the editor.

## Verification gates

1. Capture the current shell through `shell layoutSnapshot` IPC and back up its
   source settings. Compare the imported model with that snapshot.
2. Load the imported model without edits. Verify the same widget order, app pins,
   dimensions, visibility, sidebar content and control-centre contents.
3. Exercise real widget controls before and after moving them. Cover control-centre
   detail pages, audio sliders and mute, network and Bluetooth actions, clock and
   calendar, search, input source selection, tray menus, privacy and capture.
4. Cover dock app launch/focus/minimise, multiple windows, pinning and reordering,
   media controls and seek, notification previews and retained notification counts.
5. Verify popup placement with each sidebar edge, dock alignment, small screens,
   overflow and different scale factors. Test keyboard use and reduced motion.
6. Verify drag insertion, incompatible drops, drag cancellation, removal, display
   inheritance, per-widget overrides and icon selection in the editor.
7. Save, close, reopen and restart the shell. Compare the loaded model with the
   saved model. Test failed writes, malformed files and migration idempotence.

Use private compositor sessions for tests that change layout or service state.
Record unavailable hardware checks as unverified. A mapped window or a successful
unit test alone does not prove that an existing widget still works.
