# Tabs

Contributor notes for `bingux_ui::controls::tab_strip`: what the tabs look
like and why, the numbers, and the behaviour that does not depend on the look.
The component's own behaviour (keys, menus, overflow) is in the module comment of
`crates/ui/src/controls/tabs.rs` and in `docs/bingux-ui.md`.

## Design thesis

1. **Tabs are the shell's own controls, spread along the title bar.** A tab is a
   rounded rectangle of `Radius::Control`, the corner of a button, a sidebar row
   and a segmented control; the selected one has the fill of a selected sidebar
   row (`Paint::Selection`), the pointer's fill is `Paint::Hover`, a press is
   `Paint::Pressed`, and accent colour is used for nothing but a drop target.
2. **The tabs are separate from the content, and say so.** The bar is the
   window's title bar on `Paint::Surface`, the page under it is `Paint::Bg` (the
   same colour), and nothing joins the two: no flare, no shared shape, no
   darker strip, no card. The page runs square to the window's edges as the
   Settings page does, so it has no corner of its own to match. The window's
   rounding is the compositor's.
3. **Quiet and dense.** An unselected tab is muted ink on the bare bar, a tab
   is 32 px tall, 4 px from the bar's edges, 4 px from its neighbour; no
   separators, no outline, no hairline, no shadow.

## What it replaced, and why

The first version was Chromium's tab strip: the selected tab and the content
were one shape, a concave flare at each foot ran into the content's top edge, the
bar behind the tabs was one step darker (`Paint::TabStrip`), and the window's
frame was that darker colour round an inset, rounded *card*. Kieran's verdict on
it was that it looked out of place next to the rest of Bingux and that the card's
corners (square where the first tab joined them, round elsewhere, a different
radius from the window's and from every other surface) were not consistent.
Rounded corners that depend on which tab is selected cannot be made consistent:
the join under the selected tab, the card's two top corners, the window-edge
corners and the sidebar's edge are four treatments of one corner. Of the two
honest options, truly connected (with a concave join, no seam, and corners that
follow the selection) or clearly separate, we took the second, because every
other surface in the shell (the Settings page, a dialog, a popover, the segmented
control above a page) is separate from what it controls.

| Chromium | Here | Why |
| --- | --- | --- |
| Selected tab is the content's surface with flares into it | A rounded rectangle with the selection fill, an inset above the page | The rest of the shell does not join a selection to its content |
| Strip one step darker than the page | The bar is `Paint::Surface`, the page's colour | One window surface; the selected fill carries the state |
| Inset rounded card | Full-bleed page, square to the window | One corner treatment: the window's |
| Separator between inactive tabs | None | The gap and the muted ink separate them; a hover fill appears on the one under the pointer |
| Close button on the active tab and under the pointer | Same | |
| Width: shrink to a minimum, then scroll | Same: `Dim::Field` at most, `Dim::TabMin` at least | |
| Scrollable strip with a tab list, new tab button after the last tab | Same | |
| Drag follows the pointer, others slide; tear-out | Same, on `crate::reorder` and `crate::drag` | `design/tab-tearoff.md` |
| Rapid closure, close-tab menu, reopen closed tab | Same | |
| Pinned tabs, groups, split tabs | No | No caller needs them |

## Geometry

Every length is a token. The strip is `Dim::TabBar` tall and *is* the window's
title bar: the `HeaderBar` that holds it is that tall too, so the window carries
no second row above its tabs. A tab is `Dim::TabButton`: the strip less
`FIELD_INSET` above and below, so it is as tall as an end button, and at compact
density, where there is no inset, the whole 44 px touch target. Numbers are
logical pixels at mouse density, then compact.

| Name | Token | Mouse | Compact |
| --- | --- | --- | --- |
| Strip (title bar) height | `Dim::TabBar` | 40 | 44 |
| Tab height | `Dim::TabButton`, the strip less two `FIELD_INSET` | 32 | 44 |
| Band above and below a tab (drags the window) | `FIELD_INSET` | 4 | 0 |
| Tab corner | `Radius::Control` | 12 | 12 |
| Gap between tabs | `S1` | 4 | 4 |
| Width | `Dim::TabMin` .. `Dim::Field` | 96 .. 240 | 128 .. 168 |
| Icon | `IconSize::Control` | 16 | 18 |
| Title | `CAPTION`, the small type step | 13 | 13 |
| Close button | `Dim::DenseControl` / `Dim::InlineButton`, corner `Radius::Control.nested(FIELD_INSET)` | 24 | 44 |
| New tab and tab list buttons | `Dim::TabButton`, the corner of a tab, in a cluster with an inset each end | 32 | 44 |
| Icon buttons elsewhere in the bar | `Dim::TabButton` (`InTabBar`) | 32 | 44 |
| Start edge | the header bar's padding, `S2` | 8 | 8 |
| Overflow fade | `Dim::Box5`, into `Paint::Surface` | 20 | 20 |

A strip of `n` tabs of width `w` has content `n (w + gap) - gap` wide. A tab is
laid out and painted in the same box: what takes the pointer and the focus is
what is drawn, so the gap between two tabs drags the window.

The close button is centred in the tab and `FIELD_INSET` from its edges, and its
corner is the tab's less that inset (`Radius::Inner`, 8 px inside 12 px):
concentric, the nesting rule of every other control in the shell. The focus ring
is the tab's own rounded rectangle.

## Colours

| State | Fill | Ink |
| --- | --- | --- |
| Unselected | none (the bar) | `Ink::Muted` |
| Hovered (mouse only) | `Paint::Hover`, fading in over `MOVE_MS` | `Ink::Text` once past half |
| Pressed | `Paint::Pressed`, at once | `Ink::Text` |
| Selected | `Paint::Selection` over the bar | `Ink::Text` |
| Drop target | `Paint::AccentSoft` | `Ink::Text` |
| Held by a drag | its own fill, lifted over the others (faint once going on would tear it off) | |

The selected tab's fill is the selection layer composed over the bar *as one
opaque colour*: the tab in the hand of a drag slides over its neighbours' titles
and must hide them, which a translucent layer would not. Contrast: unselected
titles are `Ink::Muted` on the bar and under the hover and pressed fills; the
selected title is `Ink::Text` on the selection fill; `tabs.rs` tests every
ground in both schemes.

## The page under the tabs

The page begins at the bar's bottom edge and runs to the window's edges on the
left, right and bottom: no card, no margin, no rounding, no hairline between it
and the bar (they are the same colour). At the top corners of the page there is
nothing to round, at the bottom corners the compositor's window rounding clips it,
exactly as in Settings. Whatever is selected, at any width, with one tab or many,
with tabs dragged, in either reading direction, and in light and dark, the
page's pixels at its four corners and along its top edge are the page's colour
(`tab_tests.rs`, `the_page_runs_square_to_the_window_under_a_bar_of_its_own_colour`).
The sidebar below the bar is the shell's `Paint::Sidebar` as everywhere; its top
edge is the bar's bottom edge.

## Closing

Every tab closes, a lone one too, by: its close button, a middle click,
Delete, Ctrl+W (the strip's, or the host's when the focus is in the page), and
the menu. The strip reports the id; the host decides.

**Why Kieran could not close tabs.** Reproduced in the Files app with real
input: (1) a lone tab, which is what Files opens with, had no close slot at all
(`closable = tabs > 1`), its middle click did nothing, and Ctrl+W with the focus
on the tab was swallowed by the strip, which stops propagation before the window
shortcut can close the window; (2) an unselected tab showed its button only
under a *mouse* pointer; (3) the button was 20 px. With two or more tabs and a
mouse the X worked (checked). All three are fixed at the source: the strip no
longer special-cases a lone tab (Files closes the window for it), the button
shows on every tab at touch and compact density, and it is a 24 px icon button
(44 px at compact), inside the tab body, where a press is stopped before it can
reach the header bar's window drag.

* **Selection** (`tab_close::selection_after_close`): the tab to the right,
  else the left, else none. The host applies it (Files in `TabSet::close`).
* **Last tab.** Files closes the window, and asks first, in a dialog, only if
  a copy, move or compression is running (`files::operations_running`).
* **Rapid closure** (`tab_close::RapidClose`): a close with the pointer (button,
  middle click) holds the tab width. While the pointer stays anywhere in the
  strip's row the next tab slides under it with its close button visible (the
  tab under a pointer that did not move is shown as hovered), so the person
  can click again. When the pointer leaves the row the widths grow to the room
  over `MOVE_MS` (at once with reduced motion). Closing with the keyboard or
  the menu, or adding a tab, does not hold. The machine is pure over a clock
  passed in, tested with a fake one.
* **Reopen closed tab** (Ctrl+Shift+T and the menu): `files::ClosedTabs` keeps
  the last 10 closed tabs with their place and their `TabMemory`, so Back and
  Forward come back with the folder. Closing several (others, to the right)
  remembers them rightmost first so Reopen brings them back left to right.
* **The menu** (`tab_close::menu_rows`, pure): Close tab, Close other tabs,
  Close tabs to the left, Close tabs to the right, a separator, Duplicate
  tab, Reopen closed tab, a separator, Move left, Move right. An action that
  cannot do anything is greyed out (the moves are left out at the ends).

## Motion

* Add and close animate the widths over `MOVE_MS` (instant with reduced
  motion); a leaving tab keeps its place until it has shrunk to nothing.
* A dragged tab follows the pointer one-to-one. The others slide with the
  critically damped spring of `crate::reorder`. It settles into its place
  when let go and `on_reorder(id, to)` is called once, after it has settled.
  With reduced motion everything is at its place at once.
* Dragging past the strip's end scrolls it, at the speed `reorder::edge_speed`
  gives; Escape puts the tab back. Without an `on_detach` handler, letting go more
  than a header's height above or below the strip puts it back too; with one,
  the tab is torn off first (`design/tab-tearoff.md`).

## Where tabs appear

* **A window of views of one kind** (Files): `tab_frame(HeaderBar, content)`
  with the strip in the header bar's tab slot. The window controls share the
  row; the bar is the title bar.
* **A few fixed pages that cannot be closed** (the inspector's Elements,
  Performance, Accessibility and Layout; the Properties dialog): the segmented
  control's `tabs()` variant. It is a rail of choices, not a set of documents:
  nothing to close, reorder or scroll, and no content it is attached to.


## Not done

Pinned tabs, groups and state indicators are listed above with the reason. An
inactive tab's close button is not a keyboard stop (the selected tab is the stop;
Delete and Ctrl+W close it), so "visible on keyboard focus" is the selected
tab's.

## Behaviour notes that outlived the first design

* **The pointer.** Which tab is hovered is worked out from where the pointer and
  the tabs are (`tab_at`), on every pointer move and every draw, not from
  per-tab enter and leave events (which Freya does not send when the tabs move
  under a still pointer, nor reliably when a node's siblings change). The fill
  fades with `use_tween` on `MOVE_MS`; pressed is a stronger fill at once; a
  drag ends the pressed look explicitly (it takes the release). A tab's pieces
  are keyed so Freya never pairs one piece with another's body.
* **Scrolling.** The strip keeps its own logical offset (`controls/tab_scroll.rs`):
  wheel (the vertical one too), touchpad pan and a finger scroll it; scroll into
  view only runs when the selection or the widths change. (An effect that called
  `ScrollController::scroll_to_x` subscribed to every layout and pulled the strip
  back after each wheel notch.)
* **Titles.** `fade_text` replaces the ellipsis: the glyphs' alpha ramps to
  nothing over the last `Dim::TextFade` (24 px) at the end of the reading
  direction, so it is right on the hover fill, the selected fill and the bar, and
  the close button is never touched.
* **Layers.** Paint order in Freya is the layer, which is the depth in the
  tree, so a dragged tab is lifted by `HELD_LAYER` or it would be drawn under a
  neighbour's title.
* **Menu.** Close tab, Close other tabs, Close tabs to the left (greyed out on
  the first tab), Close tabs to the right, Duplicate tab, Reopen closed tab, Move
  left, Move right. A closed group is remembered rightmost first, so Reopen
  brings it back left to right at its places. The tab list shows each tab's icon,
  its name (cut in the middle, the whole name as the tooltip) and ticks the
  selected one.
* **Right to left.** `controls::direction`: the strip, a tab's pieces, the tab
  list rows, scroll offsets, arrow keys and the place a torn tab lands
  (`StripLayout { rtl }`) are mirrored; `tab_tests.rs` runs the geometry checks
  in both directions.
* **The insertion marker** of a tab brought from another window is the strip's
  edge at either end of the strip (there is no room outside the clipped region)
  and the gap between two tabs otherwise.
