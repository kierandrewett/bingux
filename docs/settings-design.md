# Settings window

Bingux Settings follows GNOME's sidebar and boxed preference list patterns:

- https://developer.gnome.org/hig/patterns/nav/sidebars.html
- https://developer.gnome.org/hig/patterns/containers/boxed-lists.html

Use the existing Bingux Theme and controls. The sidebar has a persistent selection
and the page header contains the title and Undo action. Changes save automatically;
Undo restores the previous batch of changes, including after it has been saved. There is no permanent footer or branding slogan.

Centre preference groups in a column capped at 600 pixels. Related settings share
one surface, with inset separators and no separator after the final row. Section
headings explain the grouping; descriptions are short and use Theme.fontSmall.

ControlRow and ControlSwitch own switch rows. Clicking either the row or its
switch changes the setting once. Only the switch is a keyboard focus stop.
ActionButton, IconButton and ControlCentreButtonSurface retain the shell's usual
hover, pressed and focus states. Entry rows use permanent labels above their values.
Advanced executable configuration is an expander, using ControlRow's optional
navigationRotation property for the chevron.

At widths below 740 pixels the sidebar becomes an overlay, opened from the header.
Selecting a page dismisses it. Scrollbars overlay the page margin, so row width does
not change between pages. Page fades and chevron motion honour reduced motion.

Run `bash tests/bingux-settings.sh` for persistence, validation, row and switch
interaction, keyboard navigation, the advanced expander and the narrow layout.
Run the same check with `BINGUX_REDUCED_MOTION=1` for the reduced-motion path.

Search detail pages use the window header for their title and back navigation.
Engine default and removal actions belong in the engine editor. Filtering has an
explicit empty state. SettingsGroup owns separators and SettingsField owns label,
value, focus and validation styling; pages must not recreate those controls.

The header title stays fixed during saves. The Undo tooltip reports save progress. Keep the form usable during
backend saves, queue newer edits, and keep unsaved drafts when the window is reopened. A helper
that exits without a response must leave a visible error and preserve the draft.

The standalone `bingux-settings` launcher uses settings.qml and the Bingux.Settings
Qt plugin to set the desktop identity before creating its window. It reuses one
process and preserves drafts when closed. The desktop entry and binguxctl launch it.
The Qt plugin must be built against the same Qt version as Quickshell.

Client decorations follow libadwaita: a 47 px header, 34 px window control targets,
24 px circular backgrounds, 3 px spacing and themed symbolic icons. Rounded corners
and a soft shadow disappear when maximised. Native Wayland content margins exclude
the shadow from window geometry, and the shadow does not intercept pointer input.
Reference: https://github.com/GNOME/libadwaita/blob/main/src/stylesheet/widgets/_header-bar.scss

Section changes slide in navigation order; detail pages enter from the right and
return to the left. Motion uses short eased translation and opacity transitions,
and is disabled for reduced motion. Pages reload their data after QML hot reload.

Native margins must follow the compositor's configured size, not Qt's requested
window state. Updating them from the maximised-property notification commits an
intermediate resize and prevents Mutter's normal scale animation. The platform
plugin applies pending margins from width/height notifications instead.
`tests/settings-maximize.py` checks actual compositor transition samples in a
private Gnoblin session, in both directions.

SettingsRow owns preference geometry: 48 px for a single line and 64 px with a
subtitle, with identical text/control insets across pages. Do not add page-local
row heights or extra horizontal padding. Section gaps are 24 px; a heading and
its group share an 8 px gap. SettingsReveal owns expander height and opacity
animation, including reduced motion. Header text reserves the actual action and
window-control widths so editing status cannot overlap controls in narrow windows.

The sidebar starts with Desktop, Control Centre and Search. Their overview rows open detailed
pages; the header back button and Alt+Left return to the parent category. Global
search is toggled by the sidebar header button or Ctrl+F. Its field slides below
the header and results replace the sidebar list, retaining the current page. It
matches setting names and related terms, shows category paths, and
opens the matching page. Specific results scroll to and focus their control.

DesktopSettings owns the Desktop overview, Top Bar, Sidebar and Control Centre
pages. Optional controls use the existing desktop.controlCentre schema. Sidebar
panel changes preserve the rest of the layout and require one panel to remain.
Search providers and website engines have separate pages under Search.
