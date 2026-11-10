# Tabs

Contributor notes for `bingux_ui::controls::tab_strip`: what the tabs look
like and why, the numbers, and the behaviour that does not depend on the look.
The component's own behaviour (keys, menus, overflow) is in the module comment of
`crates/ui/src/controls/tabs.rs` and in `docs/bingux-ui.md`.

## Design thesis

1. **Tabs float.** A tab is a neutral tile of `Radius::Tab` (10 px, a little
   rounder than a button is square), no border, `Dim::TabButton` (30 px) tall
   in the 40 px strip. The chosen one is the raised tile (`Paint::Raised`);
   the others have no fill at rest and a faint hover layer (`Paint::Hover`);
   a press is `Paint::Pressed`; accent colour is used for nothing but a drop
   target. No tab joins the toolbar or the page: a tile never grows to the
   strip's height and never takes the chrome colour.
2. **The strip is its own tier.** The tabs stand on `Paint::Strip`, a tone off
   both the chrome (`Paint::Sidebar`) and the content (`Paint::Bg`), because
   they sit outside the browsing frame (`design/boundaries.md`).
3. **Natural width, left-aligned, never stretched.** A tab is as wide as its
   icon (16), title and close circle need, `Dim::TabMax` (200 px) at most. All
   tabs of a strip share one width (the widest title's), so the stride is even
   for reordering and scrolling. With few tabs the strip stops after the New tab
   button, which follows the last tab, and the rest of the bar is empty and
   drags the window. Only when the tabs no longer fit do they share the strip
   and shrink together: the title fades first (`fade_text`, never an ellipsis),
   then, under `Dim::TabTitleMin` (96 px), each tab is its favicon only (the
   close circle shows on the chosen tab and under the pointer, in the favicon's
   place when the two do not fit side by side), down to `Dim::TabMin` (36 px),
   and then the strip scrolls with fade edges while New tab moves to a fixed place
   at the strip's end beside a tab-list button.
4. **A faint hairline** (1 px, 16 px tall) stands in the gap between two
   adjacent tabs when neither is chosen, hovered, pressed, a drop target or
   arriving or leaving; it is hidden while a tab is dragged.

## What it replaced, and why

The first version was Chromium's tab strip: the selected tab and the content
were one shape, a concave flare at each foot ran into the content's top edge, the
bar behind the tabs was one step darker (`Paint::TabStrip`), and the window's
frame was that darker colour round an inset, rounded _card_. Kieran's verdict on
it was that it looked out of place next to the rest of Bingux and that the card's
corners (square where the first tab joined them, round elsewhere, a different
radius from the window's and from every other surface) were not consistent.
Rounded corners that depend on which tab is selected cannot be made consistent.
Of the two honest options, truly connected or clearly separate, we took the
second, because every other surface in the shell is separate from what it
controls. A later pass tried the first again (the selected tab grown to the
strip's height in the toolbar's colour); the final model is the second option
only: floating tiles that never connect.

| Chromium                                                            | Here                                                                                      | Why                                                                     |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| Selected tab is the content's surface with flares into it           | A raised neutral tile on the strip, 5 px from its top and bottom                          | The rest of the shell does not join a selection to its content          |
| Tabs share the whole strip up to a cap                              | Natural width up to 200 px, left-aligned; they share the strip only when it is full       | A few tabs are not stretched into slabs; the empty bar drags the window |
| Narrowest tab keeps a title                                         | Favicon only under 96 px, down to 36 px, then scroll                                      | The icon identifies a site at any width                                 |
| Inset rounded card                                                  | Full-bleed page, square to the window                                                     | One corner treatment: the window's                                      |
| Separator between inactive tabs                                     | A faint 1 px, 16 px hairline, hidden beside the chosen or hovered tab                     | Tabs read as separate with no fill at rest                              |
| Close button on the active tab and under the pointer                | A 16 px close circle (`Dim::TabClose`), same rule                                         |                                                                         |
| Scrollable strip with a tab list, new tab button after the last tab | Same: New tab follows the last tab; when scrolling it is fixed at the end with a tab list |                                                                         |
| Drag follows the pointer, others slide; tear-out                    | Same, on `crate::reorder` and `crate::drag`                                               | `design/tab-tearoff.md`                                                 |
| Rapid closure, close-tab menu, reopen closed tab                    | Same                                                                                      |                                                                         |
| Pinned tabs, groups, split tabs                                     | No                                                                                        | No caller needs them                                                    |

## Geometry

Every length is a token. The strip is `Dim::TabBar` tall and _is_ the window's
title bar: the `HeaderBar` that holds it is that tall too, so the window carries
no second row above its tabs. A tab is `Dim::TabButton` (30 px, centred, 5 px
above and below, which still drag the window); at compact density, where there is
no band, the whole 44 px touch target. Numbers are logical pixels at mouse
density, then compact.

| Name                                          | Token                                         | Mouse  | Compact |
| --------------------------------------------- | --------------------------------------------- | ------ | ------- |
| Strip (title bar) height                      | `Dim::TabBar`                                 | 40     | 44      |
| Tab (tile) height, and an end button's side   | `Dim::TabButton`                              | 30     | 44      |
| Band above and below a tab (drags the window) | (strip less tab) / 2                          | 5      | 0       |
| Tab corner                                    | `Radius::Tab`                                 | 10     | 10      |
| Gap between tabs                              | `S0_5`                                        | 2      | 2       |
| Widest                                        | `Dim::TabMax`                                 | 200    | 200     |
| Icon-only under                               | `Dim::TabTitleMin`                            | 96     | 120     |
| Narrowest (favicon)                           | `Dim::TabMin`                                 | 36     | 44      |
| Icon                                          | `IconSize::Control`                           | 16     | 16      |
| Title                                         | `CAPTION`, the small type step                | 13     | 13      |
| Close circle                                  | `Dim::TabClose`, `Radius::Pill`, 12 px glyph  | 16     | 44      |
| Hairline between tabs                         | `Dim::Hairline` wide, `SEPARATOR_HEIGHT` tall | 1 x 16 | 1 x 16  |
| New tab and tab list buttons                  | `Dim::TabButton`, in a cluster                | 30     | 44      |
| Start edge                                    | the header bar's padding, `S2`                | 8      | 8       |
| Overflow fade                                 | `Dim::Box5`, into `Paint::Strip`              | 20     | 20      |

A strip of `n` tabs of width `w` has content `n (w + gap) - gap` wide; a tab's
natural width is `lead (8) + icon + gaps (4) + title + close circle + the band
round it (7)`, the widest title's, at most 200 (`Geometry::natural`,
`natural_width`). A tab is laid out and painted in the same box: what takes the
pointer and the focus is what is drawn, so the gap between two tabs drags the
window.

The close circle is centred in the tile, the same distance (7 px) from its top,
bottom and end, so it is concentric with the tile. The focus ring is the tab's
own rounded rectangle and shows for keyboard focus only (a click or a tap
leaves the keyboard where it was).

## Colours

| State                | Fill                                                                         | Ink                        |
| -------------------- | ---------------------------------------------------------------------------- | -------------------------- |
| Unselected           | none (the bar)                                                               | `Ink::Muted`               |
| Hovered (mouse only) | `Paint::Hover`, fading in over `MOVE_MS`                                     | `Ink::Text` once past half |
| Pressed              | `Paint::Pressed`, at once                                                    | `Ink::Text`                |
| Selected             | `Paint::Raised`, the raised tile                                             | `Ink::Text`                |
| Drop target          | `Paint::AccentSoft`                                                          | `Ink::Text`                |
| Held by a drag       | its own fill, lifted over the others (faint once going on would tear it off) |                            |

The selected tab's fill is `Paint::Raised`, one opaque colour: the tab in the
hand of a drag slides over its neighbours' titles and must hide them, which a
translucent layer would not. Contrast: unselected titles are `Ink::Muted` on the
strip and under the hover and pressed fills; the selected title is `Ink::Text` on
the raised tile; `tabs.rs` tests every ground in both schemes.

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
under a _mouse_ pointer; (3) the button was 20 px (it is now a 16 px circle, `Dim::TabClose`, with the whole 44 px target at compact). With two or more tabs and a
mouse the X worked (checked). All three are fixed at the source: the strip no
longer special-cases a lone tab (Files closes the window for it), the button
shows on every tab at touch and compact density, and it is a 24 px icon button
(44 px at compact), inside the tab body, where a press is stopped before it can
reach the header bar's window drag.

- **Selection** (`tab_close::selection_after_close`): the tab to the right,
  else the left, else none. The host applies it (Files in `TabSet::close`).
- **Last tab.** Files closes the window, and asks first, in a dialog, only if
  a copy, move or compression is running (`files::operations_running`).
- **Rapid closure** (`tab_close::RapidClose`): a close with the pointer (button,
  middle click) holds the tab width. While the pointer stays anywhere in the
  strip's row the next tab slides under it with its close button visible (the
  tab under a pointer that did not move is shown as hovered), so the person
  can click again. When the pointer leaves the row the widths grow to the room
  over `MOVE_MS` (at once with reduced motion). Closing with the keyboard or
  the menu, or adding a tab, does not hold. The machine is pure over a clock
  passed in, tested with a fake one.
- **Reopen closed tab** (Ctrl+Shift+T and the menu): `tabs::ClosedTabs` keeps
  the last 10 closed tabs with their place and their content (for Files its
  `TabMemory`, in a `files::FileTab`), so Back and
  Forward come back with the folder. Closing several (others, to the right)
  remembers them rightmost first so Reopen brings them back left to right.
- **The menu** (`tab_close::menu_rows`, pure): Close tab, Close other tabs,
  Close tabs to the left, Close tabs to the right, a separator, Duplicate
  tab, Reopen closed tab, a separator, Move left, Move right. An action that
  cannot do anything is greyed out (the moves are left out at the ends).

## Motion

- Add and close animate the widths over `MOVE_MS` (instant with reduced
  motion); a leaving tab keeps its place until it has shrunk to nothing.
- A dragged tab follows the pointer one-to-one. The others slide with the
  critically damped spring of `crate::reorder`. It settles into its place
  when let go and `on_reorder(id, to)` is called once, after it has settled.
  With reduced motion everything is at its place at once.
- Dragging past the strip's end scrolls it, at the speed `reorder::edge_speed`
  gives; Escape puts the tab back. Without an `on_detach` handler, letting go more
  than a header's height above or below the strip puts it back too; with one,
  the tab is torn off first (`design/tab-tearoff.md`).

## Where tabs appear

- **A window of views of one kind** (Files): `tab_frame(HeaderBar, content)`
  with the strip in the header bar's tab slot. The window controls share the
  row; the bar is the title bar.
- **A few fixed pages that cannot be closed** (the inspector's Elements,
  Performance, Accessibility and Layout; the Properties dialog): the segmented
  control's `tabs()` variant. It is a rail of choices, not a set of documents:
  nothing to close, reorder or scroll, and no content it is attached to.

## Not done

Pinned tabs, groups and state indicators are listed above with the reason. An
inactive tab's close button is not a keyboard stop (the selected tab is the stop;
Delete and Ctrl+W close it), so "visible on keyboard focus" is the selected
tab's.

## Behaviour notes that outlived the first design

- **The pointer.** Which tab is hovered is worked out from where the pointer and
  the tabs are (`tab_at`), on every pointer move and every draw, not from
  per-tab enter and leave events (which Freya does not send when the tabs move
  under a still pointer, nor reliably when a node's siblings change). The fill
  fades with `use_tween` on `MOVE_MS`; pressed is a stronger fill at once; a
  drag ends the pressed look explicitly (it takes the release). A tab's pieces
  are keyed so Freya never pairs one piece with another's body.
- **The focus.** A click or tap selects a tab and leaves the keyboard where it
  was, normally in the page: click a tab, then type, and the text goes into the
  page. Only keyboard navigation in the strip (arrows, Home, End) moves the
  focus onto a tab, and after a close from the keyboard the newly selected tab
  takes it. A host that wants its page focused after a pointer switch focuses
  it itself when the selection changes.
- **Scrolling.** The strip keeps its own logical offset (`controls/tab_scroll.rs`):
  wheel (the vertical one too), touchpad pan and a finger scroll it; scroll into
  view only runs when the selection or the widths change. (An effect that called
  `ScrollController::scroll_to_x` subscribed to every layout and pulled the strip
  back after each wheel notch.)
- **Titles.** `fade_text` replaces the ellipsis: the glyphs' alpha ramps to
  nothing over the last `Dim::TextFade` (24 px) at the end of the reading
  direction, so it is right on the hover fill, the raised tile and the strip,
  and the close circle is never touched. Under `Dim::TabTitleMin` the title is
  dropped altogether (`Geometry::icon_only`) when the strip has cut it.
- **The row's offset.** The scrolled tabs are placed by an absolute `left` of the
  row, not by `offset_x` of its parent: Torin applied `offset_x` again, on top,
  whenever one tab under it was laid out alone (the pointer entering a tab), which
  moved the whole strip by half the scroll.
- **Layers.** Paint order in Freya is the layer, which is the depth in the
  tree, so a dragged tab is lifted by `HELD_LAYER` or it would be drawn under a
  neighbour's title.
- **Menu.** Close tab, Close other tabs, Close tabs to the left (greyed out on
  the first tab), Close tabs to the right, Duplicate tab, Reopen closed tab, Move
  left, Move right. A closed group is remembered rightmost first, so Reopen
  brings it back left to right at its places. The tab list shows each tab's icon,
  its name (cut in the middle, the whole name as the tooltip) and ticks the
  selected one.
- **Right to left.** `controls::direction`: the strip, a tab's pieces, the tab
  list rows, scroll offsets, arrow keys and the place a torn tab lands
  (`StripLayout { rtl }`) are mirrored; `tab_tests.rs` runs the geometry checks
  in both directions.
- **The insertion marker** of a tab brought from another window is the strip's
  edge at either end of the strip (there is no room outside the clipped region)
  and the gap between two tabs otherwise.
