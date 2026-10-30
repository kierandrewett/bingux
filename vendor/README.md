# Vendored Freya crates

Copies of `freya-core`, `freya-winit`, `freya-components` and `freya-animation` 0.5.0-rc.10, used
through `[patch.crates-io]` in `crates/ui`, `apps/settings`,
`apps/taxonomy` and `apps/files`. Every app lists all four, so the whole
Freya stack resolves one `freya-core`; a manifest that patches only some of them fails to build.
The changes are panic resilience (`freya-core`, `freya-winit`) and scroll feel for a desktop:

- `freya-core/src/guard.rs` (new), `runner.rs`, `element.rs`, `helpers.rs`, `event_handler.rs`,
  `current_context.rs`, `reactive_context.rs`, `scope.rs`, `lib.rs` and `lifecycle/{base,effect,
  task}.rs`: **a panic in app code never unwinds into the event loop.** Upstream lets a panic in a
  component, an event handler, an effect or a task unwind through the runner into winit, which
  aborts the process; Freya's "Hook functions must follow these rules" is one such `panic!` in
  `use_hook`. Here every place that runs app code catches the panic
  (`catch_unwind(AssertUnwindSafe(..))`) and reports a `guard::Failure` (kind, component type name
  and path, message, panic location, backtrace when `RUST_BACKTRACE` asks):
  - A component's `render` (the `comp` closure of every `Component`, `from_fn*` and the root
    scope) goes through `guard::render`. On a panic the scope's hook values, contexts and tasks
    are dropped and its hook counters stay at zero (the runner skips `current_run += 1` for a
    render that panicked, so the scope can start over), an empty `rect` stands in for the output,
    and the failure goes to the nearest `RenderSink` context above the scope: the error boundary
    in `bingux-ui` (`resilience/boundary.rs`). A boundary remounts its content under a new key
    (a new scope), so Retry never reuses the half-made state. With no sink above, a red label
    names the failure.
  - `EventHandler::call`, every `Effect` callback (the effect keeps listening and reruns on the
    next change), `use_drop` callbacks, scope teardown, and the task polls in `Runner` and
    `GlobalTasks` go through `guard::run`. The failure goes to the `NoticeSink` context (a toast);
    a task that panicked is dropped, as a future cannot be polled again.
  - `CurrentContext::run`, `run_in_scope` and `ReactiveContext::run` restore the previous
    context and pop the reactive stack with a drop guard, so a caught panic leaves no stale
    current context behind.
  - `use_hook` names the component, the hook's index and type and the caller (`#[track_caller]`
    on every `use_*`) in the hook-order message instead of the generic rules text. `Runner`
    gains `mark_all_dirty()`; `ScopeStorage` gains `name`, the component type name set on each
    render, for failure paths.
  - `guard::install_panic_hook` records what a panic said for the catch site and reports the
    panics nothing catches (worker threads); `set_observer` is the process-wide failure log;
    `set_unrecoverable_handler` is called for out-of-memory and lost-GPU wording and when the
    framework fails five times in three seconds, and `bingux-ui` relaunches the app there.
- `freya-winit/src/renderer.rs`: `user_event` and `window_event` run inside `guard::framework`, so
  a panic in the runner, a plugin or a frame is reported and the window is laid out again (a
  failed runner poll calls `Runner::mark_all_dirty`) instead of unwinding into winit. A
  graphics driver that cannot be rebuilt goes through `guard::unrecoverable`: the app gets its
  chance to relaunch, and the window closes if it is still here.

- `freya-winit/src/bingux_scroll.rs` and `renderer.rs`: the wheel step is 64 px
  per notch (upstream 53) and touchpad movement is 1x (upstream 2x).
  `BINGUX_WHEEL_STEP` and `BINGUX_TOUCHPAD_FACTOR` override them.
- `freya-components/src/scrollviews/use_scroll_controller.rs`: wheel and
  keyboard scrolls use an adaptive spring after Firefox's smooth scrolling: a
  motion starts quickly, glides softly while wheel events keep coming and
  settles fast once they stop (upstream: one fixed 60 ms spring).
  `BINGUX_SCROLL_SPEED` scales it (1 default, 2 twice as quick).
- `freya-components/src/scrollviews/scrollview.rs`: a wheel scroll can be
  interrupted. Turning the wheel the other way drops the pending travel and
  starts from the content's current position. `BINGUX_SCROLL_DEBUG=1` prints
  every wheel event and animation frame to stderr.
- `freya-winit/src/bingux_scroll.rs` (now `pub`) and `renderer.rs`: a raw wheel
  tap. The `MouseWheel` arm hands winit's event (`LineDelta` or `PixelDelta`,
  the phase and the window scale, before Freya drops every phase but `Moved`)
  to `bingux_scroll::tap`. `BINGUX_SCROLL_LOG=path` appends one JSON object per
  event to `path`; `bingux_scroll::enable_tap()` queues them for
  `bingux_scroll::drain_tap()` (the `scroll_probe` example reads them). Off
  (one atomic load per event) unless one of the two is set.
- `freya-components/src/scrollviews/wheel_accel.rs` (new), `scrollview.rs`,
  `virtual_scrollview.rs`, `use_scroll_controller.rs`: the wheel feel.
  `BINGUX_WHEEL_FEEL` (`current`, `raw`, `accelerated`) or
  `wheel_accel::set_feel` picks how both scroll views turn a wheel event into
  movement. `current` (default) is the behaviour above, unchanged. `raw` is
  the event's distance at once, no spring. `accelerated` scales each detent by
  a gain of 1x to 4x from the detent rate over the last 150 ms (pure
  `wheel_accel::step`, unit-tested), on top of the same spring, and makes
  `VirtualScrollView` drop pending travel on reversal as `ScrollView` does.
  Not the default yet: the `scroll_probe` example compares the three.
  `ScrollController::motion()` reports the spring's velocity.
- `freya-components/src/scrollviews/shared.rs`, `scrollbar.rs`,
  `scrollview.rs` and `virtual_scrollview.rs`: an auto-hidden scrollbar stays
  for 3 s after the last scroll or pointer movement, then fades out over
  400 ms, and fades back in over 120 ms (upstream: it vanishes after 0.8 to
  1.6 s with no fade). The idle check sleeps until the 3 s are up and runs
  no timer while the bar is hidden.
- `freya-components/src/scrollviews/shared.rs`, `scrollbar.rs`, `scrollview.rs`
  and `virtual_scrollview.rs`: `ScrollView::scrollbar_inset_bottom(px)` keeps
  a vertical bar `px` clear of the bottom edge, so a page that touches a
  rounded window corner does not sit on it. The track, thumb and cursor maths
  subtract the inset (`end_inset`); the default is 0 (upstream).
  `ScrollView::scrollbar_inset_top(px)` does the same at the top
  (`start_inset`), so the bar starts below a header that floats over the
  content.
- `freya-components/src/scrollviews/scrollbar.rs`: the track is placed by an
  absolute `left(-20)` (`top(-20)` for a horizontal bar) instead of an
  `offset_x`/`offset_y` on the absolute box (upstream). A right to left window
  mirrors an absolute position with its holder, but an offset on a mirrored box
  was applied twice once the bar's subtree was measured again (a page that
  changes height under it), putting the bar 40 px outside the window
  (`controls::direction_tests::a_page_scrollbar_in_a_phone_window_...`).
- `freya-components/src/scrollviews/virtual_scrollview.rs`:
  `VirtualScrollView::initial_size(Size2D)` seeds the measured size of the
  visible area. A scroll view knows its size only after its first layout, so
  its first render builds one row and, as the content outgrows a viewport of
  nothing, a scrollbar; the Files view passes the size its tab's last view had
  and draws the settled rows, scrollbar and columns in the first frame.
- `freya-components/src/scrollviews/activity.rs` (new), `scrollview.rs` and
  `virtual_scrollview.rs`: a window-level scroll signal. `ScrollActivity` is
  provided once per window (`provide_scroll_activity`, called by `WindowHost`);
  every `ScrollView` and `VirtualScrollView` reads it with `scroll_activity()`
  and pulses it from a wheel notch that moved the content and from every frame
  in which the content has a new position (a glide, a scrollbar drag, keys, a
  drag of the content). `scrolling()` (subscribing) and `is_scrolling()` read
  it; it clears after `SCROLL_STILL` (200 ms) without a pulse. Bingux tooltips
  and the file view's detail card stay down while it is set. A view with no
  provider above it does nothing (upstream has no such signal).
- `freya-components/src/input.rs`: the text field reports its caret and
  selection to the app. `Input::probe(State<InputProbe>)` writes the focus,
  the field's window area, the caret rectangle and the selection ends on each
  change. `Input::request(State<Option<InputRequest>>)` takes `SelectWordAt`
  (select the word at a window point) and `DragSelectionEnd` (drag a selection
  handle). Both are inert unless the app sets them, so upstream behaviour is
  unchanged.
- `freya-components/src/input.rs`: a text field can stay focused while its own
  overlay has the keyboard. `Input::focus_within(bool)` (it replaces
  `keep_selection`) draws and reports the input as focused while its context
  menu is open: the caret stays on and steady, the selection and background
  are the focused ones, and the probe still reports a caret. The overlay's keys
  go to the overlay; the input only looks and reports as the focus owner.
  `Input::read_only(bool)` keeps the caret, selection, Select all and Copy and
  ignores typing, deleting, Cut and Paste. A pointer release of any button
  inside the input or its companions (the edit bar) now counts as inside, so a
  right click on a focused field no longer blurs it (upstream blurred it on any
  press that was not a touch tap). The `trailing` element is no longer wrapped
  in 8 px of right padding: the app sets its own inset from the field's edge.
- `freya-components/src/input.rs`: the `leading` element is no longer wrapped in
  8 px of left padding (the app sets its own inset, as for `trailing`), and
  `Input::role(AccessibilityRole)` replaces the role the mode gives (a search
  field is a `SearchInput`). Upstream gives every input its text role.
- `freya-components/src/input.rs`: `Input::keep_focus_when(Callback<(), bool>)`.
  The input's global pointer-up blurs a focused input for any release outside
  it, but the shield of a popup the input owns (its context menu) takes the
  press that dismisses the popup, so that release is not a press outside. The
  owner's callback is asked at each such release; `true` keeps the focus.
- `freya-core/src/accessibility/dirty_nodes.rs` and `tree.rs`: a pending
  absolute focus (`Node`) is no longer lost when a relative move (`Forward`,
  `Backward`) is requested before the next accessibility update. The last request
  still wins, but the node focus is kept as `anchor_focus` and applied first, so
  the relative move starts from it. A Tab off the last control of a modal scope
  lands on the scope's end sentinel, which asks for its start and then asks for
  a move on; both requests reach Freya within one update, and without the anchor
  the second replaced the first and Tab left the dialog for the page behind it.
- `freya-winit/src/modifiers.rs` and `renderer.rs`: `freya_winit::modifiers::live()`
  returns the keyboard modifiers held right now. The renderer stores each
  `ModifiersChanged` there. Pointer events carry no modifiers, so a press
  handler reads this at press time to tell Alt-click apart. Tracking Alt from
  key events goes stale when focus leaves the window.
- `freya-components/src/input.rs`: a touch press on the field (the focus press
  on the input and on its paragraph, and the pointer press) does not call
  `stop_propagation()`. Freya cancels the global pointer down and up for a
  touch once a press is stopped, so the text field's global handlers could
  not hear a tap, long press or drag. Mouse presses still stop as upstream.
- `freya-components/src/scrollviews/virtual_scrollview.rs`:
  `VirtualScrollView::pass_ctrl_wheel(bool)`. When true and Ctrl is held, the
  view neither scrolls on the wheel nor stops the event, so an ancestor's
  `on_wheel` can use Ctrl+wheel to zoom. Ctrl is tracked from the global key
  events like Shift; any other key press re-reads the modifiers it carries, so
  a Ctrl released while the window had no focus does not stay stuck. The
  default is false (upstream).

- `freya-components/src/image_viewer.rs`: `ImageSource::load` drops the encoded
  file once the image is a raster, instead of caching it with the decoded
  image for the asset's age (an hour). A folder of wallpapers held every full
  file in memory: about 500 MB for 31 pictures. At most two images decode at once
  (upstream: four), which halves the peak while a large folder opens.

- `freya-components/src/image_viewer.rs`: `ImageViewer::fade_in(ms)`: an image
  that arrives after the viewer was first drawn fades in from transparent, over
  whatever the caller drew behind it (the Appearance hero's stand-in), instead
  of popping in. An image already in the cache shows at once. Also,
  `ImageSource::bytes` returns in-memory bytes without a thread round trip.

- `freya-components/src/svg_viewer.rs`, `cache.rs`: `warm_svg(source, px,
  colour)` rasterises an SVG on a worker into the asset cache under the key an
  `SvgViewer` of that size and colour looks up, and `AssetCacher::reserve`
  claims the entry first so a viewer that mounts meanwhile waits for it instead
  of rasterising the same icon again. Bingux's `icons::warm` uses it to have
  the sidebar and header icons ready for the first frame.

- `freya-animation/src/hook.rs`: an animation tick adds at most 34 ms to the
  animation's clock, and the first tick adds nothing. Upstream adds the whole
  wall time since the animation started, which includes the frame that
  started it. A slide started by a page mounting (a slow frame) jumped to its
  end in one frame, so Back seemed to have no slide-out; a hitch now delays an
  animation by a frame or two instead of skipping it.

These crates are MIT licensed by the Freya authors. Tests, benches and examples are
not copied. Drop this directory and the patch entries when upstream exposes these
settings and catches panics itself.

## `torin` and right to left

`torin` 0.5.0-rc.13 is vendored too (patched in all four app manifests, like the Freya crates) so
the layout engine can mirror. `Node::rtl` reflects a node's stacked and absolute children about its
box after they are laid out left to right (margins are swapped with the child, global positions stay
physical, scroll offsets keep their sign), and the cached-layout fast path re-measures a child whose
mirrored place changed. `Size::inner_sized` is public. The unused optional `hotpath` dependency is
removed so `cargo test` runs in place (`tests/rtl.rs` is the mirroring suite).
The scroll fast path (a node whose only change is its `offset_x`/`offset_y`, `DirtyReason::InnerLayout`)
returns "not revalidated": the node keeps the place its parent gave it, which is already mirrored, and the
parent must not mirror it a second time (it did, so a scrolled strip of tabs in a right-to-left window ended
up a content width off the screen; `tests/rtl.rs`, `a_scrolled_node_in_a_mirrored_parent_...`). The
fast path is also skipped when the parent is measured again (it places its children again in its own left
to right frame, which a cached, already mirrored place is not in), and a node measured as the root of a
partial relayout is mirrored in its mirrored parent (`Torin::measure`), so every relayout matches a layout
from scratch (`a_subtree_measured_again_from_a_root_below_a_mirrored_parent_...`).

`freya-core` carries the other half: an inherited `TextDirection` (`.text_direction(..)`), the
`Node::rtl` flag synced from it by `Tree` (also for a replaced element, `set_scale_factor` and a
changed direction), paragraphs shaped and laid out in that direction (`elements/text_layout.rs`), the
default `TextAlign::Start`, the resolved alignment kept in `ParagraphHolderInner::align` for a caret
in an empty line, and borders, corner radii, shadows and linear gradients drawn reflected
(`render_rect` takes the direction). In left to right every one of these is the identity.

`freya-core/src/elements/text_layout.rs` (`fit`): a label or paragraph is "shrink-wrapped" (laid out at its own
width, so right to left it is aligned to the right of a box exactly as wide as the text) when its width is
`Inner` and it has no minimum width. Torin's default minimum width is `Size::Inner`, not zero pixels; `fit` took
only `Pixels(0)` for "no minimum", so every ordinary label counted as having one, was laid out at the whole room
of its row and drawn that far toward the end of a box as wide as the text. A tab title was cut ("Folde"), a file
name in a list or a sidebar row was painted outside its cell (nothing visible), a header title ran off the bar.
`controls::tab_tests` (`a_title_that_fits_...`, `a_tabs_title_is_drawn_whole_...`) draw it in both directions.

## `winit` and popup windows

`winit` 0.30.13 is vendored (patched in all four app manifests, like the Freya crates) so a window can be
an `xdg_popup`: a menu, dropdown list or popover the compositor places against its parent, which can
leave the parent's bounds. `design/popup-windows.md` has the evidence and the decisions. Changes:

- `platform/wayland.rs`, `platform_impl/linux/mod.rs`: `WindowAttributesExtWayland::with_popup(PopupAttributes)`
  (parent, anchor rect, anchor, gravity, offset, constraint adjustment, card size, shadow `margin`, grab).
- `platform_impl/linux/wayland/window/role.rs` (new): `Role`, the `xdg_toplevel` or `xdg_popup` behind a
  window. `window/popup.rs` (new): the positioner. `window/{mod,state}.rs`: popup creation (fails with `Err`
  when refused), geometry and input region = the card, `request_inner_size` = `xdg_popup.reposition`,
  `WindowAttributes::with_parent_window` sets `xdg_toplevel.set_parent`. `state.rs`: `PopupHandler`
  (`Moved` = the compositor's placement, `popup_done` = `CloseRequested`). `event_loop/mod.rs`: the deepest
  popup is destroyed first, a window with an open popup waits (the other order ends the connection).
- `Cargo.toml` has no examples, tests or dev-dependencies.

`freya-core/src/lifecycle/context.rs`: `snapshot_contexts` / `provide_context_snapshot` copy every context
visible from a scope into another window's runner (a popup's content reads its opener's tokens and state).

`freya-winit`: `popup.rs` (new: `supported`, `placed`, `PopupMarker`), `config.rs` (`with_popup`, `with_parent`),
`window.rs` and `renderer.rs` (parent handle, popup creation that cannot take the app down, a popup's keys go to
the window it grew from, focus swallowed between a window and its popups), `extensions.rs` (`try_launch_window`).
- `freya-core/src/tree.rs`: a removal by index checks that the child at that index is the removed node and
  otherwise removes it by id. A node removed after a sibling of the same parent was at a shifted index, so
  removing by position took the wrong sibling out and left the removed node listed as a child with no
  element; the tree walk of the test runner (`find_many`) then unwrapped `None` (taxonomy page
  `files/file-manager-and-chooser`, `freya-testing` `lib.rs:663`). A guard for this is
  `controls::tooltip::tests`' neighbours in `bingux-taxonomy` (`tests.rs`, every page opens).
