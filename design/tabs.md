# Tabs

Contributor notes for `bingux_ui::controls::tab_strip` and `tab_frame`: what
model the tabs follow, which parts of it we took, and the numbers. The
component's own behaviour (keys, menus, overflow) is in the module comment of
`packages/bingux-ui/src/controls/tabs.rs` and in `docs/bingux-ui.md`.

## The model: Chromium's tab strip

Chromium draws a tab as part of the content it opens. The selected tab is the
same surface as the toolbar and page under it; its outline rises from the
strip's bottom edge, rounds two top corners and comes back down, and nothing is
drawn along its bottom, so the tab and the content are one shape. Every other
tab is recessed into a darker strip. Reading the sources:

* **The strip is behind the tabs and the page is in front of the strip.** The
  design document for the strip describes tabs that *overlap by a small amount
  (about 20 px)* and a z-order that must be kept consistent so the overlap
  looks the same however the selection changes, a layout loop that walks the
  tabs left to right, and a *New Tab Button* placed after the last one
  ([Tab Strip Design (Mac)](https://www.chromium.org/developers/design-documents/tab-strip-mac/)).
* **The tab is a path, not a rounded box.** `HorizontalTabStyleViews::GetPath`
  builds the selected tab's outline going clockwise from the lower left: it
  starts in the overlap region *"preventing a gap between toolbar and
  tabstrip"*, draws the bottom-left **extension** corner as an arc of the
  *bottom* corner radius (counter-clockwise: a concave flare), runs up the
  ascender, rounds the top left with the *top* radius, crosses, and mirrors
  down the right side. The outline stroke is inset by half its width and the
  stroked path (`kBorder`) is open at the bottom, so the line follows the tab
  and stops at the feet
  ([`horizontal_tab_style_views.cc`](https://github.com/chromium/chromium/blob/main/chrome/browser/ui/views/tabs/common/horizontal_tab_style_views.cc)).
  The tab also extends a little *into* the toolbar below it
  (`kTabstripToolbarOverlap`) so no gap appears at fractional display scales.
* **Selected, hovered and inactive tabs are different shapes.** The highlight
  path for a hovered or inactive tab is a detached rounded rectangle inset
  from the strip's bottom, not the flared outline; only the selected tab joins
  the content.
* **Radii and widths** are in `tab_style.cc`: the 2023 style has a *top*
  corner radius of 10 and a *bottom* (flare) radius of 12; the tab *overlap* is
  twice the bottom radius less the separator, so neighbours' feet share the
  space between them; the separator is 2 px wide and 16 px tall with a 2 px
  margin each side, and the older GM2 style used 1 px; the standard tab width
  is 240; the *minimum active width* is a close button plus the content
  insets, and the *minimum inactive width* lets a tab shrink until its
  interior is 16 px; a pinned tab is 24 px of content plus insets
  ([`tab_style.cc`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/chrome/browser/ui/tabs/tab_style.cc)).
* **The width algorithm** (`tab_strip_layout.cc`) gives every tab a minimum,
  a *crossover* (the active tab's minimum) and a preferred width. With little
  room the inactive tabs shrink between their minimum and the crossover; with
  more, all tabs grow from the crossover towards the preferred width. The
  fractional pixels the rounding leaves are handed back, left to right, to the
  tabs that can take one
  ([`tab_strip_layout.cc`](https://github.com/chromium/chromium/blob/main/chrome/browser/ui/views/tabs/tab_strip_layout.cc)).
* **Behaviour the Mac design document records.** Closing with the close
  button keeps the tab widths fixed until the pointer leaves the strip, so the
  next close button lands under the pointer ("rapid closure mode"). A tab being
  dragged follows the pointer and the others get out of its way; a placeholder
  gap marks where it would land. Leaving the strip by a margin tears the tab
  out into its own window. A new tab "submarines" into place from below.
  A window with one tab and nowhere to drop it drags the window instead.

## What we take, and why

| Chromium | Here | Why |
| --- | --- | --- |
| Selected tab is the content's surface, outlined from the strip's bottom edge, no line under it | Yes: one path, filled with the content's paint, stroked open at the bottom | It is the point of the design: the tab says which content it opens |
| Concave flares at the feet | Yes, radius = top radius less `FIELD_INSET` | Reuses the nested-radius rule; the flare and a close button inset by the same amount agree |
| Strip one step darker than the content | Yes: `Paint::TabStrip` | The recess that makes the selected tab read as raised |
| Hovered or inactive tab is a detached pill, not the flared shape | Yes: `Hover`, `Pressed` and `AccentSoft` fills, inset from the strip's bottom | Only the selected tab joins the content |
| Separator between inactive tabs, hidden next to the selected and hovered tab | Yes | Tells neighbours apart without borders; hides where a fill or the outline already does |
| Neighbouring tabs overlap by two feet | The feet of two tabs are the *same* space: bodies are one foot apart | The flare of a selected tab then never reaches a neighbour's body, so a hover fill next to it is never bitten by it |
| Width: preferred 240, shrink to a minimum, active tab wider | One shared width, `Dim::Field` at most, `Dim::TabMin` at least; scroll beyond | Our tabs always reserve a close slot, so the active tab needs no extra room; a readable minimum beats Chromium's 16 px sliver |
| Scrollable strip with a tab list | Yes: fades at the edge, tab list button, selected tab scrolled into view | Same as Chrome's scrollable strip |
| New tab button after the last tab | Yes, pinned after the tabs, never scrolls away | |
| Drag follows the pointer, others slide | Yes, on `crate::reorder` and `crate::drag` | The shared drag primitive the sortable list uses |
| Width animation on add/close | Yes | |
| Rapid closure mode | No | Tabs here are 96 to 240 px wide and the close button is not at the tab's centre; revisit if closing many tabs is slow |
| Pinned tabs, tab groups, split tabs | No | No caller needs them; the API has no place for them yet |
| Tear-out into another window | No | A Bingux app window is one window; there is no window to drop into |
| Favicon, loading spinner, audio and attention indicators | An icon only | `TabItem` carries an icon; state indicators wait for a caller that has state to show |

## Geometry

Every length is a token. The frame is `Dim::Header` tall and the tab is
`Dim::Control` tall, so a tab is as tall as a header button, and at compact
density it is the 44 px touch target. The numbers are logical pixels at mouse
density, then compact.

| Name | Token | Mouse | Compact |
| --- | --- | --- | --- |
| Strip height | `Dim::Header` | 48 | 56 |
| Tab height | `Dim::Control` | 36 | 44 |
| Band above the tabs (drags the window) | strip less tab | 12 | 12 |
| Top corner radius | `Radius::Control` | 12 | 12 |
| Flare radius, foot width and gap between bodies | `Radius::Control.nested(FIELD_INSET)` | 8 | 12 (no inset) |
| Body width | `Dim::TabMin` .. `Dim::Field` | 96 .. 240 | 128 .. 168 |
| Close button | `Dim::Box5` / `Dim::InlineButton`, radius `Radius::Control.nested(FIELD_INSET)` | 20 | 44 |
| New tab and tab list buttons | `Dim::InlineButton`, same radius | 28 | 44 |
| Outline and seam line | `Dim::Hairline` | 1 | 1 |
| Separator | `Dim::Indicator` x `Dim::Box5`, `Paint::MutedFaint` | 2 x 20 | 2 x 20 |
| Overflow fade | `Dim::Box5` | 20 | 20 |

A strip of `n` bodies of width `w` has content `foot + n * (w + foot)` wide:
a foot before the first tab, a body and a foot after each. The first tab's
left flare and the last tab's right flare are those outer feet, so a tab never
changes its layout when it becomes selected; only what is painted changes.

A tab is drawn in a slot a foot wider than its body on each side, so the
flares fit inside the painted box (a painted background is clipped to its box).
The *body* inside the slot is the control that takes the pointer and the
focus; the feet take nothing, so the gap between two tabs drags the window.

### The path

Local to the slot, with `f` the foot (flare radius), `r` the top radius, `T`
the tab height, `w` the body width and `l` the line width, all on the middle of
the line (half a line inside the edge):

```
move  (0, T - l/2)                         left foot, on the strip's hairline
arc   centre (0, T - l/2 - (f + l/2))      flare, counter-clockwise, to the wall
line  (f + l/2, ...)                       up the wall
arc   top left corner, radius r - l/2
line  across the top
arc   top right corner
line  down the right wall
arc   centre (f + w + f, ...)              flare, counter-clockwise, to the foot
[fill only] line (f + w + f, T), (0, T), close
```

The fill is the closed path, in the content's paint. The stroke is the open
path, `Paint::Separator`, one `Dim::Hairline` wide. The strip's own hairline
runs along the bottom edge of the frame's bar; the tab's line starts and ends
on the middle of it, so the two are one line that dips up and over the tab.

## The colour rule

> The selected tab and the content area directly under it share **one surface
> paint**, taken from the same value; the strip behind them is **one step
> darker**.

* `tab_frame` owns the value. It paints its content in `surface` (default
  `Paint::Bg`) and provides the same value to the `TabStrip` inside it, which
  fills the selected tab with it. A strip with no frame uses the default.
* `Paint::TabStrip` is the darker step. Light: `0xebebed`, the same step the
  sidebar takes below the `0xfafafb` page. Dark: `0x18181b`, below the `0x222226`
  page (the dark sidebar is a step *lighter*, so it cannot be reused).
* Contrast: unselected titles are `Ink::Muted` on the strip, and under the
  hover and pressed fills; they must be 4.5:1 (`contrast_matrix` lists
  `TabStrip` among the layers, and `tabs.rs` tests the hover and pressed
  grounds). The selected title is `Ink::Text` on the surface.
* Never fill a tab with a colour of its own, and never put a border on the
  bottom of the selected tab or the top of the content. Both break the rule.

## Seam

The bar's bottom edge is the content's top edge: the bar is `Dim::Header`
tall, the content starts right after it, and the selected tab's body ends on
that edge (a layout test asserts the two areas touch). The painted tab ends on
the same edge. The line below the other tabs is the frame's hairline, laid in
the bar's last row; the tab paints over it with the surface, which is why the
bar's children must be transparent. Both rows are whole logical pixels, so
there is no gap at fractional scales that keep the header on a whole pixel.

## Motion

* Add and close animate the widths over `MOVE_MS` (instant with reduced
  motion); a leaving tab keeps its place until it has shrunk to nothing.
* A dragged tab follows the pointer one-to-one. The others slide with the
  critically damped spring of `crate::reorder`. It settles into its place
  when let go and `on_reorder(id, to)` is called once, after it has settled.
  With reduced motion everything is at its place at once.
* Dragging past the strip's end scrolls it, at the speed `reorder::edge_speed`
  gives; Escape, or letting go more than a header's height above or below the
  strip, puts the tab back.

## Where tabs appear

* **A window of views of one kind** (Files): `tab_frame(HeaderBar, content)`
  with the strip in the header bar's start slot. The window controls share the
  row.
* **A few fixed pages that cannot be closed** (the inspector's Elements,
  Performance, Accessibility and Layout; the Properties dialog): the segmented
  control's `tabs()` variant. It is a rail of choices, not a set of documents:
  nothing to close, reorder or scroll, and no content it is attached to.

## Not done

Rapid closure mode, pinned tabs, groups, tear-out and state indicators are
listed above with the reason. Chromium's tab colours also have separate values
for an inactive window (the `frame_active` argument in `tab_style.cc`); the
strip here has one set.
