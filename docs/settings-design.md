# Settings window

Bingux Settings follows GNOME's sidebar and boxed preference list patterns:

- https://developer.gnome.org/hig/patterns/nav/sidebars.html
- https://developer.gnome.org/hig/patterns/containers/boxed-lists.html

Use the existing Bingux Theme and controls. The sidebar has a persistent selection
and the page header contains the title and Apply action. Revert is available only
when there are unsaved changes. There is no permanent footer or branding slogan.

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
