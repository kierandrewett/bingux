# Shared popup appearance

Calendar, system monitor and Control Centre inherit their outer appearance from
`ShellPopup.qml`. Keep surface tint, border strokes, radius, background effect,
shadow policy and settled pixel alignment in that parent. Full panels use
`Theme.popupPadding`; compact menus may explicitly use smaller content insets.
Content size and layout can differ without introducing a separate popup skin.

The September 13 consistency pass removed redundant appearance bindings from
those three panels, removed notes/sidebar radius overrides and the emoji-specific
tint/shadow, restored the parent surface for the media details popout, and moved
the launch-error dialog onto `ShellPopup`.

## Audited consumers

- Calendar, Control Centre, system monitor and emoji picker.
- Notes context menu, tray menu and input-source selector.
- Dock application menu, search application menu and top-bar overflow.
- Hardware and service menus, capture choice menu and snap picker.
- Sidebar widget popouts, sidebar content menu and media details.
- Launch-error dialog and dynamically created extension popups.

Notification history deliberately uses a surface-less `ShellPopup` to position
its existing notification cards; the embedded sidebar calendar is also
surface-less because its host provides the frame. Neither is a second floating
panel skin. Tooltips, OSDs, the search/switcher overlays, screenshot selection,
power transitions and detached application windows serve different roles.

## Verification

Live calendar, Control Centre, emoji and system-monitor captures were inspected after
reload. All three retain the shared surface and crisp settled edges.
`tests/popup-motion.qml` includes fractional X/Y placement coverage, passing
with reduced motion. Launch-error tests pass, including queued errors and late
recovery. `git diff --check` passes. Similarity checking found no duplicate
JavaScript functions; QML popup inheritance and overrides were inspected manually.

The full animated popup fixture fails opacity/scale settling checks in this
session. The broader Control Centre fixture also reports visibility/action
failures (audio row, notification action; reduced-motion run also power action).
These suites are not green and their causes have not been established by this
appearance pass. The three named panels and emoji have live visual proof;
the remaining consumers were audited in source, with launch errors additionally
covered by their fixture.
