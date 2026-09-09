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
- Sidebar panels now move into the top bar, dock and control centre. Status,
  label and icon widgets now enter the control centre, interleaved with its native
  sections. They keep their original instances, display settings and popup
  anchors. The sidebar now accepts these widgets in its real layout, before or
  after the selected native panel. Native control-centre controls and groups
  now also move into this host, using their full panel layout. Header labels
  wrap within the sidebar width. Longer layouts scroll while preserving space
  for the selected panel, and group handles follow the scroll position.

## Next implementation boundary

Audit the wider widget interaction and popup-placement matrix, including small
screens, scale factors and wide status groups in panel containers.
Individual control-centre actions, audio, battery,
media and quick controls now inherit the same container, group and widget display
settings as their native components. The three native control groups use the same overview view
as the live control centre, with inert sample data and saved member ordering.
The sidebar, bar, dock and control centre now host
the same retained panel instances. Bar and dock buttons open the original panel
in an anchored popup; the control centre displays the full panel. Moving the last
panel out leaves an empty sidebar drop target. Adding a panel back selects it.

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

The panel-lifetime case uses native picker clicks across all six sidebar panels.
It verifies the same component and drag-preview source, unchanged panel geometry,
calendar month and date, unsent task text, and media transport controls before
and after switching. The original media controls also work in the detached
sidebar, and their instances survive editor reorder, Undo/Redo and Cancel.
Normal and reduced-motion switching pass, as do the status-sidebar regression
and the seven reload cases. A compositor screenshot verifies the terminal's
rendered text after switching back. The test exposed a terminal FBO paint
crash when making its retained item visible again. A native debugger traced it
through QQuickPaintedItem and QOpenGL2PaintEngineExPrivate::fill into Mesa's vertex
upload. The terminal now uses its supported image paint mode, preserving the
same shell process and GPU composition. This can cost more CPU during heavy
terminal output; sustained-output performance is not yet measured. The private
runner now distinguishes early process exit from a genuine timeout.

Remaining broad checks include all existing widget actions, notification
interactions after layout changes, different scale factors,
multiple monitors, keyboard use and reduced motion. Record unavailable hardware
checks as unverified, rather than treating an isolated render as equivalent.

The panel-placement case moves all six original sidebar panels through the bar,
dock, control centre and back. It checks a native Notes drag, Undo/Redo, Cancel,
original sidebar dimensions, container and widget display settings, Shift-right-click,
Notes context-menu hosting, media playback, a retained task draft and calendar date,
and the original terminal process. Normal and reduced-motion cases pass and reload
the saved placements in a fresh shell process. Screenshots verify the settled
Notes and media popups; the media popup fits its content. The status-sidebar,
full editor, external control-centre and seven reload regressions also pass.
The initial test switched
six panels before rendering them and stalled in Mesa context destruction. The
fixture now waits for each selected panel to render before the next selection,
as the native picker test already does through mouse input. That artificial rapid
selection stress case remains separate from the verified user interaction path.

The account/session, audio and quick-control palette groups now use
`ControlCentreOverview`, the same layout used by the live control centre.
`ControlGroupPreview` supplies sample device data and projects the saved group
membership and display settings. It does not register drag sources or call real
services. Previews fit both dimensions without cropping; compact groups use their
natural width. Dedicated checks cover every member, reordered and removed header
items, dock text inheritance, per-item overrides, focus isolation and source
registry stability. Rendered previews confirm the full quick-control grid and
both audio rows. The extracted native view passes bar/dock and sidebar group
movement and reload tests, and the original populated/empty geometry comparison.

Individual control previews now follow their parent group's placement, use the
full panel layout in the sidebar, and use their natural width in the dock.
The same native visual components receive container and group display modes,
per-widget label/icon overrides, and inert sample data. Quick-control tiles keep
their native background. The presentation fixture covers all fifteen individual
controls in the dock, sidebar and control centre, both independently and within
their groups. The previous implementation fails these assertions. Rendered samples
verify compact audio, media and action controls and the full sidebar quick tile.

The notification and overflow buttons now share their complete visual components
between the live shell and palette. Notifications retain the count beside custom
labels and icons. Recording and privacy samples inherit presentation settings;
clock and keyboard samples use their displayed values. Dividers stay horizontal
in panel containers, header spaces retain flexible or fixed sizing, and the
Customise button uses its full panel appearance in the sidebar. Standalone checks
cover these utility contexts and nine status widgets with container and per-widget
overrides, with normal and reduced motion. Native notification-button clicks work
after moving into the dock and control centre, and closing history retains the
count. The shared buttons keep the original live geometry, backgrounds and actions.

Tray samples now use the real tray renderer with service updates disabled and
three inert entries. Sidebar-panel samples share `SidebarPanelFace` with the
retained live panels. They show a compact launcher in the bar or dock, the headed
panel in the control centre, and panel content in the sidebar or unused palette.
Compact samples do not load hidden panel content. The fixture checks all six
panels across these contexts, label/icon overrides, and tray data isolation.
Task rows created after loading the sample now keep a non-focusable policy;
the unchanged all-widget keyboard regression catches the previous focus leak.
Rendered media samples exposed a missing heading allowance in the real inline
panel too. Both paths now include that space, with a shared height calculation.
Preview and native regressions failed before this fix. Native panel checks cover
retained instances and drafts, launcher clicks, popup contents, task input and
checkbox actions, media transport, Cancel, and saved placements after restart.

Settings now links to the real container inspector instead of changing layout,
visibility, sidebar membership or container appearance directly. The standalone
window forwards the selected container over IPC, preserving that target while
pending Settings changes save. Native checks cover the dock, sidebar, control
centre and top-left container, with normal and reduced motion. Opening and
cancelling each route leaves the saved desktop unchanged. The complete Settings
UI regression also passes, including search links to container options and the
existing search-provider configuration. The remaining Settings visual changes
in the working tree are separate from this layout-routing change.

Privacy indicators now resolve their placement through the same lookup as other
status widgets, including control-centre membership. The old lookup ignored this
container and failed both a focused appearance test and the native move test.
All four activity indicators now inherit icons, text, both or native mode and
retain per-widget overrides. A native click on the sharing control after moving
it into the control centre reaches the supplied privacy service. Normal and
reduced-motion cases pass and reload the saved layout. Privacy and recording
palette entries now use their real component types with inert sample state.
The all-widget keyboard test and existing recording, stop and timer checks pass.
These tests do not start or stop a physical capture session.

The disabled-control click failure also reproduced with native dock input.
The shared edit handler was destroyed when created before its window existed,
because its initial parent was null. It now stays attached to the widget until
the window is available. Both unchanged regressions pass, including disabled
groups, hidden widgets, movement, re-enabling and normal command dispatch.

Small-screen editing now uses a compact palette when the sidebar and control
centre leave less than 240 pixels of width, or the top sidebar leaves less than
300 pixels of height. The same preview grid opens above the real containers.
Dragging hides the palette and exposes every drop target. A footer button reopens
Widgets and becomes a Remove target during container drags. The palette's source
window remains mapped until the native drag finishes; unmapping it at pickup
can stall Qt. Container options take precedence over the palette.

The native `editor-compact` case covers preview geometry, dragging a spacer into
the actual top bar, dragging it back to Remove, reopening the palette and clicking
both tabs above the control centre, inspector access, all three sidebar edges,
and Cancel without a saved-layout change. At 800x600, the previous implementation
fails the palette-width assertion; normal and reduced-motion runs pass with the
compact palette. A compositor screenshot verifies its visible layout. Fractional
scale factors are covered below; wide status groups still need separate checks.
The 1280x800 compact-case geometry and full editor regression pass. The missed
Network click after saving was traced to the native test's coordinate mapping:
it sampled x=1002 while the bar still had its old width, then Network moved to
x=810 before delivery as the compositor acknowledged the left sidebar inset.
The action received no click. The test now waits for the sidebar's reveal and
for the acknowledged bar width to equal the screen width minus its margins,
then waits for rendering before mapping the input point. It also asserts that
the original Network action receives exactly one native click. Three consecutive
full reduced-motion runs passed with the width check.

The focused `editor-save-input` case repeats saving and clicking 12 times while
alternating an open sidebar between the left and right edges. It retains live
metrics and a flexible space before Network, closes an edit menu before each
native click, and checks the original control's action and detail page. This
covers the saved-layout input boundary without depending on the full editor
scenario. Normal and reduced-motion runs pass, as does the full normal-motion
editor regression. It does not establish input correctness during an unfinished
sidebar animation.

Privacy indicators now wrap within the sidebar and control centre when their
combined width exceeds the container. The same indicator instances retain their
normal single-row geometry in the top bar and dock. Constrained widget labels
can shrink and show an ellipsis. The narrow-panel test covers all four active
indicators, normal labels and long overrides in a 208-pixel host. The previous
privacy component fails both panel cases. Native tests move the original group
through the control centre and sidebar, click its sharing action in each host,
and restore its saved placement after restarting the shell. These use supplied
privacy state and verify the stop callback. Screenshots cover both real panel
containers and the long-label rendering; preview and keyboard checks also pass.

The shared native test runner now rereads the result file after observing process
exit. A fixture can finish and exit between polls; previously the runner could
report an incomplete test using a stale RUNNING result. A real subprocess test
fails with the old runner and passes with the final read; an early exit without
a report remains a failure.

Popup drop geometry now follows the actual Scale transform, including its origin,
width and height. Previously, the opening animation could leave the control-centre
drop origin 20.8 pixels to the right of the visible card. A native Search drag at
125% exposed this. The focused fixture covers both independent popup motion and
motion shared with another popup, including intermediate scale values.

The scale runner changes only a private compositor and private settings. It checks
Mutter's reported scale and the shell's logical screen dimensions before running
native editor and moved-widget tests. On a 1920x1200 output, 125%, 150% and 200%
cover 1536x960, 1280x800 and 960x600 logical desktops. Normal and reduced-motion
runs pass, including fresh-process restoration of the moved widgets. Screenshots
cover the compact palette, privacy groups, calendar and search at these scales.

The 200% run exposed a calendar clipping defect: its agenda extended 52 pixels
past the popup body. The agenda now shrinks when the screen is short. On still
shorter screens, the full calendar scrolls without overshoot; keyboard focus
reveals the active control. Inline sidebar calendars keep their expanding agenda.
The old calendar fails the agenda-bounds check at 600 pixels. The new fixture
covers 720-, 600- and 480-pixel hosts, wheel input, keyboard focus and agenda
scrolling, and restoring normal size. Existing calendar navigation, events,
month transitions and sidebar-calendar checks pass with normal and reduced motion.

Reduced-motion geometry assertions wait for the layout pass, which can finish
after the zero-duration reveal. The compact input fixture requests an image frame
before mapping each gesture. Waiting for an unrequested frame on an unchanged
item had consumed five seconds per check without proving that it rendered.

A fresh 200% session also passes the compact and moved-widget cases against the
staged source tree, with normal and reduced motion. The compact cases complete
in about ten seconds after requesting their frames; an earlier run had timed out.
The live reload preserves the exact desktop settings, layout and dock snapshot,
with no QML errors or shell-process restart. These scale checks use one virtual
output; they do not establish mixed-scale, multiple-monitor behaviour.

System monitors now adapt their original readout grid to panel width in the
sidebar and control centre. All nine readouts fit, with their values and graphs
kept separate. The top bar and dock retain the original two-row layout and
height. Custom label/icon presentations retain their original 32-pixel target
and constrain long labels. The palette projects the live widget's selected
readouts into the same component with sample values and the panel's width.

The narrow-host fixture covers 208- and 376-pixel panels, all readouts, long
labels, native activation, preview selection updates and restoring bar geometry.
The previous implementation fails the panel-width check. The native
`metrics-placement` case moves the original widget through the sidebar and control
centre, opens performance and configuration through pointer input, changes a
readout toggle, invokes Shift-right-click and reloads the saved placement and
monitor selection in a fresh process. Captured sample histories verify the
rendered mini graphs. Preview and keyboard-isolation regressions pass with normal
and reduced motion.

The original monitor regression now includes 80 sample process records for its
table-scroll checks. `BINGUX_HARDWARE_RECORD` can still replace these samples for
live-probe runs. Without records, it had been trying to scroll an empty list. These checks cover graph history, digit motion, sorting, filtering,
scrolling, configuration and outage states. They do not exercise process signals
against live applications. The new grid also avoids deriving its column count
from a width assigned during its own layout pass, which caused recursive
rearrangement during placement.

Staged-source placement, reload and original monitor regressions pass with normal
and reduced motion. The live reload preserves the exact desktop settings, layout
and dock snapshot, with no QML errors or shell-process restart. Tray width and
mixed-scale, multiple-monitor behaviour remain separate checks.

The system tray now wraps its retained app buttons in the sidebar and control
centre. The top bar and dock keep their horizontal row and twelve-icon width
limit, with wheel scrolling and focus reveal for offscreen items. Natural width
remains independent of panel width so wrapping cannot change overflow priority.
Long custom labels fit their targets, and the palette uses the same Tray component
with inert sample items and the placed container width. Each button owns its
entrance animation; a Flow transition could leave scale at zero when a resize
interrupted insertion.

The `tray-layout` fixture covers eighteen items in 208- and 376-pixel hosts,
left/middle clicks, wheel dispatch, keyboard activation, long labels, retained
instances, bar geometry restoration and empty state. The previous tray remains
384 pixels wide in a 208-pixel host and fails the regression. Native
`tray-placement` input drags the original tray from the control centre into the
sidebar, checks Undo/Redo, opens and activates its app menus, invokes
Shift-right-click, saves placement and verifies a fresh shell process. Menu and
app callbacks use sample service data; this does not prove a third-party tray
service's D-Bus implementation. Normal and reduced-motion checks pass, including
staged-source runs. Screenshots show all eighteen icons inside both panels.
The original hover checks, palette previews and keyboard isolation also pass.
The hover runner now copies the complete component dependencies through the
shared fixture runner and locates the tray button by name instead of ListView
internals. Live reload preserves the exact desktop, layout and dock snapshot,
with no QML errors or shell-process restart.

More now opens from its real button during Customise UI and exposes the existing
overflow widgets through the shared editing handles. It does not become a saved
container or change those widgets' placement until the user moves them. Its
outside-click layer is disabled during editing so it cannot intercept drops onto
the desktop containers. The same native window remains available to the editor,
as with the control centre; closed popup handles are inactive. The control centre
stays open while More is being edited, and leaving the editor closes More.

Overflow widgets now report their actual host window for menus, tooltips and
Shift-right-click. Opening a nested performance popup keeps More open. The More
button also follows its dock anchor instead of retaining a fixed top-bar Y
position. Inspector placement uses the widget's visible surface and clears the
whole temporary popup, below it when space permits and above it near the dock.
Tray app tooltips are suppressed during editing. Container outlines observe both
ends of their coordinate mapping so popup padding does not leave a shifted border.

The native `overflow-edit` case covers the tray's app menu and anchor, opening
More in the editor, inspector and outline bounds, native dragging into the sidebar,
retained widget identity, Undo/Redo, Cancel without a disk change, nested monitor
menus, Shift-right-click, moving More into the dock, and saved placement after a
fresh shell process. Screenshots verify clear inspector headings in both bar and
dock positions. Final staged-source checks pass with normal and reduced motion,
along with the compact-editor regression. The tray placement and keyboard
regressions also pass. Live reload preserves the exact desktop, layout and dock
snapshot without QML errors or a shell-process restart.

Two earlier staged runs timed out before the first editor capture. Diagnostic
repeats completed before the scheduled stack capture, so no stalled stack was
obtained. More now retains its registered native window between openings; the
final normal/reduced-motion and compact-editor loop passed. That does not establish
the timeout's precise cause. Keep cold-start reliability in the remaining audit.
Other open checks include mixed-scale and multiple-monitor behaviour, and
third-party tray services.

Detached popup checks found failures in the existing shell. Opening a nested
monitor popup closed More because the parent check compared its unused layer
window with the floating host. In a short floating window, the monitor popup also
reserved desktop dock space and could put its switches outside its clipped card.
The shell now checks the anchor's ancestry for inline popups, and monitor menus
use the floating host's available height. Shared popup cards and outside-click
surfaces use their nesting depth to keep child menus above their parent. Inactive
floating edit surfaces no longer read layer-window margins. The monitor tooltip
closes while its popup is open.

A native right-click on the monitor popup also opened the Notes context menu
behind it. The next outside click dismissed that hidden menu instead of More.
The shared popup card now handles context-menu events so they cannot reach the
editor behind it. A mouse-event trace identified the Notes menu's dismissal area
as the actual recipient of the click. This was not a stale coordinate or a
disabled More surface.

The `overflow-detached` case uses compositor input for More, Shift-right-click,
monitor popups and switches, outside-click dismissal and the sidebar clock. The
input helper resolves the private test window by its unique title and accounts
for Qt's title bar. The test resizes the actual compositor window to a 360-pixel
content height before exercising the switches. It also enters Customise UI,
checks the real container's layer-window host and editing bounds, cancels and
reopens More in the restored floating window. Optional screenshots do not move
the pointer. The old shell fails the parent-retention check and reports undefined
layer margins.

Staged-source `overflow-detached`, `overflow-edit` and `sidebar-layout` checks pass
with normal and reduced motion, including the latter two cases' fresh-process
reload checks. The detached case also verifies that the monitor right-click does
not open Notes' menu, while a right-click on Notes still opens its own menu.
Screenshots show the monitor controls inside the short window and above More.
The live reload preserves the exact desktop, layout and dock snapshot, with no
QML errors or shell-process restart. Core dumps remain disabled.

The narrow floating More menu now wraps the original tray items and monitor
readouts. Monitor values retain their space while the small graphs shrink.
The monitor's natural bar width remains independent of wrapping, so resizing
does not change overflow membership. More's anchor also excludes its own overflow
content dependency. This removes a binding loop when changing windows.

More scrolls vertically when its content exceeds the card height. It stops at
both ends without overshoot and reveals focused controls. The detached fixture
checks all 18 tray items and nine readouts at 384, 224 and 180 pixels, retaining
the same native items. It exercises native wheel input, focus in both directions,
child menus, monitor switches and return from Customise UI. The monitor fixture
also checks the original two-row width and restoration after wrapping.
These checks pass against staged sources with normal and reduced motion,
alongside overflow editing and sidebar save/restart cases. The live reload
preserves the exact desktop, layout and dock snapshot without QML errors or a
shell-process restart. Similarity checks found no duplicated functions in the
changed QML files or private input helper.
The wider compatibility audit remains open.

The sidebar layout regression now changes between left, top and right edges.
At each edge, native input drags the retained Clock to the bar and back, opens
its Shift-right-click actions while Notes has focus, and opens the anchored
Calendar. Assertions check the destination, retained Notes instance, absence of
an unintended Notes context menu and popup bounds. The saved widgets reload in
a fresh shell process. The scale runner also verifies the shell's logical screen
dimensions and rejects unsupported display scales before starting the matrix.

The current-source matrix passes at 125%, 150% and 200% with normal and reduced
motion on a 1920-by-1200 virtual display. The corresponding logical sizes are
1536 by 960, 1280 by 800 and 960 by 600. Screenshots confirm left, top and right
Calendar placement. The earlier 1280-by-800 test display passed at 125% but did
not support 150%; that was a test-display configuration failure, not a shell
layout failure. Staged-source checks also pass at 125% with normal motion and
200% with reduced motion, including both fresh-process reloads. Multiple
displays with different origins and scales remain outside this check.

The two-display matrix now covers horizontal and vertical arrangements at 100%
and 150%, with each display tested as the edited output and with the primary
output changed between arrangements. The dock explicitly follows the top bar's
screen. Gnoblin also keeps layer-surface output reporting tied to the assigned
display during compositor animations. The old compositor reported a neighbouring
output while the bar slid across its stage view; the dock could then remain on
that output after the bar returned. Wayland traces confirm the corrected output
assignment, and the old sources fail the real-container screen assertion.

All twelve compact-editor, sidebar-layout and overflow-edit combinations pass
with normal motion. The twelve reduced-motion combinations also pass against
staged sources. Native input covers Clock drags into the actual dock, all three
sidebar edges, nested overflow menus, and sidebar/overflow save and fresh-process
reloads. Screenshots verify the secondary display's dock, inspector and sidebar
anchors. Direct unpin actions also pass on both vertically arranged outputs with
normal and reduced motion, including saved state and editor Undo/Redo. The input
helper now preserves D-Bus error text if preparation or a gesture fails.

Run `tests/customise-monitors-live.py` inside the Gnoblin private session with
`MONITOR=1920x1200 EXTRA_MONITOR=1920x1200` and the patched compositor. Use
`--cases dock-unpin` for the app-menu check. Gnoblin's configuration guide contains
the complete invocation. The broader action audit remains open.
The live Bingux reload preserves the exact desktop, layout and dock snapshot
without QML errors or a process restart. The tested Mutter library is installed
for the next login; the current compositor continues to use its previous library.
Its eleven layer-animation lifetime cases pass in the private session.

Notes context menus now use the actual sidebar host window and the shared
screen-local coordinate mapping. Native right-click tests exposed two offsets:
the global display origin was counted twice, and global mapping omitted the
32-pixel margin of the top sidebar. The shared mapping handles both. Floating
and moved-panel menus continue to use their existing inline host.

The sidebar regression now opens Notes through native right-click at each edge,
checks the menu position and bounds, and applies Bold to the selected text.
Both horizontal and vertical arrangements pass on the 100% and 150% displays
with normal and reduced motion, including every saved-layout reload. The original
floating Notes menu, formatting, heading and keyboard checks also pass.
The live reload preserves the saved Notes file and the exact desktop, layout
and dock snapshot, with no QML errors or process restart. Remaining work is the
wider widget-action audit in the compatibility contract.

The Bluetooth action check moves the original tile into the dock, top bar and
sidebar. Native input verifies dragging, Undo/Redo, label/display overrides,
inline power switching, device-page navigation, power/discovery state, paired
device connect/disconnect, and Shift-right-click. A fresh shell restores the
saved sidebar placement and appearance. The adapter and device are test objects;
these checks do not establish a physical Bluetooth connection.

This check exposed a compositor stacking failure: the reopened control centre
could remain below the sidebar because its native window stays mapped. Gnoblin
now raises retained shell menus when they request keyboard input again, without
changing the active application. Screenshots show the complete popup above the
sidebar, and native clicks reach its power and device controls. Normal and
reduced-motion checks pass, alongside direct dock unpin, nested overflow menus
and Gnoblin's eleven layer-animation lifetime cases. Native input measurements
wait for QML layout and popup resizing to finish. The recording-status widget's
active stop/cancel/saving actions are the next unverified action group.
