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
  popup using those same instances. The account/session, audio and quick-control
  groups now move as a unit into the bar and dock. They retain their native
  children, ordering, command dispatch and detail-page anchors. Group display
  settings inherit from the host container; individual children can override
  them or move out of the group and back. The divider, header space and
  Customise button also move into the bar and dock. The space has a fixed-width
  override and can return to its original flexible header behaviour.
- `DesktopLayout.accepts()` still confines sidebar panels to the sidebar. Status,
  label and icon widgets now enter the control centre, interleaved with its native
  sections. They keep their original instances, display settings and popup
  anchors. The sidebar now accepts these widgets in its real layout, before or
  after the selected native panel. Native control-centre controls and groups
  now also move into this host, using their full panel layout. Header labels
  wrap within the sidebar width. Longer layouts scroll while preserving space
  for the selected panel, and group handles follow the scroll position.

## Next implementation boundary

Extend sidebar panels into the other real containers. The sidebar host still
selects one native panel. Extend placement validation and the real panel hosts
together. Preserve the same
component instances and service actions. The palette group previews also need
to share the complete native group composition, including display inheritance,
instead of their current representative layouts.

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

The shared edit handle accepts Shift-right-click for disabled controls and
children of disabled groups. Hidden controls stay inactive; normal clicks keep
their disabled behaviour. A native compositor gesture verifies the disabled dock
action and subsequent command dispatch after enabling it. Palette keyboard
checks cover every registered preview type: loading keeps the current focus,
and Tab cannot enter the sample controls. The previous preview implementation
fails this keyboard regression.

The native group case covers moving the original header, audio and quick-control
layouts, child extraction and reinsertion, Undo/Redo, saved placement, group
and child display settings, native command dispatch, mute and audio/network
popup anchoring. Visible group handles support dragging and click-to-configure;
the inspector follows the selected group. Both normal and reduced-motion cases
restore group placement, child membership and display overrides in a fresh
shell process. Geometry comparisons preserve the original full control centre.
Screenshots verify the compact group rendering and tooltip placement in the
dock and top bar.

Utility checks cover native divider and spacer drags, Undo/Redo, Cancel, display
overrides, fixed and original spacing, Shift-right-click and opening the editor
from the moved Customise button. Normal and reduced-motion cases save and reload
their placements in a fresh shell process. Their palette previews match the
placed utility dimensions. The unchanged group regression and original native
geometry comparison also pass.

The external control-centre case verifies native Search dragging, moving labels
between the panel and dock, all ten original status-widget instances in the
panel, container and per-widget display settings, unique decoration instances that survive cross-container moves and additions,
Cancel and saved placements after a fresh shell process. The original Clock
opens its anchored calendar, and Search opens its real separate UI process.
The Control Centre button also works from inside the panel. Normal and reduced
motion pass, as do the full editor regression, seven reload cases and unchanged
native control-centre geometry. Validation rejects duplicate placement and
oversized section lists without changing the saved file.

The sidebar case verifies a native Clock drag and its insertion before Notes,
Undo/Redo, all ten status-widget instances, labels moved through the control
centre without recreation, appearance inheritance, and saved placements after a
fresh shell process. Cancel restores the exact original panel padding and size.
Normal and reduced motion pass. The drop rectangle follows the sidebar's slide
timeline; it previously kept a stale position and could insert after Notes when
the pointer was above it. Calendar and keyboard menus, tooltips and edit actions
use the detached sidebar window. Screenshots verify the actual panel and its
detached Calendar menu. Physical input-source switching remains unverified.

The native sidebar-controls case covers an audio-group drag, every portable
control's original instance, Undo/Redo and Cancel, saved groups and child
appearance overrides, header wrapping, scrolling during editing, and hidden
offscreen group handles. Native clicks dispatch Settings, mute and change volume,
open audio and network detail pages, and play the original media widget. Detail
menus also follow the detached sidebar. Normal and reduced motion save and
reload their placements in a fresh shell process. An empty control centre keeps
a visible drop target. The bar/dock group, utility, status-sidebar, full editor
and seven reload cases pass; original control-centre geometry remains unchanged.

Restore defaults now resets native control groups, widget placement, visibility,
container and widget appearance, sidebar edge and dock behaviour in one undoable
step. It uses the settings backend's defaults and preserves pinned apps and their
order. The native reset case checks the button, Undo/Redo, Cancel without a disk
change, and fresh-process persistence with normal and reduced motion. The existing
native editor regression also passes. The sidebar picker now opens its Customise
UI inspector instead of changing the screen edge directly. Panel selection and
pop-out remain available in the picker. Its new action fits the menu without
truncation, including fractional text widths.

Remaining broad checks include all existing widget actions, notification
interactions after layout changes, different scale factors,
multiple monitors, keyboard use and reduced motion. Record unavailable hardware
checks as unverified, rather than treating an isolated render as equivalent.
