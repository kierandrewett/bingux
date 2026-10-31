# UI motion coverage

What holds every frame of every motion in `crates/ui`, what does not, and what the in-motion suite
found. Written 2026-10-31 with the suite itself. Run it with `make ui-motion`; what it proves and
cannot see is in `docs/bingux-ui.md`, "In-motion tests". Paths are under `crates/ui/src/` unless
they start with `scripts/`.

**Reading the matrix.** *Before* is what existed when the suite was written, by test or probe name.
*Now* is `motion_stress/<file>` (new: frame by frame, 16 ms a step, scale 1 and 2, light and dark,
spam, interruption, reduced motion). *Gap* is what nothing checks on a frame. A control with an
empty *Now* and a gap is not covered, and the table says so.

## Matrix

| Control or motion | Source | Before | Now | Gap |
|---|---|---|---|---|
| Switch: thumb travel 100 ms, width 80 ms | `controls/switch.rs` | `controls/hit_area_tests.rs` (hit area, hover fill, box does not move; no frames) | `motion_stress/switch.rs` (10): thumb inside the track, on the centre line, monotone, exact end, 100 ms, start latency, spam, reversal from the current x, hover in and out, keys and pointer interleaved, reduced motion, thumb colour in both states and every accent | thumb width tween (pressed 2 px) is bounded, not curved |
| Checkbox, radio: fill 100 ms | `controls/checkbox.rs`, `controls/radio.rs` | none | none | whole control. The tick is drawn at once while the fill fades in; not measured |
| Slider: knob scale 120 ms, drag capture | `controls/slider.rs`, `controls/capture.rs` | `controls/colour_picker/live_tests.rs` (the picker's strips) | `motion_stress/slider.rs` (7): knob on the line and inside the travel at every size, fill ends at the knob, drag past both ends, hover in and out on every frame, press and release, keys and pointer interleaved, travel ends at `max`, fluid slider in a resizing container, reduced motion | the app's value changing under a drag (no control animates its own value) |
| Slider row (label, track, readout) | `rows.rs` `LevelRow` | `layout-lint.py` `truncated` rule on captured pages | `motion_stress/slider_row.rs` (4): label whole when the row has room, readout on the slider's line, a longer value moves nothing, sliders of a card start at one x, at six column widths and on every frame of a resize | phone layout (title line over track line) is only held by `layout-lint.py` |
| Dropdown list (popover) 90 ms | `controls/dropdown.rs`, `shell/panel.rs` | `controls/dropdown_rows_tests.rs` (every row's label is drawn while opening) | `motion_stress/dropdown.rs` (7): fade in place, monotone, duration, close and unmount, close half way, open and close on every frame, open over a scrolling page, open by the window's bottom edge, keys and pointer interleaved | the phone's sheet at 2x and light; the scale of the card (a transform) |
| Context menu, menu cascade | `shell/menu.rs`, `shell/panel.rs` | `shell/menu_first_frame_tests.rs`, `shell/menu_cascade_tests.rs`, `shell/menu_over_dialog_tests.rs` | shares the dropdown's panel motion | fade of a context menu, submenu open delay and cone |
| Tooltip 120 ms after a 500 ms wait | `controls/tooltip.rs` | `controls/tooltip_tests.rs` (8, frames), `controls/tooltip_layout_tests.rs`, `scripts/tooltip-check.py` | `motion_stress/tooltip.rs` (5): one place, monotone fade, inside the window and off its control, no flash for a passing pointer, a press does not move or dim it, never two strong tips, reduced motion | label change while shown (held by `tooltip_tests.rs`) |
| Toast 140 ms, queue, hold | `shell/overlays.rs`, `shell/banner.rs` | `shell/notice_tests.rs` (hover hold, queue, geometry) | `motion_stress/toast.rs` (4): fade and 8 px rise along one curve, lands in time, page does not move, one at a time in order, pointer over it, reduced motion | dismiss, swipe away, queue advance (they run on the 4 s real clock) |
| Banner (inline) | `shell/banner.rs` + `reveal_if` | `motion_tests.rs` reveal frames | via expander and reveal | none of its own |
| Sheet (phone dropdown, form sheets) | `shell/sheet_motion.rs`, `shell/panel.rs` | `shell/sheet_motion_frame_tests.rs` (7: open, close, release, spring back, turn round, scrim) | none | scale 2, light scheme |
| Alert dialog 120 ms | `shell/dialog.rs`, `shell/panel.rs` | none for motion | `motion_stress/dialog.rs` (5): fade in place at desktop and phone, close and unmount, close half way, open and close on every frame, reduced motion | the card's scale; scrim fade |
| Tabs: add, close shrink, drag, slide aside, scroll | `controls/tabs.rs` | `controls/tab_tests.rs` (about 55, several frame-level: widths held while closing, closing shrinks, dragged tab follows) | none | scale 2, light scheme, interrupted drags |
| Segmented control | `controls/segmented.rs` | `reveal` of the selected segment into view only; selection is an instant fill swap | none | whole control (it has no animation to measure; its box and the rail's must not move on a change) |
| Text field, search field | `controls/text_field.rs`, `controls/search_field.rs` | `controls/text_field_focus_tests.rs` | none | typing moves nothing; error line reveal; clear button |
| List rows (hover, press, selected) | `rows.rs`, `style/recipes.rs` | `shell/rest_fill_tests.rs`, `scripts/state-check.py` | pointer crossing rows while the page scrolls (`motion_stress/scroll.rs`) | hover fill timing |
| Expander, `reveal`, `reveal_if`, tree branches | `motion.rs`, `rows.rs`, `controls/tree_view.rs` | `motion_tests.rs` (9 reveal frames), `expander_tests.rs` (14), `controls/tree_motion.rs` (6) | `motion_stress/expander.rs` (7): rows below move by exactly the opened height, monotone, 160 ms, close, toggling on every frame at four gaps, turn round half way, keyboard, reduced motion | the chevron's rotation (a transform) |
| Reorder, drag a row or tab | `reorder.rs`, `controls/sortable.rs`, `drag.rs` | `controls/sortable_tests.rs` (13) | none | scale 2, light scheme |
| Scroll view, wheel glide | vendored `freya-components/src/scrollviews` | `wheel_accel.rs` unit tests, `files/scroll_tests.rs`, `scripts/scroll-continuity.py` | `motion_stress/scroll.rs` (4): one turn glides to the distance sent and stops, a burst never overshoots and a turn back starts from the content's place, both ends hold, rows keep their spacing with the pointer crossing them | scrollbar thumb and its fade (MO-09), keyboard scroll, touch fling |
| Spinner, indeterminate bar | `controls/progress.rs` | `controls/progress_tests.rs` (11), `scripts/loop-check.py` | none | light scheme, 2x |
| Animated number | `motion/animated_number.rs` | `motion/animated_number_tests.rs` (16), `scripts/number-roll-check.py` | none | none known |
| Icon morph | `motion/icon_morph.rs` | `motion/icon_morph_tests.rs` (6) | none | none known |
| Page push and pop | `shell/navigation.rs`, `nav/gesture.rs` | `shell/page_motion_tests.rs` (5: curve, mirrored close, swipe, fling), `nav/slide_users_tests.rs`, `scripts/nav-slide.py`, `scripts/mobile-walk.py` | `motion_stress/page_push.rs` (7): curve at every scale and scheme, push and back on every other frame at three gaps, back half way through a push, wheel during a push, reduced motion, settled frame after a spam of pushes | resize mid-slide (no harness resize), predictive back at 2x |
| Sidebar selection (cross-fade of root pages) | `shell/navigation.rs` | none | none | whole motion |
| Focus ring in and out | `motion.rs` `FocusRing` | `focus/ring_anim_tests.rs` (9), `focus/ring_layout_tests.rs`, `scripts/focus-ring-check.py` | `motion_stress/focus_ring.rs` (4): tab spam at four gaps leaves one whole ring on the right control, back and forth on every frame, ring sent away half way, ring always around its face and no further than its motion reaches, reduced motion | the ring's blur and radius (shadows) |
| Charts draw, skeleton pulse, status line slide | `charts/view.rs`, `files/skeleton.rs`, `files/status_line.rs` | `scripts/loop-check.py` (skeleton) | none | decorative; not measured |

Counts: the suite adds 61 frame tests in `motion_stress/` (dialog 5, dropdown 7, expander 7, focus ring 4, page push 7, scroll 4, slider 7, slider row 4, switch 10, toast 4, tooltip 5) and 2 unit tests of the series checks. Final run: 63 passed in 204 s at 8 threads on a loaded machine (4 min 37 s with the build).

## Invariants the suite holds

Each is a function of the recorded frames (`motion_stress/series.rs`, `frames.rs`) and is applied to
every frame of a test, in every condition of the test.

| Class | Invariant | Where |
|---|---|---|
| Stillness | a box the motion does not own does not move (all nodes, by path) | `unmoved` in every file |
| Elision | no text changes whether it is cut | `elision_flips` |
| Path | a value goes between its two ends: monotone, inside them, no jump beyond one curve step, ends exactly | `monotone_between_ends`, `within`, `no_jump`, `ends_at` |
| Colour | no frame draws a colour that is not the start, the end or between them | `colour_path` |
| Interruption | after a turn the value continues from the one on screen and moves only towards the new target | switch, dropdown, dialog, expander, page push, scroll, focus ring |
| Spam | any number of toggles at any spacing ends on the last state, and the settled frame equals a calm run's | switch, expander, page push, focus ring |
| Duration | the motion lasts its specified time at 16 ms a frame, within one frame for an ease-out tail; it begins within five frames | switch, expander, dropdown, dialog, tooltip, toast, page push |
| Reduced motion | no frame between the two ends | one test per control |
| Geometry | thumb and knob on their centre line and inside their track; popovers, dialogs and toasts inside the window | switch, slider, dropdown, toast |
| Layout | label whole when the row has room; value on the slider's line | `slider_row.rs` |
| Contrast and look | the on thumb is the knob colour for every accent; polarity of both thumbs agrees in the dark scheme | `switch.rs` |

## Findings fixed

Each was found by a test that failed on the old code. Before and after evidence is in the commit messages.

1. **Window size taken from the window system as logical (every HiDPI window).**
   `PlatformWindow::root_size` is the size in physical pixels (`freya-winit/src/renderer.rs`,
   `window.inner_size()`); fifteen places in `crates/ui` and the taxonomy's switcher read it as
   logical. At scale 2 a push slid its page from 2200 logical px away in a 1100 px window: the page
   was off screen for the first 11 of 20 frames, then crossed the window in 8. The same number
   decides the narrow breakpoint, how far a menu, popup or dialog may reach, a tooltip's room and
   the drag-to-scroll edge. Fix: `surface::window_size()` and `window_size_peek()` divide by the
   scale; every caller uses them. Test: `page_push::a_push_slides_the_subpage_in_along_one_curve_in_every_scale_and_scheme`,
   `dropdown::a_list_opened_by_the_bottom_edge_stays_inside_the_window_on_every_frame`.
2. **A touchpad's scroll moved the content twice as far at scale 2.** The wheel's pixel delta is
   physical (the cursor next to it is divided by the scale in `freya-core/src/events/emittable.rs`)
   and was added to a logical offset. Fix in the vendored `freya-core` (listed in
   `vendor/README.md`). Test: `scroll::one_wheel_turn_glides_to_where_it_was_sent_and_stops_there`
   (400 physical px moved the content 400 logical px at 2x).
3. **Switch thumb: dark navy when on, white when off (dark scheme).** The on thumb was
   `Paint::OnAccent`, an ink derived from the accent, so it was a different colour for every
   accent (`#000e28` on blue in the dark scheme, `#fff8f8` on red in the light one) while the off
   thumb was the text colour; the component's own comment, `ControlSwitch.qml` and the slider's
   knob all say white. Fix: `controls/switch.rs` `thumb_paint(true)` is `Paint::Knob`, with the
   knob's soft shadow on both thumbs. Tests: `switch::the_thumb_keeps_one_look_in_both_states_in_both_schemes`,
   `switch::the_on_thumb_is_the_same_colour_on_every_accent`. **A design call to confirm:** white
   on the dark scheme's accents is 2.3:1 to 2.8:1 (the navy was 7:1); see Open findings.
4. **Slider row: label cut with room to spare, value on a line of its own.** `LevelRow` gave the
   title 34 % of the row: in a 404 px row "Delay before repeat" (131 px) had 119 px and was cut while
   the track had 184 px more than it needs. The value sat in the track's column, under the slider,
   at every width, and a new value changed that column's height. Fix: the track and its readout
   take a fixed 55 % (`TRACK_SHARE`) so the sliders of a card still start at one x, the title has
   the rest, and the readout sits beside the track in a `Dim::SliderValue` slot (72 px). Tests:
   `slider_row::*` (four, at six column widths).
5. **Phone: Back during a push jumped the page.** On a compact window a pop is a predictive
   back, which starts from the page at rest: with the push 84 % of the way, the page snapped back
   330 px to its place and left from there. A push during a predictive pop likewise restarted from
   off screen. Fix: `shell/navigation.rs`: a pop that interrupts a push is a `Motion::Pop` from
   the page's place (drawn as the push reversed), and a push that interrupts a predictive pop
   starts from the lifted screen's x. Tests: `page_push::pushing_and_going_back_on_every_other_frame_never_jumps_and_ends_where_the_input_ends`,
   `page_push::going_back_half_way_through_a_push_starts_from_where_the_page_is`.

## Open findings

- **Input to motion takes three frames (48 ms).** Every tween (switch, slider knob, reveal,
  checkbox) starts the frame after its state changes, and the vendored animation hook counts the
  first tick for nothing (`vendor/freya-animation/src/hook.rs`, `MAX_TICK` and `first_tick`): a
  switch's 100 ms travel stands still for its first 48 ms with the track already recoloured. The
  tests hold it to five frames (`LATENCY`); lowering it is a decision about the hitch the patch
  was made for.
- **White thumb contrast on the dark scheme.** The white knob on the dark scheme's accent fills is
  2.33:1 (yellow) to 2.76:1 (green); the matrix's 3:1 target for marks cannot be met by any thumb
  that looks like the off thumb, because the accent is already as light as the layers allow
  (`theme.rs`, `fit_fill`). The off thumb is 9.6:1 and 12:1. Options: keep (as the slider
  knob already is), a deeper accent for the switch track only, or an outline on the thumb.
- **Rapid Tab leaves a trail.** Each ring fades out over 120 ms on its own, so five Tabs on five
  frames draw five rings at once. By design in `ring_anim_tests.rs`; the suite only holds one
  whole ring at rest and two at most for Tabs 8 frames apart.
- **The tree snapshot cannot see transforms or shadows.** Chevron rotation, a presence's scale
  and every glow are not in the layout, so no test here covers them.

## Not run

The probes `make ui-motion` calls (`loop-check.py`, `number-roll-check.py`, `nav-slide.py`,
`frame-gate.py`, `structure-stability.py`, `scroll-continuity.py`, `focus-ring-check.py`,
`tooltip-check.py`) need the Settings, Taxonomy and Files binaries built with `--features snapshot`;
they were not built or run in this change. The whole `bingux-ui` lib suite was run once (1711 passed,
7 ignored, 3 failed): two `tab_tests` that pass alone and fail only at 8 threads on a loaded machine
(`a_tab_let_go_beyond_the_threshold_in_one_move_is_torn_as_it_is_released`,
`closing_a_tab_with_its_button_shrinks_it_away_and_the_others_widen`), and `slide_users_tests`, which
the suite's own files tripped and which now skips `motion_stress/`; those three pass in a rerun of
their own. The older frame suites in `UI_MOTION_TESTS` ran at scale 1 only
(`BINGUX_SNAPSHOT_SCALE=2` was not tried on them).
