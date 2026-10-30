# Right to left

bingux-ui reads left to right or right to left per window. The engine mirrors the tree; code is
written for left to right and read as *start* and *end*. How it works is in `docs/bingux-ui.md`,
"Direction". This page says what mirrors, what does not, and why.

Sources: Material Design's bidirectionality guidance (mirror text alignment, navigation order,
direction-bearing icons and the side icons sit on in a field; do not mirror numbers, charts and
graphs, icons that do not communicate direction, clockwise motion, physical objects such as media
players and keyboards, and the handedness of a search glass), and Apple's right-to-left guidance
(flip back and forward and the leading edge; keep playback controls, clocks and music notation).

## Mirrors

| What | How |
|---|---|
| Layout: rows, sidebar side, header controls, window controls | Torin `Node::rtl`; the close button is at the left |
| Padding, margin, absolute `left` | start and end; `pl` is the start |
| Text alignment, a trailing `.`, a caret in an empty line | `TextAlign::Start` and the paragraph direction |
| Row leading and trailing slots, chevrons, back and forward | engine, plus `#mirror_rtl` icons |
| Switch thumb, slider, progress fill | engine order; a slider's pointer and keys read `use_direction()` |
| Scrollbar and a scrolled content's start | engine (absolute `right` is the start side's mirror) |
| Border, corner radius, shadow x, linear gradients | freya-core `render_rect(direction)` |
| Menus, tabs, segmented control, tree, split pane, stepper keys | `Direction::reading_key` / `along` |
| Popup alignment, a submenu's side, the back swipe edge | `place_aligned(direction)`, `swipe_x` |
| Tab strip order, insertion index, overflow fade | engine, `StripLayout { rtl }`, `fade_text` |

## Does not mirror

| What | Why |
|---|---|
| Charts and graphs (`bingux_ui::charts`) | Material: charts and graphs are not mirrored. The plot is pixels, so its labels are too (`drawing` reads left to right) |
| The media bar and its seek bar | A player is a physical object (Material); its keys seek by the screen's arrows |
| Numbers, a clock, digits of a `NumberRoll` | Latin digits keep their order |
| Check marks, logos, search glass, slash on an off state | Do not communicate direction or are handed |
| Circular arrows, spinners | Time goes clockwise in every language |
| Colour planes and hue strips | Not text; right is more in every language |

## Icons

`#mirror_rtl` in `scripts/icons/labels.tsv`: go-next, go-previous, pan-end, sidebar-show, send-to,
view-list, help-browser, emblem-symbolic-link. `Icon::mirrors_in_rtl()` must equal it (a test).

## Checks

* `make layout-check` runs `layout-lint.py --rtl`: each capture is taken left to right and right to
  left and every box must be the reflection (`direction-asymmetric`). A box that must not mirror is an
  `ALLOWED` entry with the reason.
* `controls/direction_tests.rs`, `torin/tests/rtl.rs`, `a_right_to_left_strip_is_mirrored`.
* Look: `BINGUX_DIRECTION=rtl BINGUX_PSEUDO_LOCALE=ar`.

## Known gaps

Text editing in a right-to-left field aligns and places the caret from Skia's paragraph, but
pointer hit-testing in an overflowing field was not checked; the bidi order of a mixed run is
Skia's. Scrub (a number line) is deliberately physical.
