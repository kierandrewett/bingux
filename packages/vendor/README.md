# Vendored Freya crates

Copies of `freya-core`, `freya-winit`, `freya-components` and `freya-animation` 0.5.0-rc.10, used
through `[patch.crates-io]` in `packages/bingux-ui`, `packages/bingux-settings`,
`packages/bingux-taxonomy` and `packages/bingux-files-app`. Every app lists all four, so the whole
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
