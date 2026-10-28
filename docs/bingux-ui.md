# bingux-ui

`packages/bingux-ui` is the design system for Bingux's native applications.
It is a Rust library on [Freya](https://freyaui.dev) with the shell's visual
system: the same surfaces, radii, rows, switches and sliders as the Control
Centre. Bingux Settings is built on it.

## Use it in an application

Add the crate and build the window root with `provide_tokens`, which reads
the GNOME accent and colour scheme, follows changes made elsewhere, and
styles Freya's own widgets to match:

```toml
[dependencies]
freya = { version = "0.5.0-rc.10", features = ["borderless", "titlebar"] }
bingux-ui = { path = "../bingux-ui" }
```

```rust
use bingux_ui::prelude::*;
use freya::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Page { General, About }

impl NavItem for Page {
    fn title(self) -> &'static str { match self { Page::General => "General", Page::About => "About" } }
    fn icon(self) -> Icon { match self { Page::General => Icon::PreferencesSystem, Page::About => Icon::About } }
}

#[derive(PartialEq)]
struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let mut page = use_state(|| Page::General);
        let content = Group::new("Options".to_string())
            .row(switch_row("Enabled", true, |_| {}))
            .row(row("Version").value("1.0"));
        rect()
            .expanded()
            .tw(&tokens, &[w_full(), h_full()])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(NavigationSplitView::new(
                "My App",
                vec![Page::General, Page::About],
                page(),
                move |chosen: Page| page.set(chosen),
                content,
            )))
    }
}
```

The split view does not own the selected page. `selected` is the page the sidebar
shows, and `on_select` is called with the page a sidebar row chooses, so the caller
keeps that state. On a narrow window, `.list(shown, on_show_list)` decides whether the
list of pages or the page is the screen. `on_show_list` is called with `true` when the
list is asked for (Back from a page, Ctrl+F) and with `false` when a page is chosen.
Without `.list`, the split view keeps that state itself.

`OverlayHost` provides the handle returned by `use_overlays()`, which opens
dialogs and shows toasts from any component below it. It mounts `WindowHost`
inside itself, so popups need no extra wrapper at the root.

### Alert dialogs

`overlays.show_dialog(dialog(title, body)...)` asks a question and waits for
one of its responses. It is `AdwAlertDialog`: a card over a scrim, one modal
focus scope.

```rust
overlays.show_dialog(
    dialog("Forget \"HomeNet\"?", "The saved password is removed.")
        .cancel("Cancel")                                   // id "cancel"
        .response("Forget", ResponseKind::Destructive, forget)  // id "forget"
        .default_action(CANCEL_ID)                          // what Enter runs
        .cancel_action(CANCEL_ID),                          // what Escape runs
);
```

`response(label, kind, f)` names a response after its label (`"Keep both"` is
`keep_both`); `action(id, label, kind, f)` names it explicitly. `icon(DialogIcon)`
puts a warning, error or information glyph above the title, for a question
that loses data or an error that stopped something. `content(element)` puts a
field or a checkbox between the message and the buttons.

**Layout.** One padding, `DIALOG_INSET` (12 px at every density), on all four
sides of the card. The buttons sit inside it, so a button's radius is the
card's less the inset (`Radius::Overlay.nested(inset)`, 8 px, concentric with
the card's corners). The icon, title, message and a field sit one inset
further in, so the text starts 24 px from the card's edge and the buttons 12.
Down the card the gaps grow with what they separate: icon to title and title
to message `S2` (8 px), message to buttons twice the inset (24 px: the text
block's own bottom padding plus the card's gap). The card is as wide as its
text and buttons ask for, between `Dim::DialogMin` (280 px) and `Dim::Dialog`
(400 px) and never wider than 94 % of the window; a dialog with `content` takes
the maximum. A long title and message wrap inside it. All text is centred, in
every dialog. The buttons are one row of equal widths, `S2` apart, in the
order the responses were added (Cancel first, the affirmative response last).
At compact density they are a full-width column, each `Dim::Button` (48 px,
above the 44 px touch minimum) tall, in the **reverse** order: the
affirmative response on top and Cancel at the bottom, nearest the thumb, as
on iOS and in libadwaita's narrow alerts. Tab follows what is on screen. The
default action is not moved: it is marked, not placed.

**Keyboard.** Every dialog has a default action, which Enter runs, and a
cancel action, which Escape, Back and a press on the scrim run. Both are one
of its responses, run through the same path as a click (the handler, then the
dialog closes, once: a second Enter while it fades out does nothing). Neither
is "close without a result".

| Key | What it does |
| --- | --- |
| Enter | Presses the button that has the focus; with no button focused, runs the default action. In a single-line field it runs the default action; in a multi-line field it adds a line |
| Space | Presses the focused button |
| Tab, Shift+Tab | Move the focus across the buttons inside the dialog, in the order on screen, and wrap; the page behind is inert |
| Escape, Back, a press on the scrim | Run the cancel action (a dialog with none just closes) |

- `default_action(id)` names the default response. Without it the default is
  the suggested response, else the cancel response, else the first response
  that is not destructive. A dialog that loses data therefore defaults to its
  safe answer: Cancel. Name a destructive response only when Enter should
  confirm it.
- `cancel_action(id)` names the cancel response. Without it, the response
  added by `cancel(label)` (id `CANCEL_ID`).
- The dialog opens with the keyboard focus on the default button. A
  destructive default is marked but **never focused**: the focus stays on the
  dialog's scope, so a stray Enter that opened the dialog cannot confirm it, and
  Enter still runs the default that the dialog asked for.
- **The default is shown at all times**, not only after Tab. The default
  button carries a key cap with the Return glyph (`↵`) after its label, in the
  ink the button already uses (the on-colour of a coral or accent button, the
  text colour of a plain one), so it reads on every fill with no new colour.
  It is a label, not a second ring: the keyboard focus ring is the only ring.
  Tab moves the ring from button to button and the key cap stays where Enter
  would go; on the default button itself the ring and the key cap are both
  shown, and they never overlap. A phone draws the focus ring alone (no
  keyboard to tell about). Candidates tried by looking at renders
  (`DIALOG_PNG_DIR`, see `shell/dialog.rs` tests): a 2 px ring in the button's
  colour offset by 2 px, a 1 px version of it, a double ring, an inner
  keyline. Every ring variant put two rings of one shape on two buttons after
  Tab, and which one had the focus had to be read from a colour; the key cap
  did not.

An alert is not a place for a form. A dialog with more than a field or two is
a popup (`use_window().open(popup(..))`); an alert whose body is the point (a
preview, properties) is a `framed_dialog`, which has no responses or keys of
its own.

`controls/default_mark.rs` is the key cap; `ActionButton::default_action()`
turns it on for a button of your own (a dialog does it for its default
response), and `ActionButton::radius(r)` and `fill_width()` are what a dialog
uses to nest its buttons and stack them.

### Nothing blocks the UI thread

A frame is 16 ms, and Freya runs every component, event handler and effect on
one thread. A process, a file read, a sleep, a D-Bus call or a lock held across
work on that thread is a dropped frame, and during a page slide it is a glitch
you can see. The rule for an application: **nothing on the UI thread waits.**

- Run commands with `runtime::run` / `run_with_stdin`, and read files with
  `runtime::read_to_string`, `read_file`, `read_dir` and `metadata`, on a
  worker: `runtime::background(state, work)`, `background_then(work, done)`,
  `use_cached_persistent`, `optimistic_then`, or `task::spawn_blocking` /
  `task::spawn_named(name, |token| ..)` inside a `spawn` task. The pool in
  `task.rs` is bounded; a job's future cancels it when dropped, and a named
  job gets a `CancelToken` to check between steps.
- Those helpers report a call made on the UI thread: `[ui-blocking] command
  `nmcli` on the UI thread` on stderr, once per call site, with a backtrace.
  The window marks its thread when it first renders, so start-up work before
  the window (fonts, the font family) is allowed to wait.
- Ask nothing in `render`. A page draws from what it has: cached values or a
  placeholder model behind `veiled` (`loading.rs` in Settings), so the first
  frame is the final layout. Things a placeholder needs from the machine are
  read once on a worker at start-up (`loading::warm_hints`), and `render` reads
  the answer.
- A page that slides in does not start or take work mid-slide. The navigation
  view tells the pool with `task::set_sliding(true)` while a slide runs. A job
  submitted from the UI thread (every `runtime::background`, `spawn_blocking`
  and `use_resource` load) is queued and starts when the slide has settled, so
  its process spawn and parsing do not compete with the slide for the CPU; the
  result of a job that was already running is held and delivered with the rest
  when it turns false. A slide that never reports its end is over after 1.5 s.
  The first frame of a subpage therefore comes from its cache or placeholder;
  its refresh starts after the slide.
- A hook (`use_state`, `use_focus`, `focus_stop`, `use_tokens` ...) is a call
  on the component's own list of hooks, and a page that returns early while it
  loads, or a helper function that only some renders reach, changes the number
  of hooks between the placeholder and the data: Freya panics ("hook #N ... has
  no match in the previous render") and the page shows its error card the
  moment the data lands. Put any element that contains a hook (a focus stop, a
  card, a swatch) in a `Component` of its own, not in a plain function called
  from a page's `render`.
- Animations are paced by frames: a tick adds at most 34 ms to an animation's
  clock (the vendored `freya-animation`), so a slow frame delays a slide by a
  frame instead of skipping it.
- Icons are looked up, never rasterised, in `render`. `icons::icon` (and
  `icon_scaled`, `app_icon` and the file views' glyphs) draw an `SvgViewer`
  that finds its raster in the asset cache, keyed by the icon, its size in
  device pixels and its colour. The first draw of a combination rasterises it
  on a worker, and until it lands the viewer is an empty box of the same size,
  so nothing moves. The colour is set as both `fill` and `color`, so the key
  does not change when the viewer learns its inherited text colour after the
  first draw (which used to rasterise every icon twice). The icons the first
  frame shows are rasterised at start-up: `NavigationSplitView` calls
  `icons::warm(icon, size, colour)` for the sidebar rows, the search field,
  the window controls and Back and Forward, and an app can warm its own the
  same way from a hook.
- A poll writes only what changed. A timer that reads a source again uses
  `runtime::refresh(state, work)`, or `State::set_if_modified` in a
  `background_then` callback, so a tick that finds nothing new redraws
  nothing and an idle window stays idle. The Date & Time clock writes only
  when the minute (the second, when the face shows seconds) changed.
  `runtime::background` is for first loads and one-shot work.
- The frame that mounts a page costs in proportion to what the page has, so
  the page is mounted a piece a frame (`bingux_ui::stage`). `veiled(..)` (the
  Settings page wrapper) and `stage::staged(page_column()..)` mount the page
  head and the first group with the page and one more child of the column
  each frame, top to bottom; a group of more than 24 rows mounts 24 and
  adds 12 a frame (`use_staged_rows`), and what is below it waits for it to
  be whole. Nothing is mounted below something that is not mounted, so
  nothing moves while the page fills in, and a page keeps its height from the
  last time it was whole as a minimum height. What a page's host puts under
  the page's column (the Related group under a Settings page) would be
  pushed down by every child that mounts above it, so the host calls
  `stage::provide_page()` and wraps it in `stage::after_page(..)`, which
  mounts it once the page is whole. A page mounts whole when the
  location names a row (`#fragment`) and while a search runs;
  `BINGUX_NO_STAGE=1` mounts every page whole, to compare.
- A control that mounts at rest has no hook that works at mount. A focus ring
  is not made until keyboard focus first reaches its control (`RingFace` in
  `motion.rs`: the wrapper keeps the face's size and a latch, and the ring
  component, its two animations and its nodes exist only after the latch);
  the anchor highlight of a row exists only on the row a location points at;
  a row tells its group whether it is lit and whether it matched in one hook
  (`use_row_slot`); a switch, checkbox, radio and slider thumb move with
  `use_tween_with`, which starts nothing at mount, not with
  `use_animation_transition`, which runs an animation from the value to
  itself on every mount. A row measures its title only once it and its
  control have been sized.
- Text is laid out once. The window's text cache keeps the paragraphs of the
  nodes that went away (2048 of them, `RETAINED_PARAGRAPHS` in the vendored
  `freya-core`), so a page opened again finds its labels shaped, and
  `warm::FontWarm` lays out one word in each text style the apps use, a style
  a frame, after the window is up, so the first page with tabular figures or
  a new weight does not pay for fontconfig's family search (40 ms on the
  frame that mounted it).
- `scripts/ui-lint.py` forbids `Command::new`, `std::fs` reads and writes and
  `thread::sleep` outside `BLOCKING_ALLOWED` (each entry has its reason), and
  any of them, or a blocking `system::` call, directly in a `render` body or
  in the initial value of a `use_state(|| ..)` / `use_memo(|| ..)`, which also
  run on the UI thread.

To find what still blocks:

```sh
BINGUX_UI_WATCHDOG=1 bingux-settings          # 12 ms; =20 for another threshold
BINGUX_UI_WATCHDOG_LOG=/tmp/wd.log ...        # also append the lines to a file
```

A monitor thread watches the UI thread through `/proc`; when it has been busy
for the threshold within one step of a frame (the stretch ends at every plugin
event, and the time the thread waits in the run queue for a CPU is left out,
so a loaded machine does not read as a blocked thread) it takes a backtrace and
logs `[watchdog <ms>] UI thread blocked 48
ms in std::thread::sleep (from bingux_settings::...)` with the stack
(`slow frame` when the stack is drawing, not waiting). `BINGUX_SLOW_SOURCES_MS=500`
makes every `runtime::run` wait 500 ms before it starts, as a slow `nmcli`
would: if a page still moves smoothly, its commands are on workers.
`BINGUX_PERF_LOG=FILE` writes one line per frame: unix time, frame ms, the
UI thread's CPU ms since the last frame, and the same without the presenting
step (on a software renderer, rasterisation, which says nothing about the
app; `frame-time.py` gates on that last figure).

`BINGUX_PERF_FOLDED=FILE BINGUX_PERF_SLOW_FRAMES=12` keeps only the stacks of
the frames busy for more than 12 ms (each under a `frame <ms> ms` root), the
profile of what the gate fails on. `BINGUX_SETTINGS_SNAPSHOT_TIMING=10
bingux-settings --snapshot DIR page,..` (a `snapshot` build) opens each page
ten times from the Power page and prints the UI-thread milliseconds of the
mount update: no window, no slide, no GPU.

`scripts/frame-time.py` drives a headless session through page switches, every
Network subpage's push and pop, scrolling, sliders and typing, and prints
interval and UI-thread busy-time percentiles per scenario. `--app files` runs it
on the Files app (`--binary` its path) in a folder of 20,000 files
(`FILES_HOME`, default `/var/tmp/FileTips-home`, made by
`scripts/files-fixture.py`) and scrolls a list and a grid at three zoom levels with the
wheel and the scrollbar while the pointer has armed the detail card. With
`--slow-sources 500` it slows every command first. It exits 1 when a scenario
has busy p99 over 20 ms, a busy frame or blocked stretch of 50 ms or more, no
frames at all, or (with `--gate-wall`) an interval p99 over a limit. A software
renderer inflates wall intervals; the busy time does not depend on it. Build
`bingux-settings` at opt-level 2 for it (the command is in the script's
docstring): the dev profile leaves the app's own page-building code at 0.

### Panics are never fatal

Upstream Freya lets a panic in a component, an event handler, an effect or a
task unwind through its runner into winit, which aborts the process. One
conditional hook (`Hook functions must follow these rules`) closed the whole
window. In a Bingux app that never happens: the vendored `freya-core` and
`freya-winit` catch the panic at the place that ran the code (see
`packages/vendor/README.md`), `bingux_ui::resilience` decides what the user
sees, and the rest of the window keeps running. **A panic is never a reason for
the process to end**, and nothing in an app may make it one: no
`panic = "abort"` (`scripts/ui-lint.py` rejects it), no `process::abort`, no
`catch_unwind` of your own around UI code.

| What panicked | What happens |
|---|---|
| A component's `render`, anywhere under a boundary (a conditional hook, a hook in a loop, `unwrap` on `None`, a missing context) | The nearest `ErrorBoundary` replaces its content with an **error card**: the message, the component path (`Settings › Switches`), **Retry** and **Copy details**. The failed component's hook values, contexts and tasks are dropped at once. Retry mounts the content again under a new key, a new scope, so it starts with fresh hook state. Everything outside the boundary keeps its state and stays interactive. |
| An event handler, a `use_side_effect`, a spawned task, a `use_drop` callback | The work is abandoned and the window shows a toast, "Something went wrong in X: message", with **Copy details**. The content stays. An effect keeps listening and runs again on the next change; a task that panicked is dropped (a future cannot be polled again). |
| A job on the worker pool (`spawn_blocking`, `spawn_named`, `spawn_detached`) | The pool survives. The failure is recorded and the window shows a toast. A `spawn_blocking` future stays pending, as before; use `try_spawn_blocking` to react. |
| The renderer, the runner or a plugin | Reported; the window is laid out again, and a failed runner poll renders every scope from its state. Five such failures in three seconds count as unrecoverable. |
| Out of memory, a graphics driver that cannot be rebuilt | The app starts a fresh copy of itself (`exec`, same process id) that opens the location the window showed and says why in a toast. Once: a copy started this way does not relaunch for a minute, so a failure that returns at once ends the process instead of looping. |

**Where boundaries are.** By construction, none of them written in an app:
the window's root (`Application::run`, and each inspector window), so a failure
nothing else caught is a full-window card; every page (`RouteTable::render`
builds the page inside a boundary keyed by the page, so the page function runs in
its own scope and a page that fails does not touch the sidebar or the other
pages); every popup body (`popup`) and every dialog body. An app adds its own
where a part may fail alone, for example one card of a dashboard, one row of a
list:

```rust
use bingux_ui::prelude::*;

// `content` is built by the caller; panics in the components below it are caught.
error_boundary("Network graph", NetworkGraph { .. })
// The closure runs inside the boundary's own scope: hooks it calls belong to
// the content, and a panic while building is caught too.
error_boundary_with("Usage", move || usage_card(&data).into_element())
    .look(BoundaryLook::Row)          // Block (default) | Row | Window
    .key(device_id)                   // a new boundary, cleared, when the key changes
```

The card is drawn with the standard components (`StatusPage` for the whole
window, a danger-tinted card for a block, one `Row` for a list). A failure while
the card itself is drawn (the window's root failed before it provided the tokens)
falls back to a plain card that reads nothing from the window.

**What is recorded.** Every failure goes to stderr with the component path, the
panic location and a backtrace when `RUST_BACKTRACE=1` asks, and into a ring of
the last 64 (`resilience::recent_errors()`, `report_text()`, `error_count()`),
readable from any thread, for Spotlight providers and bug reports. The inspector
shows them: the Elements tree marks an error card with a red `failed` chip and
the message, and the Performance tab has an Errors card with Copy report and
Clear. A failure repeated in a loop is logged once for two seconds and counted.

**What still must not be done.** A render must still call hooks in the same
order: the boundary makes the mistake recoverable, not correct. Do not hold a
`std::sync::Mutex` guard across code that may panic (a poisoned lock stays
poisoned; `bingux-ui` reads its own with `unwrap_or_else(PoisonError::into_inner)`).
A `Drop` that panics while another panic unwinds aborts, as in any Rust program.

**What is not caught, and why.** An allocation failure aborts instead of
panicking: the `SIGABRT` handler `Application::run` installs relaunches the
app, but the window's state is lost. A panic that unwinds out of a `Drop` while
another panic is already unwinding is a Rust abort. A stack overflow, a memory
fault in native code (Skia, the driver) and an `exit`/`abort` call in a
dependency end the process; none is a panic. A UI thread that loops or
deadlocks is not a failure the runtime sees; the watchdog (above) reports it.
Panics in `Callback::call` and `NoArgCallback::call` (callbacks that return a
value) are not caught at the call, only by the handler or render that made it.
A panic during layout or painting (a custom `ElementExt`) is caught by the
renderer's guard, which skips that frame and requests another.

Tests: `resilience/tests.rs` provokes a conditional hook, a hook in a loop, a hook
in an event handler, a panic in render, in an effect, in a handler, on a worker
thread and in popup content, and checks that the window renders, the card shows,
Retry remounts and the siblings keep their state
(`cargo test --lib --features testing resilience`). The taxonomy's
Feedback page has an Error boundary specimen that fails on purpose behind a
button.

### Application loop

`Application` is the process loop, like `GtkApplication`. It reads the URI
named on the command line, keeps one copy of the app running, and opens the
window:

```rust
fn main() {
    Application::new("my-app") // the app id, also the window's Wayland app id
        .routes(routes()) // the navigation tree: see Navigation below
        .title("My App")
        .borderless() // Gnoblin draws the frame
        .about(bingux_ui::app_info!()) // who it is: the About window's facts
        .usage("Usage: my-app [URI]") // printed by --help
        .run(|| App);
}
```

- `size(w, h)` and `min_size(w, h)` set the window size. They default to
  `theme::WINDOW_SIZE` and `theme::WINDOW_MIN_SIZE`.
- A second launch does not open a second window. It sends its URI to the
  running copy over `$XDG_RUNTIME_DIR/<app-id>.sock` and exits with status 0.
  The running copy navigates to it and raises its window. A socket left by a
  crash is replaced.
- **The inspector comes with it.** `Application::run` is the only way to start
  a bingux-ui window: it builds the one launch configuration (interface font,
  the inspector plugin, the borderless plugin) in a private function, so every
  app, example and tool has the inspector and its Ctrl+Alt+Shift+I shortcut
  with no code of its own, in the main window and in any other window the
  process opens. Do not call `freya::launch` or build a `LaunchConfig`: the
  inspector would be missing, and `scripts/ui-lint.py` fails on both anywhere
  outside `bingux-ui/src/app.rs`. (Dialogs, popups and sheets are layers of
  the window, not windows, so they are covered too.) An example that wants a
  window does the same as an app: `Application::new("id").title("…").run(|| Root)`.

Where the window is, and how it got there, is the [Navigation](#navigation)
system's: a location is a URI, and `use_location()` reads it.

### About window

Every application has an About window, made by `Application::run`. It is a
card over the window (the same card on a phone) with the application icon,
name, version, a one-line description, the website and "Report an issue" as
links (opened with `xdg-open`), Licence, Credits and Legal pages, the app id and
build, and **Copy details**, which puts a plain-text block for a bug report on
the clipboard: name and version, app id, commit and build date, licence, the
Rust, Freya and bingux-ui versions, OS, kernel and session, display scale, the
graphics driver, and the window's theme and density. A fact that is not known
is left out of the block.

**Configure it in three lines.** Put the package's metadata in `Cargo.toml`,
call `about(app_info!())` in `main`, and add the app's own items to its main
menu:

```toml
[package] # description, homepage, repository, license and authors feed the window
description = "Browse and organise files and folders"
homepage = "https://github.com/kierandrewett/bingux"
repository = "https://github.com/kierandrewett/bingux"
```

```rust
Application::new("my-app")
    .about(bingux_ui::app_info!(name = "My App", licence_text = include_str!("../../../COPYING")))
    .run(|| App);

// the app's main menu ends with About; an app with no menu uses a header menu
items.extend(bingux_ui::about::app_menu_items(overlays));
split.header_trailing(bingux_ui::prelude::app_menu(Vec::new()));
```

`app_info!()` reads `CARGO_PKG_NAME`, `VERSION`, `DESCRIPTION`, `HOMEPAGE`,
`REPOSITORY`, `LICENSE` and `AUTHORS` at compile time, the build's commit from
`BINGUX_BUILD_COMMIT` and its date from `BINGUX_BUILD_DATE` when the build sets
them (`cargo:rustc-env=` in a `build.rs`, or the environment of `cargo build`).
The name and id come from the package name (`bingux-files-app` is "Files" with
id `bingux-files`), the website from `homepage`, else `repository`, and the
issue tracker from a GitHub repository (`<repository>/issues`). Each
`key = value` after it calls the `AppInfo` method of that name (`name`, `id`,
`icon`, `description`, `website`, `issues`, `licence`, `licence_text`,
`authors`, `commit`, `built`, `third_party`) and wins. Only web and `mailto:`
links open; a field left empty leaves its row out.

- **Licence text** is the page behind the Licence row. Pass `licence_text =
  include_str!(..)` (Bingux's apps embed the repository's `COPYING`).
- **Legal** lists the libraries linked in. It shows the `third_party` text when
  the app gives one, else the `<id>.LICENSE.dependencies` that
  `scripts/cargo-licenses.py` writes and the RPM and Arch packages install under
  `/usr/share/licenses/<package>/` (`$BINGUX_LICENSES_DIR` overrides the
  directory, for a build tree). Nothing is read until the page is opened.
- **It cannot be forgotten.** `Application::run` registers the window whatever
  `main` says, so a bare `Application::new("id")` still has one, named after its
  id and title with no version, and the shortcut and `about::open` work.
  `scripts/ui-lint.py` fails an app crate whose `src/main.rs` starts an
  `Application` without `.about(` and `app_info!`.
- **The shortcut is Ctrl+Shift+?** in the main window of every application
  (`about::is_shortcut`). It does nothing while a dialog is open: that dialog
  holds a question. `about::open(overlays)` opens it from code, and the
  headless runner's script command `key ctrl+shift+?` reaches it the way a
  person does.
- **The main menu ends with "About <name>"** from `about::app_menu_items(overlays)`
  (a separator and the item, with the shortcut shown). An app whose menu is a
  list it builds appends them; an app with no menu puts `app_menu(items)` in
  its header (`NavigationSplitView::header_trailing`). Settings, Files and
  Taxonomy do. Settings' **System > About** page is about the computer; the
  window the menu opens is about Settings.
- The page behind it is inert while it is open, Escape and a press on the scrim
  close it, Alt+Left goes back from a page to the main one, and the focus returns to
  where it was. The card has one size on every page.

## Navigation

Every app gets navigation by construction. A location is a URI; the window's
back stack is a list of URIs; a route table, declared as data, maps URI
patterns to pages. Back, the header title, the history menu, the window title,
the swipe-back preview, search jumps and the inspector are all derived from
that one list, so none of them can disagree. The contributor design, with the
options that were compared, is `design/navigation.md`; this is how to use it.

### Declare the routes

```rust
fn routes() -> RouteTable {
    RouteTable::new("notes") // the scheme: notes://inbox/7
        .index("Notes", |_| notes_list()) // notes://, the list on a phone
        .root("inbox").title("Inbox").icon(Icon::Mail).page(|_| inbox())
        .root("archive").title("Archive").icon(Icon::Folder).page(|_| archive())
        .route("inbox/:id:int").title_with(note_title).page(|p| note(p.int("id")))
        .route("inbox/:id:int/attachments").title("Attachments").page(|p| attachments(p.int("id")))
}
// main: Application::new("notes").routes(routes()).run(|| NavigationSplitView::new("Notes"));
```

| Declaration | Means |
| ----------- | ----- |
| `.index(title, page)` | The app's own location, `scheme://`. On a compact window it is the list of roots; on a wide one the sidebar is that list and it is hidden from history |
| `.root(pattern)` | A sidebar destination. Listed in declaration order; `.group(name)` sections the sidebar; `.unlisted()` keeps it out |
| `.route(pattern)` | A subpage, pushed over its parent |
| `.tab(pattern)`, `.sheet(pattern)`, `.dialog(pattern)` | A sibling view of a page, and the modal kinds |
| `.title("…")`, `.title_param("ssid")`, `.title_with(\|params\| …)` | A static title, a path segment as the title, or one worked out from the params. The default is the last path segment |
| `.icon(Icon::…)` | The route's icon, shown in the sidebar and its history rows |
| `.query(key, default)`, `.query_pushes(key, default)` | Declares a query key. A value equal to its default is dropped from the canonical URI. A change replaces the top entry, or with `query_pushes` adds a history entry. Neither slides |
| `.virtual_parent(\|params\| …)` | Where Up goes when that is not the path's parent |
| `.ephemeral()` | Replaced rather than stacked when something else opens from it, so Back skips it |
| `.transition(Transition::…)` | How a move to this route is drawn, instead of the depth rule |
| `.page(\|params\| element)` | The page. `params.str("name")`, `params.int("id")`, `params.query("tab")` and `params.fragment()` read what the URI bound |

Patterns are literal segments, `:name` (a string), `:name:int` (an integer) and one
trailing `*rest`. A literal beats a parameter. Two routes with the same pattern
panic when the table is built.

### The URI

```
bingux-settings://network/wifi/network/Home%20Net?tab=saved#password
\_______________/ \___________________________/ \________/ \_______/
     scheme              path segments            query     fragment
```

- **Path segments are the hierarchy.** Opening a subpage adds a segment, and
  Up drops the last one.
- **The query is one-off state that does not change the hierarchy**: a filter, a
  tab, a selected id. A change of the query alone never slides.
- **The fragment is an anchor in the page.** `use_anchor(id)` returns the anchor
  of a row; the page scrolls to it and flashes it. A search result's jump uses it.
- Segments and values are percent-encoded when written and decoded when
  parsed, so a segment can hold spaces and slashes. `Location::parse(text)`
  returns `None` for text that is not a URI; `Location::parse_in(scheme, text)`
  also takes a bare path. A location is canonical when built (scheme lower case,
  `.` and `..` resolved, query sorted), so equal places compare equal. The route
  table also drops declared default query values.
- A URI no route owns shows a built-in not-found page that offers Up and the
  app's root. Nothing panics, and text that is not a URI at all is ignored.

### Move

Components read the window's location with `use_location()`, which redraws
them when it changes. `use_router()` returns the `Router`, which is `Copy`; any
code on the UI thread can use `bingux_ui::nav::router()`, or `nav::navigate(uri)`
to open a URI. From another thread (a socket, a D-Bus call) use
`bingux_ui::app::open_uri(uri)`, which queues it as a second launch would.

| Call | Does |
| ---- | ---- |
| `push(&location)`, `push_titled(&location, title)`, `open(Target)` | Open a subpage. A row that already knows the name passes it with `push_titled` |
| `back()` | *The* Back: the top menu, dialog or sheet first, else the page below. Escape, Alt+Left, the mouse Back button, the Back button and the swipe all call it |
| `pop()`, `pop_to(entry_id)`, `up()` | Leave one page, leave to a chosen entry as one move, or go to the route's parent |
| `replace(&location)`, `reset(&location)` | Swap the current entry, or drop the whole stack |
| `switch_root(&location)` | A sidebar choice: the stack becomes the index and that root, with a cross-fade |
| `navigate(uri)`, `navigate_to(&location)` | A deep link, a search action, a second launch. The stack is built from the table's ancestors (`network/wifi/network/Home` can go Back to Wi-Fi and Network), the way Android builds a back stack for a deep link. In a linear app it only pushes |
| `forward()`, `forward_to(entry_id)` | Only in an app whose table is `Mode::Linear` (the file views) |

A page never writes its own Back button or `on_back`. A subpage's header shows
Back whenever a page exists below it (`Router::can_go_back()`); a menu or dialog
never makes Back appear or disappear. A page that learns a better name than the
route's (a device name) calls `use_page_title(name)`, and the header, history
menu and window title all follow. `use_retained_state(key, init)` keeps UI state
(a selected tab, an expanded group) across Back, and the scroll position is kept
for you; both are dropped when the entry leaves the stack.

A page with unsaved edits calls `use_pop_guard(|| PopGuard::…)`: `Allow`,
`Block`, or `Confirm(Confirmation { title, body, discard })`, which asks and
leaves only if the person discards. Every way of leaving asks: Back, a history
choice, Up, a sidebar switch and a deep link.

### Motion

The transition is classified from the two locations, not chosen by the caller:
the same path never slides; a deeper path pushes; a shallower one pops; a
sibling at the same depth pushes; another root cross-fades; a linear table
never animates. A history choice that crosses several entries is one slide.

**Predictive back.** On a compact window, and for a finger on any window, a
drag from the left edge (the first 20 px) is a back gesture. The current screen
scales to 90 % and rounds its corners while it follows the finger, over the
previous screen rendered from its retained state, which grows from 95 % to full
size. Releasing past 35 % of the width, or with a rightward fling of 700 px/s
or more, completes the pop; a leftward fling of 500 px/s or more, or dragging
back past the start, cancels it with a spring. A touch that moves vertically
first belongs to the page's scroll. The gesture does not start when there is no
page below, and a swipe while a menu or dialog is open dismisses that instead of
leaving the page. The same pose plays for Back on a compact window; a desktop
window slides the page out over 320 ms. Under reduced motion it is a cross-fade
with no spring.

### The history menu

`BackButton` and `ForwardButton` open a menu of the places they would reach,
nearest first, on a **right click** or a **450 ms hold** (a finger or the
primary button) of the button. Choosing a row is one `pop_to` or `forward_to`.
The hover tooltip closes when the menu opens. The rows are the route titles (and
the route's icon when it has one). An app with its own button builds the same
menu with `IconButton::context_menu(history_menu(model.back_menu(), on_pick))`;
the file chooser and Files do.

### Guidelines

- Put a page at a path segment, not in state. If Back should return to it, it
  is a location; a filter that Back should not return to is a query.
- A location is the whole truth about where the person is. Do not keep a second
  `selected_page` state next to it.
- Titles come from the route. Never pass a title through a closure to the header.
- Name a route's parameters for what they are; a segment is shown in the history
  menu when no `title` says otherwise.
- Test with the model: `NavModel::new(Arc::new(table), &location)` and its
  operations are pure, so route resolution, deep links, `up`, the stack and the
  history menu need no window.

### File views

The file chooser, the Files window's tabs and the Files window's router all use
`bingux_ui::files::folder_table()`: a linear table where `file:///home/me/Documents`
is a folder, `recent:///` the recent files and `bingux-files://` an alias of
`file:`. A tab's folder, Back and Forward are a `NavModel` on it, so the toolbar's
Back and Forward buttons have the history menu above, and a tab shown again is
where it was left. `files::location_of(&Where)` and `files::place_of(&Location)`
convert between a place and its location. A location is text, so a folder whose
name is not valid UTF-8 is recorded with a lossy name: going Back to it opens a
folder that is not there, and the view says so.

### The inspector

The inspector's Properties pane ends with a **Navigation** section for the
window: its location, route, title, mode, the route's params, the stack below
(nearest first, with each entry's id), what Forward holds and the open menus and
dialogs (top first). It reads `Router::snapshot()` each time the pane renders.


## Components

| Family           | Components                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Controls         | `switch`, `checkbox` (tri-state), `slider` (`step`, `show_value`, `fill`, `gradient(stops)`: the track is a colour ramp instead of the accent fill; the night light's colour temperature uses it, and a hue or brightness ramp is the same call), `segmented` (`fill` for equal segments; only the selected segment is raised, a hovered one brightens its text), `dropdown` (`width`; a list of applications is `app_dropdown(Vec<AppChoice>, ..)`, which icons it), `stepper`, `usage_meter` (a figure, its total and a thin bar), `qr_code` (`ecc`, `label`, `size`: a pure-Rust QR encoder, versions 1 to 15, drawn black on a white tile with the quiet zone in every theme; `QrMatrix::encode` gives the modules; too-long data draws an empty tile), `action_strip` of `strip_action` (two to four icon-over-label actions in one rounded bar), `text_field` (`password`, `invalid`, `clearable`), `search_field`, `progress_bar`, `spinner` |
| Navigation       | `segmented(options, selected, on_select).tabs()` (the tab bar: each tab a real button with the segmented look, only the selected one raised, Left, Right, Home and End move the selection and focus; `scroll()` for a narrow container; an app's view tabs and the inspector's tabs use it too), `breadcrumb(crumbs, on_select)` (crumbs separated by chevrons, each a Tab stop, the last in normal ink; `code` for a code-face trail; a file manager path uses it too) |
| Buttons          | `action_button` (`suggested`, `destructive`, `flat`, `loading`, `icon`), `primary_button`, `row_button`, `icon_button` (`tooltip`, `danger`, `busy` (a spinner in place of the icon, the size unchanged), `radius`: `Control` like every button by default), `split_button(icon)` or `SplitButton::new(icon)` (a primary action and a menu in one control: `on_press`, `tooltip` or `label` (the primary segment's name), `menu(items)` or `on_menu(handler)` for the chevron, `menu_label` (the chevron's name), `enabled`; see Split buttons) |
| Rows             | `row` (`subtitle`, `value` (selectable, one line), `badge`, `leading`, `leading_tone` (draws the leading icon in the accent, warning or danger colour, for a row that reports a state), `leading_space` (a blank icon column so text lines up), `leading_app`, `navigates`, `selected`, `destructive`, `context_menu`, `on_press` (makes the row activatable)), `radio_row(title, selected, on_select)` (a radio on the leading edge; the whole row selects; `subtitle`, `value`, `leading_app` as for `row`), `radio_rows(title, rows)` (a group of radio rows: Up and Down move the selection, and only the selected row is in the Tab order), `switch_row` (`nested` for sub-rows shown while on), `slider_row` (a `level_row` for a setting), `level_row` (title on the left and a slider filling the rest of the line; `control` such as a mute button sits between them, `footer` under the track, `mark` and `mark_label` for a tick such as 100%, `danger_from`, `value_text`, `max`, `leading`, `context_menu`), `choice_row`, `text_row`, `entry_row`, `expander_row`, `property_row` |
| Icons            | `icon` (bundled symbolic glyphs), `app_icon` (full-colour application icons from the icon theme)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Groups and text  | `Group` (`description`, `content` for a chart or other free content), `section_label`, `group_head(title, trailing)` (a group's title and toolbar with no card, for a page that lists several cards under one name), `caption`, `centered_caption`, `action_row`, `heading`, `status` (loading, empty, error, info blocks)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Path text        | `path_label(path)` (one line that shortens a long path by what matters least, so the full way stays readable: `/home/kieran/…/design/final-report-2026.pdf`, `/h/k/…/design/…`, `final-rep…2026.pdf`; `style`, `ink`, `name` for a file name, `middle` for other text, `selectable`, `copyable`; see Path text below). `elide_path`, `elide_name`, `elide_middle` (module `elide`) are the same cuts as pure functions of a width and a `Measure`; `distinguish` labels paths that share a name |
| Layout           | `hstack(Space)`, `vstack(Space)`, `spacer`, `divider`, `card`, `clamp(Dim, child)`, `page_column`, `tile_rows(tokens, tiles, height)` (two tiles a row on a phone, four on a desktop)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Shell            | `NavigationSplitView` with `NavItem` and `subpage`, `HeaderBar`, `WindowControls`, `status_page`, `banner`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Page header      | `PageHead` (`subtitle`, `action`, `switch`): the title block that starts each page's content, with a status line under the title and one action or master switch at the right                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Search           | `NavigationSplitView::search(query, results)`: while the sidebar search has text, `results` replaces the page. Ctrl+F focuses it and Escape clears it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Search highlight | `search_highlight::provide(query)` in the results pane, then `search_highlight::label(style, tokens, text)` or `paragraph(...)` for text below it: each matched word gets an accent tint. Outside a search, or with no match, the text draws exactly as `TextStyle` would. While a search has text, a `Row` whose text does not match renders nothing, and a `Group` with no visible row hides with its title. `Group::unfiltered()` keeps every row for a group whose rows matched by other text, such as an index entry                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Overlays         | `dialog` with `ResponseKind::{Default, Suggested, Destructive}`, `framed_dialog(title, content, width, height)` (a dialog that is only its content, for a preview or a properties sheet), `Overlays::toast`, `Overlays::toast_with_action`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Popups           | `popup(title, content)` with `Popup::size`, `Popup::tabs` and `Popup::address`, opened by `window.open(popup)` from `use_window()`. A popup is a modal panel of its own, centred in the window: a Cancel button on the left, the title in the middle, the primary action on the right. It is 600 px tall unless `Popup::size` says otherwise, and it is clamped to the window. The body declares the primary action with `use_popup_actions(PopupActions { primary: Some(PopupAction { .. }) })`, and the caller closes the popup with `window.close()` once its work succeeds. Build the body with `popup_column()` and put each block in a `Group`, so the cards stand out from the panel. Cancel and Escape close it; Enter runs the primary action. The panel grows to at most 80% of the window height, and the body scrolls past that                                                                                                                                    |
| Forms            | `form_row(label, control)` for a label beside a control; `form_field(title, value)` for a validated text row (see Forms below); `live_state(rows)` for a read-only "Right now" group; `pending_bar(dirty, valid, on_apply, on_revert)` for the apply bar of a form with a draft; `segmented(options, selected, on_select).fill()` for a popup tab rail of equal segments (it stays a rail on a phone) |
| Trees            | `tree_view(roots, expanded, selected)` takes `TreeNode`s (`TreeNode::new(id, label)` with `detail`, `icon`, `badge`, `chips`, `paints(label, detail)` and `children`; a `Vec` or an `Rc<[TreeNode]>`) and the sets of open branches and the selected id as state. Rows are `MenuItem` height, or `CodeRow` height in the code face with `.code()`. Up and Down move the selection, Right opens a branch or enters it, Left closes it or goes to the parent, Enter and Space open or close a branch and activate it. A click selects; clicking the selected row activates it; Alt-clicking a chevron opens or closes the whole subtree. A selection the caller sets opens the row's ancestors and scrolls it into view. `TreeView::on_hover`, `on_activate` and `on_context` (a right click selects the row and reports its window position) report to the caller; `child_counts()` shows the child count of a closed branch and `find(text, current)` marks a search text in the rows. The inspector's Elements tree is a `tree_view`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Splits           | `split_pane(first, second, ratio)` with the first pane's share of the length in `ratio` (kept between 0.15 and 0.85). Side by side by default; `.vertical()` stacks the panes. Drag the divider to resize; the divider is a hairline with an `S1` hit zone on each side                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Menus            | `menu_item` and `menu_button(label, items)`. `MenuItem::checked` (state is a tick; menus are text, and `MenuItem::icon` is only for a menu whose rows the icon names, never in the file views; a list of applications is the exception and always has app icons: `app_menu_items(Vec<AppChoice>, on_pick)`, `app_dropdown(Vec<AppChoice>, selected, f)`), `menu_button` opens its menu against its own button (below it, above when there is no room, edge aligned, kept inside the window), `Overlays::show_menu` at a point is for context menus only; `MenuItem::destructive` and `MenuItem::disabled`; `menu_separator()`. `Overlays::show_menu` opens at window coordinates and stays on screen: it opens upward or leftward when there is no room. Picking an item closes the menu, then runs it; Escape and an outside press close it. While a menu or dropdown list is open, the window behind it takes no presses, wheel turns or hover, and the wheel never scrolls the page behind a list                                                                                                                                                                                                                                                                                                                                                                                                                                                |

Every control supports keyboard use: Enter and Space activate. Arrows move
sliders and steppers. Page Up and Page Down move sliders and steppers by ten
steps, and Home and End go to the ends. Open dropdown menus take arrows, Page
Up, Page Down, Home and End, and typing searches their options (see Type-ahead
below). Enter, Space and the arrows open a closed dropdown. Escape closes
dropdowns, menus, popups and dialogs. Segmented controls and radio groups are
roving: only the selected item is in the tab order, and the arrows move the
selection and focus together (Up and Down also work in a vertical radio group,
wrapping at the ends). Text fields: Enter submits, and Escape reverts an edited
value and then leaves the field. A password field has a reveal button, and a right
click opens Cut, Copy, Paste and Select all. The text cursor shows over the
editable text only: the reveal and clear buttons, the search glyph and any
suffix show the normal pointer, as every other control does. On touch, a tap places the caret,
a second tap on the same spot, or a tap in an empty field, shows a compact edit
bar over the caret (Paste while the clipboard holds text, Select, Select all;
Cut, Copy and Paste once there is a selection),
and a double tap or a long press selects the word under the finger. Two caret
handles mark a selection and drag to resize it; the bar, centred over the
selection, follows it. The bar's chevron opens the full menu as a floating
panel against the same caret or selection, never a sheet, and the selection
stays highlighted while it is open. Typing, Escape, a tap elsewhere or a scroll
hides the bar. Select all keeps it. Apps can append items to the
menu with `text_field(..).edit_menu_extra(items)`. In a popup, the panel takes
focus on open unless a field is marked `autofocus()`, and Enter in a
single-line field runs the primary action. Each control has an accessibility
role and a focus ring.
Animations are short (80 to 200 ms) and stop when `BINGUX_REDUCED_MOTION=1` is set.

### Split buttons

`SplitButton::new(icon)` is a primary action beside a chevron that opens a menu: the view button of a file manager, whose icon switches the view and whose menu picks one. The two segments are flush against each other and the control's edge.

```rust
SplitButton::new(Icon::ViewList)
    .tooltip("Switch view")            // the primary segment's tooltip and name
    .menu_label("View options")        // the chevron's accessible name
    .on_press(move |_| toggle_view())  // the primary segment
    .menu(vec![                        // the chevron opens this, against the whole control
        menu_item("List", move |_| set_view(List)).checked(is_list),
        menu_item("Grid", move |_| set_view(Grid)).checked(!is_list),
    ])
```

| Part | Behaviour |
| ---- | --------- |
| At rest | The icon, a hairline and a chevron, with no fill. The control is `HEADER_BUTTON` (`Dim::Control`) tall. The segments are flush: no padding between the control and its segments, no gap between the segments. The primary segment is square (or its label plus `S3` on each side) and the chevron segment is `Dim::StepperHalf` wide. |
| Pointer over | The whole control becomes one `Radius::Control` box with the hover layer, and the hairline fades out. The segment under the pointer takes a second hover layer, or the press layer while down, and that fill reaches the control's edge. A segment rounds only its outer corners (`Radius::Control`); the corners where the two meet are square. The hairline is one pixel wide on the seam. |
| Focus ring | Each segment's ring follows its visible shape: rounded on the outer corners, square at the seam. It is drawn outside the control (the control does not clip), so it is never cut off. |
| Keyboard | Tab reaches the primary segment, then the chevron, each with its own focus ring. Enter and Space press the focused segment (on the chevron they open the menu). Down on either segment opens the menu with the first item highlighted; Escape closes it and focus returns to the segment that opened it. |
| Menu | The same menu and placement as `menu_button` and `dropdown`: below the control, its edge aligned with the control's, above when there is no room, and held inside the window. A press on the chevron while the menu is open closes it. A mouse opens it on press, a touch on lift. |
| `on_menu(handler)` | The chevron runs `handler` instead of opening a menu, for an app that draws its own popup. Without `menu` or `on_menu` the control is the primary segment alone. |
| `label(text)` | Shows text after the icon; the segment grows to fit and the text is its accessible name. Otherwise the segment is square and `tooltip` is the name. Set one of them: a segment with no name fails the inspector's accessibility check. |
| `enabled(false)` | Dims the control and ignores the pointer and keys; both segments stay in place. |

On touch every size token takes its compact value, so the control is `HEADER_BUTTON` (44 px) tall and the chevron segment `Dim::StepperHalf` (44 px) wide. In a fitted `Row` the control is `Dim::RowButton` tall with the group's inner radius.

### Rows with a control and a chevron

A row that holds a control (a switch, checkbox, dropdown or button) and also opens a subpage, such as a Wi-Fi, Ethernet, Mobile or VPN row with its switch, is split in two by `Row` itself; a page writes `row(..).trailing(switch(..)).navigates().on_press(..)` and nothing more. A control passed directly to `trailing` is recognised by its type. Wrap one in a container (a switch beside a spinner) and add `.trailing_control()`. Status icons and text (a lock, "Saved", a battery level) and a `value` are not controls, so those rows keep the plain chevron with no divider.

| Part | Behaviour |
| --- | --- |
| Body | Leading icon, text and the control, one gutter from the card edge. Pressing the text or the icon opens the page; the control only acts. The body takes the hover and pressed fill, not the chevron. |
| Divider | One `Paint::Separator` hairline between the control and the chevron, the row's height less `S3` above and below (about 60% of a one-line row), with `S3` of space on both sides. A button, dropdown or segmented control keeps `ROW_INSET` between it and the divider instead (see below). |
| Chevron zone | Its own target, the row's full height and flush with the card edge, at least `Dim::Control` wide (44 px on a phone). It has its own hover and pressed fill, clipped by the card so its outer corners follow the card radius, and its own focus ring (drawn inside, with the zone's own square painted corners) and Tab stop, named "Open <title>" to assistive technology. |

The taxonomy's Row entry shows it under "Control and chevron" and "Button, dropdown and chevron".

### Trailing buttons and switches: one inset

When the trailing content of a row is a button (`action_button(..).compact()`, `row_button`, `icon_button`), a `dropdown`, a `segmented` or a `split_button`, passed directly to `trailing`, the button fills the row: it is `Dim::RowButton` tall (the row height less `ROW_INSET` above and below: 40 px, 44 px at compact density), it sits `ROW_INSET` (`S2`, 8 px) from the row's trailing edge, and its radius, and its focus ring, is the group's `Radius::Card` less that inset, which is `Radius::Inner`. The title keeps the usual `GUTTER`. The row does this once: it provides the fit to its trailing control, and each of those controls reads it with `rows::use_row_fitted()`. A page writes nothing extra and a new button-like control calls `use_row_fitted()` once and takes `Dim::RowButton` and `Radius::Inner` when it is true. In a tall row (a subtitle) the button stays centred with at least `ROW_INSET` above and below. Beside a chevron the divider is `ROW_INSET` from the button.

A trailing `switch` sits `SWITCH_INSET` from the trailing edge (the row height less the switch height, halved: 18 px, 16 px at compact density), the distance it has above and below. Text, a `value`, a status icon, a checkbox and a `stepper` keep the `GUTTER`; a stepper is its own pill with an equal `S1` inset around its buttons. The taxonomy shows all of them under Button, "Compact, inside a row", and Row, "Trailing controls fill the row".

### Banners and toasts: one notice

A banner and a toast are the same component. `Banner` is the base: a pill with an optional leading icon, an optional title, a message that wraps, any number of quiet action buttons and an optional dismiss cross. `Toast` is a `Banner` that floats: `Toast::new(text)` builds a `Banner::new(text)` (neutral kind) and only adds what makes it float, so the pill, its colours and its buttons are drawn by one code path.

```rust
banner("Updates are ready to install.")
    .kind(BannerKind::Info)          // Neutral, Info, Success, Warning, Error
    .title("Update available")       // optional bold line above the message
    .action("Install", |_| {})       // repeat for more buttons
    .dismissible(move |_| shown.set(false))
    .visible(shown());               // opens and closes by height

overlays.toast("Settings saved");                       // unchanged
overlays.toast_with_action("Network forgotten", "Undo", |_| {});
overlays.show_toast(Toast::new("Could not reach the network.")
    .kind(BannerKind::Error).title("Offline").action("Retry", |_| {}));
```

**Shape.** The pill is the button height (`NOTICE_BUTTON`, the control height: 36 px, 44 px on a phone) plus `NOTICE_PAD` (`S1`) above and below. The action buttons and the dismiss `icon_button` are that height, centred, and the last one is `S1` from the top, bottom and trailing edge. Text sits `S4` from the leading edge (`S4` from the trailing edge too when there are no buttons). A button's radius is its container's radius less the padding between them: the container is `Radius::Pill`, so the buttons are pills. A banner that wraps to three lines or more is tall, and takes `Radius::Overlay` with `Radius::Card` buttons (20 px less the 4 px padding), so the corners stay concentric and the text clear of them. A toast keeps two lines and ends the rest with an ellipsis, in at most 60 % of the window width; an inline banner fills the page column and wraps all of its text.

**Several actions.** `.action(..)` can be called again. On a desktop the buttons share the row with the message; on a phone (compact density) a banner with more than one action puts them on a row of its own under the message, end-aligned, with the dismiss cross at the top corner, so the message is never squeezed to a word a line. That banner is tall, so it takes the tall radii above.

**Colour.** `BannerKind` decides every colour, from `tokens.notice(kind)` (`NoticePalette`): `fill`, `ink`, `edge`. For a tinted kind the fill is the kind's hue (the accent for Info, `success`, `warning`, `danger`) as an opaque low-chroma tint, so it reads over whatever a toast floats on. The text, the leading icon, the dismiss glyph and the label of an action button are that same hue moved in lightness: darker than the fill on a light scheme, lighter on a dark one, until they reach 7:1 (AAA) on the fill. The 1 px edge is the hue at 3:1 on the fill. An action button is **quiet**: no fill at rest, its label in that ink, the hover and pressed layers of every control over the fill, the focus ring, a nested-radius pill the same height and inset as the dismiss cross, so the notice reads as one calm surface. `Neutral` is the dark floating surface of a tooltip (`on_floating` ink). Paints: `Paint::NoticeFill/NoticeEdge(kind)`, ink `Ink::Notice(kind)`; `icon_button(..).ink(Ink::Notice(kind))` draws a glyph in it. Every kind, in both schemes and for every accent, is in the contrast matrix, including the quiet action label at rest, hover and press.

**Icons.** Info, Success, Warning and Error lead with `DialogInformation`, `Check`, `DialogWarning` and `DialogError`, so the kind is not told by colour alone; `.icon(Icon)` picks another, `.without_icon()` none. A neutral banner has none.

**Inline.** A banner opens and closes by height through `reveal` (`.visible(bool)`; no motion under reduced motion), and a closed one is unmounted. `.gap(space)` names the column's spacing so a closed banner takes none and the page does not jump.

**Floating.** The toast shell adds only the part that floats: a layer above the dialog and menus, the `Elevation::Raised` shadow, bottom-centre placement (`S6` above the bottom edge plus `Dim::SafeBottom` on a phone), a 140 ms slide-and-fade, a queue (toasts show one at a time, in the order they were queued) and a timer: four seconds, six with an action, set with `Toast::timeout`. The timer is a pure state machine, `shell::hide_clock::HideClock` (tested with a fake clock), held by three things: the pointer anywhere over the toast's own box (padding, gaps, text and buttons: the shell listens to `pointer_over` and `pointer_out`, which a button inside it does not steal), a finger down on it, and keyboard focus on one of its buttons. While held the clock does not run and the toast never closes; when the last hold lets go the **whole** delay starts again from zero (not from where it paused), and entering again pauses again. A toast that arrives while one is held waits behind it. On a touch screen a sideways drag of 72 px or more dismisses it; a shorter one springs back. Pressing an action also dismisses the toast.

### Tab strip

`tab_strip(tabs, selected)` is the tab strip of a window that holds several views of one kind: the folders of Files, the documents of an editor. It is Chromium's tab strip: the selected tab is the same surface as the content under it, with rounded top corners and a concave flare at each foot that joins the strip's bottom edge, and the other tabs are flat and recessed on a darker strip. The model, the sources it follows and the numbers are in `design/tabs.md`. The host owns the list and the selected id; the strip holds no state. Use `segmented(..).tabs()` instead for a few fixed pages that cannot be closed and have no content of their own to attach to (the inspector's tabs, the Properties dialog).

The strip sits in a `tab_frame(bar, content)`, which owns the two facts that make the tabs attached. The **surface**: the content is painted in `Paint::Bg` (or `.surface(paint)`) and the strip inside fills the selected tab with the same value, so tab and content are one colour and one shape, with no line between them; the bar behind the tabs is `Paint::TabStrip`, one step darker. The **seam**: the bar is `Dim::Header` tall and the content begins at its bottom edge, with no gap and no overlap; a hairline runs along the bar's bottom edge and the selected tab's outline starts and ends on it, so it is drawn everywhere but under the selected tab. The bar is either a `HeaderBar` filled with `Paint::Transparent` whose `.tab_slot(strip)` holds the strip from the start of the room between its controls (the window controls share the row, and the band above the tabs still drags the window), or any row that holds the strip.

```rust
tab_frame(
    HeaderBar::new("Files")
        .fill(Paint::Transparent)
        .trailing(WindowControls)
        .tab_slot(
            tab_strip(
                tabs.iter().map(|tab| TabItem::new(tab.id, tab.title.clone()).icon(Icon::Folder)).collect(),
                active,
            )
            .on_select(move |id| select(id))
            .on_close(move |id| close(id))
            .on_new(move |_| open_new())                 // the New tab button; Ctrl+T on a tab
            .on_reorder(move |(id, to)| reorder(id, to)) // drag a tab, or Move left / right in its menu
            .on_hover(move |(id, over)| hover(id, over)) // pointer over a tab, for drops
            .drop_target(highlighted),                   // the tab a drag would drop on
        ),
    page,
)
```

Every length is a token. A tab is `Dim::Control` tall (the height of a header button; 44 px at compact density) and stands on the bottom edge of the `Dim::Header` bar, so 12 px of the bar stay above the tabs. Its top corners are `Radius::Control`; its feet flare out by that radius less `FIELD_INSET` (`Radius::Control.nested(FIELD_INSET)`: 8 px at mouse density, 12 px at compact density where there is no inset), and the foot is also the gap between two tabs. The close button is `Dim::Box5` (the whole `Dim::InlineButton` at compact density) `FIELD_INSET` from the tab's end, with the tab's corner nested by that inset; the New tab and Tab list buttons are `Dim::InlineButton`, level with the tabs, with the same corner. The strip never overflows its container: it is as wide as its tabs (so the New tab button follows the last tab) and no wider than the room it is given, and every part is laid out from the measured width, so nothing can spill out of it.

| Part | Behaviour |
| ---- | --------- |
| Width | Tabs share one width: `Dim::Field` at most, shrinking together as tabs are added down to `Dim::TabMin` (96 px; 128 px on a phone, where the close slot is a whole touch target and a title still needs room beside it). Beyond that the strip scrolls sideways (wheel, drag with a finger, or Left and Right) instead of squeezing, the strip fills its container, and the tabs fade into the bar where they continue. The selected tab is scrolled into view by the shortest distance whenever the selection moves. |
| Tab | `[icon] [title] [close]` on one row. The title takes the room that is left and is cut with an ellipsis before the close slot. The close slot is always laid out, so a tab never changes width when the button appears. The button is the standard `icon_button`, drawn on the selected tab and on a hovered one (not on a touch screen's other tabs), named "Close <title>"; pressing it neither selects nor drags the tab. A lone tab has no close slot. The selected tab is the attached shape; an unselected one is flat, shows `Ink::Muted` text (4.5:1 on the strip, and under the hover and pressed fills), fills with `Paint::Hover` under the pointer and `Paint::Pressed` under a finger, and has a short separator after it that hides next to the selected tab and the tab the pointer is on. |
| End of the strip | The New tab button (`on_new`) follows the last tab, an `icon_button` the tab's height. It never scrolls. When tabs overflow, a Tab list button (`Icon::PanDown`) before it opens a menu of every tab, the selected one checked. |
| Motion | Adding a tab grows it in and the others narrow to make room; closing one shrinks it away and the others widen (`MOVE_MS`, instant with reduced motion). The widths are laid out from one clock, so the strip never exceeds its container during the motion. |
| Pointer | A press selects (a mouse as it goes down, a finger when it lifts). A middle click closes. With `on_reorder`, a tab dragged more than 6 px follows the pointer, drawn over the others, which slide aside on the springs of `crate::reorder`; letting go settles it into its place and calls `on_reorder(id, to)` once, after it has settled. Dragging past the strip's end scrolls it. Escape, or letting go more than a header's height above or below the strip, puts the tab back. The strip uses `crate::drag::PointerDrag` and `crate::reorder::Reorder`, as `sortable` does. A right click, or a finger held on a tab, opens its menu: Close tab, and Move left and Move right with `on_reorder`, which is how touch reorders. `on_hover` reports the pointer entering and leaving a tab, so a host can accept a drag on it (see Files below). |
| Keyboard | Only the selected tab is in the tab order. Left, Right, Home and End move the selection and the focus; Ctrl+Shift+Left and Right move the selected tab (`on_reorder`); Delete or Ctrl+W closes the selected tab and the focus follows to its neighbour; Ctrl+T calls `on_new`. The tab's focus ring is drawn inside the tab, with its top corners, so the scroll region cannot clip it. |
| Accessibility | The strip is a `TabList` named "Tabs"; each tab is a `Tab` named by its title with its selected state. The buttons are named "Close <title>", "Tab list" and "New tab". |

The taxonomy's Tabs entry shows each state in a frame at both densities: select, close and add with drag; many tabs; nine tabs; long titles; icons; a single tab; no New tab button; keyboard focus; a frame painted in a card's surface. `controls/tab_shape.rs` holds the geometry and path tests (the tab ends where the strip does, nested flare radius, the outline stays in its box and ends on the hairline), `controls/tabs.rs` the layout and contrast tests, and `controls/tab_tests.rs` renders strips at mouse and compact density: the selected tab's bottom edge is the content's top edge at every width, everything is inside the strip, tab widths are uniform, the seam and the flares are checked to the pixel in both schemes, and the keyboard, drag and motion behaviour.

### Sortable list

`sortable(rows, on_reorder)` is a list the person puts in order. The items are `row(..)`s (or any element of a row's height), drawn as one card with a hairline between them, exactly as a `Group` draws its rows: content at the gutter, 56 px (60 px on a phone) uniform rows, the card's radius, no gaps. Each row ends in a grip: a `Dim::RowButton` square, `ROW_INSET` from the row's top, bottom and end (the trailing-button rule above), holding the 16 px `Icon::DragHandle` in muted ink that turns to text ink on hover. The list never reorders itself: the caller owns the order.

```rust
sortable(
    names.iter().map(|name| row(name.clone()).into_element()).collect(),
    move |(from, to): (usize, usize)| {
        let item = order.write().remove(from);
        order.write().insert(to, item);
    },
)
.labels(names)          // grips are named "Reorder <label>"; moves are announced
.drop_indicator()       // long lists: a line instead of sliding rows
```

`on_reorder(from, to)` is called **once**, after the move has settled, with indexes into the list *before* the move: remove the item at `from`, insert it at `to`. It is never called with `from == to`, with an index out of range, or after Escape. The pure rules live in `reorder.rs` (`Stack::landing`, `Stack::offsets`, `key_target`, the `Reorder` state machine) and are unit-tested; the pointer capture is `drag.rs` (`PointerDrag`, `Frames`), shared with anything else that carries an item.

| Part | Behaviour |
| ---- | --------- |
| Pick up | Press the grip with the primary button or a finger. A finger resting half a second anywhere on a row picks it up too (the lift is the cue); one that moves first scrolls the page. |
| Held | The row lifts: it grows by 2 %, takes `lift_shadow` and drops to 94 % opacity, on its own layer above its neighbours. It follows the pointer along the list and nowhere else. The card stops clipping while a row is held, so the shadow is not cut. |
| Passing | The row it passes slides one row height (its own height, so rows of different heights work) out of the way on a critically damped spring (about 120 ms). Nothing outside the list moves: every row translates, none reflows. A row moves past a neighbour when its leading edge crosses the neighbour's middle. |
| Drop | Letting go settles the row into its place on the same spring while the shadow fades; the order is reported when everything is at rest. Escape settles everything back and reports nothing. |
| Edge | Held within 56 px of the top or bottom of the page, the page scrolls, faster nearer the edge, and the row stays under the pointer. |
| Keyboard | Up, Down, Home and End on the focused grip, Alt+Up and Alt+Down on a focused row, move the item with the same slide; focus then rests on the item's grip. |
| Long lists | `.drop_indicator()` leaves every row still and draws a 2 px accent line in the gap under the pointer. |
| Assistive technology | Each grip is a button named "Reorder <label>" with its place ("3 of 5") and the hint to press Up or Down. A polite live region announces "<label> moved to position 2 of 4". |
| Reduced motion | Every move is instant: no lift animation, no slide, the order reported on release. |

Rows may have their own controls (a `switch`, a delete button): the grip is a separate target to their right, and a drag never presses the row it started on. The taxonomy shows a four-row list, a list with switches and a twelve-row list with the drop line under "Sortable list".

### Path text

`path_label(path)` shows a path on one line and, when it does not fit, shortens it by what matters least, so the end of the path (the part you came for) and as much of the way there as fits stay readable. The text is never cut at a fixed character count: the label tries a fixed list of forms from the most to the least complete and draws the first that fits the pixel width, measured in the real font. The same width always gives the same text, and a wider width never shows less, so nothing flickers while a window is resized.

The forms, for `/home/kieran/Documents/Projects/bingux/design/final-report-2026.pdf`, in order:

1. The path whole.
2. Directories from the middle outwards become one `…`, keeping the first segment (`/` and the first directory, `~`, a drive, `smb://host`) and the last directories: `/home/kieran/…/design/final-report-2026.pdf`.
3. The directories that are left are shortened to their first letter, as fish's `prompt_pwd` does (`.config` is `.c`), longest first and from the left; the directory that holds the file stays whole. `/home/k/…/design/final-report-2026.pdf`, `/h/k/…/design/…`, `/h/…/final-report-2026.pdf`.
4. The name loses its middle and keeps its extension (`.tar.gz` whole) and the end of its stem: `/h/…/final-repor…2026.pdf`. When fewer than eight letters of the name would be left the directories go and the name has the room alone: `final-rep…2026.pdf`.
5. The extension gives way too (`fi…df`), and at the very least the text is `…`. It is never empty.

A cut is always between grapheme clusters, never inside an accent, an emoji sequence or a flag. Text with right-to-left letters is drawn as one left-to-right run, so the `/` separators keep their order. The reasons and the sources are in `design/path-elision.md`.

```rust
path_label("/home/kieran/Documents/Reports/final.pdf")
    .style(CAPTION)
    .copyable(true)      // right click: Copy path

path_label("final-report-2026-edition.pdf").name()   // keeps ".pdf"
```

The label fills the width of its parent and is one line tall, so a cut never moves anything around it. It measures the text with the font and size it draws in, redoes the cut when the width changes, and shows the whole text on its first frame before it has a width. While the text is cut, the full text is the hover tooltip and the accessible label. `.copyable(true)` adds a right click (or long press) item that copies the whole text, not the cut one. `.selectable(true)` lets the pointer select the shown text and Ctrl+C copy it; use it in a properties dialog, not in a list row, where a selection would take the press that opens the row.

To decide a cut in your own layout, call `elide_path(path, max_width, &measure)`, `elide_name` or `elide_middle` from `bingux_ui::elide`. `measure` is anything that implements `Measure` (the width of a string in px): a closure `|text| width_of(text)` does. `elide_path_traced` also says which of the forms above was used (`ElideStep`). A list of paths or tabs with repeated names gets its labels from `distinguish(&paths)`: a name alone while it is unique, else the name with the nearest parent folders that tell the paths apart (`index.ts — app/src`, `index.ts — lib/src`). The taxonomy page "Path label" has a width slider that shows each form live.

### Forms

`form_field(title, value: State<String>)` is a `Group` row for one text input. The title, with an optional hint line under it, sits at the left and the field at the right; on a phone the title and hint are above and the field fills the row. An error shows under the field in the danger colour, announced to assistive technology as an alert, and the field gets the danger outline. A field that can fail keeps a blank message line, so the row never changes height when a message comes and goes.

```rust
use bingux_ui::controls::form::validate;

Group::titled("Server")
    .row(form_field("Address", address).hint("Host name or IP address").validate(validate::hostname_or_ip))
    .row(form_field("Port", port).suffix("TCP").validate(validate::port))
    .row(form_field("Key", key).monospace().multiline(3).optional())
```

| Method                   | Effect                                                                                           |
| ------------------------ | ------------------------------------------------------------------------------------------------ |
| `hint(text)`             | One muted line under the title.                                                                  |
| `placeholder(text)`      | Greyed text in the empty field.                                                                  |
| `password()`             | Masks the text; the field's reveal button shows it.                                              |
| `monospace()`            | Monospace face for keys and addresses.                                                           |
| `wide()`                 | `Dim::FieldWide` (440 px) on a desktop, for a key or URL. Give every field of a group the same width. |
| `suffix(text)`           | A muted unit inside the field's right end, such as `bytes`.                                      |
| `trailing(element)`      | An element after the field on the same line (phone too), such as a remove `icon_button`.         |
| `optional()`             | The muted word "Optional" after the title.                                                       |
| `multiline(lines)`       | A field `lines` lines tall; Enter adds a line.                                                   |
| `error(Option<String>)`  | Shows a message now, touched or not, for example after a failed Apply. It wins over `validate`.  |
| `validate(fn)`           | Runs a check from `form::validate`, or your own `fn(&str) -> Result<(), String>`.                |
| `validate_with(closure)` | The same with a closure, such as `validate::integer_in(576..=9000)`.                             |
| `on_submit(f)`           | Enter in a single-line field.                                                                    |
| `autofocus()`            | Takes focus when it first appears; only one field per page.                                      |

`validate` never shows a message on an empty field or on one the person has not left: the message appears once the field is non-empty and has lost focus (or Enter was pressed in it), and from then on follows every keystroke, so it goes as soon as the text is right. The check is for the field only: the form runs the same function again to decide whether Apply is allowed. The validators are pure functions returning a plain-language message; `form::validate` has `ipv4`, `ipv4_cidr`, `ipv6`, `ipv6_cidr`, `ip`, `ip_list` (commas or spaces), `ipv4_netmask_or_prefix`, `mac`, `port`, `hostname_or_ip`, `url`, `wireguard_key`, `ssid` (bytes, not characters), `wpa_passphrase` and `integer_in(range)`. An empty string is an error for every validator, and an IPv6 zone such as `fe80::1%eth0` is rejected.

`pending_bar(dirty, valid, on_apply, on_revert)` returns `Some` only while `dirty`: a group holding "You have unsaved changes", Revert and Apply (disabled while `valid` is false). Put it at the top of a form with `.maybe_child(..)`. `live_state(vec![(label, value)])` is a "Right now" group of read-only rows with monospace, selectable values, to show the effect of what the form sets (address, gateway, DNS, link speed).

Outside a form, `text_field` takes the same looks: `.monospace()`, `.lines(n)` (the height of a `.multiline()` field), `.suffix(text)` and `.danger_outline(bool)` (the outline without the message line). `row(..).mono_value(text)` is `value` in the monospace face, selectable like it. `.leading_chip(label, remove_label, on_remove)` puts a removable token (such as the `in Downloads` scope of a search) inside the field after the leading glyph; it never changes the field's size. `.on_enter_modifiers(f)` reports Ctrl, Shift and Alt just before Enter submits, so a field can tell Enter from Ctrl+Enter.

Menus take the keyboard too. Enter, Space or Down on a menu button opens its
menu with the first item highlighted. Up and Down move the highlight over
enabled items and skip separators; Home and End go to the first and last item.
Left and Escape step back one level; Escape at the top level and Tab close the
menu and return focus to the control that opened it.

Every row of a menu answers the pointer: hover or keyboard focus highlights it.
A disabled row is not lit, but it still counts as a move onto a sibling, so the
pointer never crosses a dead zone. A panel with a tick (`MenuItem::checked`) or
an icon on any row keeps one leading column for all its rows, so labels line
up; radio groups are rows with a tick as well. An item made with
`submenu(label, items)` opens its submenu on hover after 150 ms (at once when
another submenu of the same menu is already open), on Right, Enter or Space, and
on a click or a tap, and it opens to the left when there is no room on the
right. Moving to another row closes it. A pointer moving from an open row toward
its submenu keeps the submenu open while it stays inside the safe triangle
between the pointer and the submenu, and a rest of 300 ms in the triangle ends
it. The pointer and key rules are `shell/menu_nav.rs`, a pure state machine
with an injected clock; `menu.rs` only draws it.

**Type-ahead.** Typing while a menu, dropdown list or crumb folder list is open
does not close it. The letters build a query, and **an open list's query never
times out**: a pause of any length, as a slow typist or someone using assistive
technology makes, leaves it as it was, and typing goes on from there. It ends
only when the person ends it:

| Key | Does |
| --- | --- |
| Backspace | Removes the last character. |
| Ctrl+Backspace | Removes the last word. |
| Delete, Ctrl+A | Clears the whole query. |
| Escape | Clears the query; a second Escape closes the list. |
| Enter, or a click on a row | Picks the row and closes the list, which drops the query. |
| Space | Types a space while a query is active (it picks as usual when there is none). |

A small pill under the list (above it when there is no room) stays up for as
long as the query is not empty. It shows the query and how many rows match, and
is announced to a screen reader as a polite status ("3 matches for fol"); the
keys above are not repeated on it. A row matches when its
label contains the query, ignoring case; a label that starts with it ranks first,
then one with a word that starts with it, then any other substring. The best
match takes the keyboard focus and the matched letters of every match are marked
on the accent tint, while rows that do not match are dimmed (nothing is removed,
so the list does not reflow). While a query is active, Down, Up, Tab, Shift+Tab,
Ctrl+N and Ctrl+P walk through the matches only, wrapping at the ends, and Enter
picks the focused match. Disabled rows and separators never match.

A **closed dropdown** (the button has focus, the list is shut) is the one place
with a reset. Typing there chooses the best match at once, as a native select box
does, and no pill shows the query, so a prefix left over from earlier would
silently change what the next letter chooses. Its query therefore ends after 2 s
without a key (`CLOSED_RESET_MS`; WAI-ARIA's listbox guidance leaves this delay
to the implementation, and 2 s is long enough to finish a word slowly). Backspace,
Ctrl+Backspace, Delete, Ctrl+A and Escape clear it earlier, and opening the list
starts it with an empty query. An open list never has a reset.

The rules are `shell/menu_query.rs`, which is pure and reads no clock, so a query
cannot age; `shell/type_ahead.rs` has the window side, including the shut-button
reset. A list that keeps its own highlight uses `use_list_query`,
`ListQuery::key(event, items, focused, open)`, `marked_label`, `dimmed` and
`Panel::pill`, and calls `ListQuery::clear` when it opens, closes or picks. Escape
is heard by the panel anywhere in the window, so a list's own key handler leaves
it alone and gives the panel `Panel::on_escape` (`ListQuery::escape` clears the
query, else the list closes).
`search_highlight::highlighted_label(style, tokens, text, ranges)` draws a
label with byte ranges marked, for any list of names.
**Menus that list applications show their icons.** A menu, a context menu, a
submenu, a popover, a dropdown or a compact sheet whose rows are applications
(Open with, Set as default, Choose application, a handler dropdown) draws each
application's full-colour icon at `IconSize::AppSmall` (20 px) in the leading
slot. It is not optional and not a style: the list API is the only way in.
Describe each application as an `AppChoice::new(key, label, icon)` (`icon` is
the desktop entry's `Icon` value; an empty one is allowed) and build the rows with
`app_menu_items(apps, on_pick)` (one row: `app_menu_item(&choice, f)`), or a
dropdown with `app_dropdown(apps, selected, on_select)`, which icons the list
and the collapsed value. The icon is looked up off the UI thread by the shared
`app_icon` loader; until it arrives the slot is an empty tile of the same
size, and an application whose icon the theme lacks shows the generic
application glyph, so a row is never empty and never moves. An application row
anywhere in a panel widens the panel's leading column to 20 px for every row,
ticks included, so labels stay aligned. The Menus page of the taxonomy shows
an Open with submenu, a dropdown of applications and the compact sheet.
`cargo test -p bingux-ui --lib app_icon` fails on a workspace source that reads
an application list and builds a menu or dropdown from it without `AppChoice`.
`MenuItem::shortcut("Ctrl+C")` shows a muted hint at the trailing edge; the app handles the keys. A menu panel with any hint is `Dim::MenuWide` (300 px) instead of `Dim::Menu` (220 px).

Popups share one motion, all with an ease-out curve and all scaled by
`tokens.motion(ms)`, so they are instant under `BINGUX_REDUCED_MOTION=1`.
Each layer stays on screen until its exit finishes.

- Popup: fades in and scales from 94% to 100% in 150 ms. The scrim fades with it. Closing reverses it.
- Dialog: fades in and scales from 96% to 100% in 120 ms. The scrim fades with it.
- Menu: fades in and scales from 96% to 100% in 90 ms, pivoting on its top-left corner (the anchor).
- Toast: slides up 8 px and fades in over 140 ms. Dismissing it slides it back down and fades it out; a swipe moves it with the finger. An inline banner opens and closes by height (`reveal`).
- Dropdown: on a phone a sheet (see Panel); on a desktop it fades and scales in over 90 ms, the same easing as the menu, anchored to its button.
- Subpage: a push slides the new page in from the right over 320 ms on an expo ease-out, and the page beneath stays still; a pop slides it back out to the right. The moving page is a card while it moves: its leading corners take `Radius::Window`, and a soft shadow (`sheet_edge_shadow`: 24 px offset, 32 px blur, black at 0.5 alpha at full amount) falls on the page beneath. Both scale to nothing over its last 15 %. The window's own buttons stay at the window's top right while it moves. A sidebar switch cross-fades over 120 ms. A move that starts mid-slide continues from where the slide is.
- Sheet (a popup on a narrow window, or a dropdown on a phone): slides up from the bottom on the same ease-out as every panel, and is as tall as its content up to a share of the window (70 % for a choice, 92 % for a popup). The grabber, the title strip and a finger dragging the list down from its top pull it. The sheet moves as one piece: `shell/panel.rs` puts the motion (`offset_y`) on a slot *around* the card, because `offset_y` shifts an element's children and not the element, so an offset on the card itself leaves its background behind. Above rest the pull is rubber-banded and stays inside the card's overhang below the window edge. A release dismisses when the pull, projected 120 ms ahead along the release velocity, passes `SHEET_DISMISS_DISTANCE` (96 px, or half a short sheet), so a fast flick dismisses and an upward flick rests; otherwise it springs back with the opening ease. The scrim follows the drag. The gesture is a pure state machine in `shell/sheet_gesture.rs` (drag, fling, snap, list hand-over) tested with a fake clock. A list in a sheet scrolls by wheel and by finger; a finger drag at the list's top hands over to the sheet and back when the finger returns. The list ends with `sheet_bottom()` (`SHEET_BOTTOM` + `Dim::SafeBottom`) as scroll content, so its last row scrolls clear of the window edge. Escape and a press on the scrim close it. `controls/dropdown_rows_tests.rs` reads the frame the renderer drew and fails if any option row of an open dropdown (short and 200-row lists, popover and sheet, both schemes, with icons, with a typed query, in every frame of the opening) shows no label: its text, opacity and ink contrast against the row's own fill, and the pixels inside the label's box, are checked for every row, scrolling the list end to end.

A popup with `PopupFlags::INSET_VIEWPORT` shrinks the window behind it into a rounded card on black: the window scales to `INSET_SCALE` (0.96) and its corners take the `Card` radius, both in step with the popup's open and close. On a sheet the inset also follows the pull: full at rest, it eases down in proportion to the drag and reaches zero at the dismiss distance, then springs back with the sheet on release. Desktop popups do not inset.

### Popups size to their content

A popup is as tall as its body and no taller. The panel measures the body (its padding and, on a sheet, its foot included) and the chrome above it (header, tabs, rule), and its height is `chrome + content`, capped at 80 % of the window (92 % for a sheet), so the body scrolls only when it is over the cap and nothing is left empty under the last row. It is centred. `popup(title, body).width(Dim::PopupNarrow)` sets the width (default `Dim::PopupWidth`, 520 px); there is no height to set. A body that lays itself out and has no height of its own (a file chooser, `PopupFlags::FILL`) uses `.size(w, h)` and keeps `Dim::PopupHeight` as its default.

A popup can still be resized from its edges and corners. The first drag turns the content height into an explicit one; the minimum is a header and one row for a content-sized popup, `POPUP_MIN_HEIGHT` for a sized one. The pure rule (`panel_height` in `window/popup.rs`) is unit-tested.

A note that belongs to a group goes under it: `Group::footer(text)` puts the caption at the card's edge, the group's 12 px gap below it. A caption added as a sibling of the group sits a whole popup-column gap away and reads as detached. `Group::description` is the same for a note above.

A form field row is as tall as any row: `FIELD_ROW_INSET` (10 px, 8 px on a phone) above and below a one-line field. The field's message line exists only while there is a message, so the row never has more room under the field than over it (it grows by a caption line when a message appears).

### Actions inside a text field

Every action in a text field (the password reveal, the clear button of a search field) is the standard `IconButton`, `Dim::InlineButton` square, which is the field height less twice `FIELD_INSET` (4 px; 0 at compact density, where the button is the whole 44 px touch target). It sits that inset from the field's top, bottom and trailing edge, its radius is the field's less the inset (`Radius::nested`), and it has the hover and pressed fill, focus ring, tooltip and name every icon button has. After a press the focus returns to the field (`IconButton::refocus`). A dense or flat field is shorter than a button, so it uses a `Dim::Box5` button. `RowAction` and the tab close button are the same component with `pointer_only()` (out of the Tab order).

`scripts/ui-lint.py` fails on a press handler of Freya's own (`|event: Event<PressEventData>|`) on a bare box outside the modules listed in `RAW_PRESS_ALLOWED`, each with its reason.

### Tooltips

`hover_tip(child, text)` wraps any element and shows `text` in a bubble once the pointer has rested on it for 500 ms. `icon_button(..).tooltip(text)` and `tooltip_icon(..)` are built on it. The bubble is the tooltip surface (`TOOLTIP` recipe, `Paint::Floating`, caption text in `Ink::OnFloating`) and takes no pointer. A touch never shows one. Pass the current text on every render; a label that depends on state (Show / Hide password, Play / Pause, Mute / Unmute) needs nothing else.

- **A new label does not move the bubble.** The bubble draws the text it was last measured with, at that size. The new text is laid out in a hidden copy first, and once its size is known the bubble swaps to the new text and the new place in one frame, so no frame draws text at the size of other text. The bubble stays on the same side of its control, centred on it, with the edge that touches the control in the same place: it grows away from the control.
- **Where it sits** is one pure decision, `shell/tooltip_place.rs`: `decide(anchor, size, window, previous_side) -> Placement { x, y, side }`. It opens below the control, else above, right, then left, centred on the control along its edge, `PANEL_GAP` away and `PANEL_EDGE` inside the window. It is never over the control. The side is remembered while the tip is up and kept while it has room: it leaves a side only when staying would put it outside the window. Near an edge the side therefore cannot flip back and forth, and a bubble does not move while the pointer rests. Only when no side has room (a window smaller than control and bubble together) does it take the roomiest side and run past the window edge, still off the control.
- **Long text wraps** inside `Dim::TooltipMax` (280 px) onto at most four lines, then is cut with an ellipsis. The size comes from the hidden copy, so the wrap never reflows after the bubble is drawn.
- **One clock per window.** While a tip is up, and for 300 ms after the last one went, the next control's tip opens at once, in full, instead of waiting 500 ms. The tip it replaces stays as it was until the new bubble has its size and is drawn, and goes in that same frame: moving along a row of controls is one bubble that changes place, with no frame between them that draws nothing and none that draws two. After 300 ms with no tip, the 500 ms rest applies again. `WindowHost` provides the shared state (`provide_tooltips`); a tip with no host keeps its own.
- **Never over a menu.** No tip opens while a menu or dialog is open, and one that is up goes when it opens (`Overlays::menu_open`).
- **Scrolling takes tips down.** Content that scrolls moves under a resting pointer, so no tip is up while anything scrolls. Every `ScrollView` and `VirtualScrollView` tells its window, through one window-level signal (`ScrollActivity`, vendored in `freya-components`, provided by `WindowHost` and read by the tooltip hub), when its content moves: from a wheel notch, and from every frame in which the content has a new position (a glide, a scrollbar drag, the keys, a drag of the content). The window is scrolling from then until the content has stood still for 200 ms (`SCROLL_STILL`). A tip that is up goes in the frame the signal rises, with no fade. A tip that was waiting for the pointer to rest, or whose pointer arrives meanwhile, opens no bubble while it is set; when it clears, a tip whose pointer is still on its control waits the usual 500 ms rest (no 300 ms grace after a scroll) and opens. Tips subscribe to the signal only while they are up or waiting, so an idle tip does not render for it. A host that does not provide the signal (a test that mounts a view bare) never scrolls, as before.

`controls/tooltip_tests.rs` steps the headless app frame by frame with the pointer on real icon buttons: a label that changes while hovered (the bubble is centred on its control, at the same distance, in every frame), neighbouring controls (never two bubbles, the second at once), the wait coming back, a long label wrapping and flipping once at the window edge, and a menu taking the tip down. The taxonomy's Tooltip specimens (Feedback) are the same cases to try by hand.
`controls/tooltip_tests.rs` also scrolls with a tip up: it goes within two frames, none opens for a pointer that arrives while the content moves, and it returns once the content is still.

### Panel

`shell/panel.rs` is the one floating-surface primitive: context menus, dropdown lists, popovers, sheets, popups and the text edit bar are all `Panel`s. A variant is configuration, not a new component. A panel owns its backdrop (a shield that takes presses, wheel and hover outside it), presence, placement (flip and clamp inside the window), dismissal (Escape, an outside press, an address change), focus trap and restore, and roving keys.

A panel knows its trigger: `PanelTrigger::Pointer`, `Control`, `Row`, `Text` or `Keyboard`. Placement is measured from that trigger.

Phone rule: a panel is a sheet only for choosing a value from a set (a dropdown such as Resolution or Refresh rate) or a long titled list. A sheet slides up with a fading backdrop, has a drag handle, a height cap (a token, about 60 to 70 percent), a header with the control's label, and a scroll affordance. It opens with the selected item visible. A floating panel is for actions (row long-press and context menus, overflow menus): anchored to its trigger, no dim, and an outside tap dismisses it. A text trigger uses the text edit bar. On a desktop everything floats, and popups are centred.

The shield is the only input shield; there is no second copy of the shield or grabber code.

### Sliders, the colour picker and every drag that makes a value

A control that turns a pointer drag into a value (the slider, and the colour picker's saturation square, hue strip and alpha strip) uses one primitive, `controls::capture` (crate-private): `use_capture(handler)` in the component, `capture.bind(rect)` on the element that starts the drag (`capture.handlers(rect)` plus `capture.sized(&event)` from its own `on_sized` when it already has one). Do not write an `on_pointer_down` and `on_capture_global_pointer_move` pair by hand; the primitive fixes what that pair gets wrong:

- **The press owns the drag.** Until the button comes up the moves go to the control wherever the pointer is: outside the small element, past a popup's edge, over a scrim. They are consumed (`prevent_default` on the capture move), so nothing under the pointer shows hover or runs a tooltip. The release is consumed too (`on_capture_global_pointer_up`, not `on_global_pointer_up`, which another capture-up that prevents default, such as a scrollbar thumb, cancels): a drag released over a scrim does not dismiss the popup, one released over a button does not press it.
- **It ends once.** The primary button's release ends it. A touch is followed by its own finger only, a cancelled touch ends it, and a mouse move while a finger drag is on ends it, because the finger's release was lost. A hover never drags.
- **One report a frame.** The press, the release and the first move of a burst report at once; later moves wait for the next frame (`RenderingTicker`, with a redraw requested so one always comes) and only the newest goes through. A control writes its state once a frame and the event handler does nothing else. The state machine (`Gesture`) is pure and unit-tested.
- **Positions are the element's.** A report (`CaptureEvent { phase, local, at }`) gives the pointer in the element's own logical pixels, from the layout as it is now (exact at the press, followed by `on_sized` after), so a scroll view that scrolls, a sheet that slides and a window at a fractional scale are all followed. A control clamps with `capture::fraction(pos, start, travel)`.

`crate::drag` (`PointerDrag`) carries an item such as a tab or a list row with the pointer; capture reads a position off it. The sheet grabber, the popup resize handles and the scrollbar thumbs keep their own gesture code: each has state beyond a position (velocity, edges, the scroll controller).

`colour_picker(value, on_change)` edits a colour held as hue, saturation, value and alpha (`controls::hsva::Hsva`), never as RGB converted back, because RGB cannot say which hue a grey was on: the hue thumb stays where it was when the saturation reaches 0, and the saturation thumb when the value reaches 0. A `value` that is the colour the picker just reported changes nothing; any other (the caller set it, a swatch or the hex field made it) is taken in as an edit of the colour on screen, so a grey keeps its hue and a black its saturation, and it is not taken in while a drag is on (the caller's echo is late). The hue strip changes the hue only; the alpha strip changes the alpha only. `on_change` is called at most once a frame during a drag. The square, hue strip and alpha strip are Tab stops with the slider role and the focus ring; arrows move one percent (one degree on the hue strip), Shift or Page Up and Page Down ten, Home and End go to the ends of a strip. The swatches are focus stops (a press or Enter chooses, keeping the alpha). The hex field applies a valid entry as it is typed (six digits keep the alpha, eight say their own) and is rewritten only when the colour changes to one the text does not already say; Enter tidies it. The picker is drawn from pure white and black, not theme colours, so the corner a pointer picks is the corner it shows. Freya's `LinearGradient::angle` is not CSS's: 0 runs top to bottom, 90 right to left, 180 bottom to top and 270 left to right.

## Files and the file chooser

`bingux_ui::files` is the SDK's answer to "let the person pick a file". An app
never shells out to `zenity` or `kdialog` and never builds its own picker:

```rust
use bingux_ui::files::{ChooserRequest, Filter, choose};

choose(
    use_window(),
    ChooserRequest::open()
        .title("Import VPN configuration")
        .filter(Filter::new("VPN configuration").glob("*.conf").glob("*.ovpn"))
        .filter(Filter::new("All files")),
    move |chosen| {
        // Runs once, on the UI thread. `None` means the person cancelled.
        if let Some(chosen) = chosen {
            import(&chosen.paths[0]);
        }
    },
);
```

`ChooserRequest::open()`, `select_folder()`, `save(name)` and
`save_files(names)` start a request; `title`, `accept_label`, `multiple`,
`filter`, `filter_spec`, `filters`, `presets`, `current_filter`,
`current_folder`, `current_file`, `choice` and `view(ViewMode::{Auto, List,
Grid})` refine it. The answer, `Chosen`, holds absolute `paths`, the active
`current_filter` and the `choices`. The dialog is a popup: a panel on a
desktop window, a full sheet on a phone (Cancel, the accept button,
`PopupFlags::FILL`).

What the dialog has:

| Part | Behaviour |
| ---- | --------- |
| Toolbar | Back, Forward, Parent folder (a folder with an arrow out of it), Refresh, the address bar, the file type dropdown, New folder, a list or thumbnails switch and the view menu (sort, hidden files). Every button follows the `Control` radius like any other button, and the view menu opens against its own button, not at the pointer. Refresh (also F5 and Ctrl+R) reads the folder again even when no change was seen, as a network or removable drive sends none; the selection, scroll position and filter stay, and the button shows a spinner of the same size while it reads. |
| Address bar (`AddressBar`) | The path as crumbs, and the search box (see [Search](#search-in-the-file-views)). The field takes the `Control` radius and a 4 px padding, so each crumb inside it takes the `Inner` radius (12 less 4). Each crumb has a chevron that lists that folder's subfolders. A trail that does not fit keeps its first and last crumb and hides the middle behind one `…` menu. Click the empty end, press Ctrl+L, or type `/` or `~` to edit the path as text. Typing shows completions (folders first); Tab completes, Up and Down choose, Enter opens, Escape closes. Errors show inline: not found, not a folder, no permission, not responding. `~`, relative paths and `file://` work. |
| Sidebar (`PlacesSidebar`) | Recent, Home and the user folders (with their own icons by path), Bookmarks, then Drives: Computer and every mount, each with a usage bar and "X free of Y". A drive that does not answer says "Not responding". There are no buttons on the rows: every row has a context menu (right click, long press, the Menu key or Shift+F10) of text items. A bookmark offers Open, Open in new tab, Rename… and Remove from bookmarks; a drive Open, Unmount or Eject, Add to bookmarks, Copy path and Properties. The folders have their own icons: a folder with a page for Documents, a down arrow for Downloads, and so on. A phone gets `PlacesMenu`, a sheet with the same rows. |
| File type filter | A dropdown in the toolbar on a desktop; on a phone a "File type" row at the bottom. The sidebar runs the dialog's full height: there is no footer under it, and the name field of Save, the request's choices and any problem sit in a strip at the bottom of the content pane. With no filters in the request it offers All files, Images, Audio, Video, Documents, Archives and Text. `presets(true)` adds those after the request's own. The list shows only matching files (folders always stay) and the empty state says when the filter hides everything. The status pill floats over the bottom-right corner of the list on an opaque floating surface and takes no row. It says `95 items`, `1 of 95 selected (19 kB)` when something is selected, `3 of 120 match` while a search is on, and what a running operation is doing; it does not report hidden files or what the filter hides. It follows the pointer over the whole window: when the pointer comes within a small margin it slides to the other bottom corner, and it returns when the pointer leaves the list. |
| New folder | The toolbar button, Ctrl+Shift+N and the empty-space menu make a folder and start renaming it in the list, with the name checked as you type. |
| View | `FileView`: columns, zoom, rubber band selection and context menus. Each folder remembers its icon size and sort. |

**The file view.** `FileView::new(listing, order, selection)` shows a folder as a list with columns or a grid of tiles, chosen by the zoom level. List columns: Name, Size, Type, Modified, Created, Accessed, Permissions, Owner (Location in Recent). Drag a column's left edge to resize it (double-click fits it), drag a heading to reorder, right-click the header to show or hide columns; widths, order and visibility are remembered in `$XDG_STATE_HOME/bingux/files/view.json`. A row is the sidebar's row (`LIST_ITEM`: the same radius, inset, fills and minimum height), held `LIST_EDGE` clear of the list's sides, and the header lines up with it. **Zoom** changes the icon and nothing else: the text, its size and the column widths never change. The level is an icon size in pixels from one ladder, `16, 20, 24, 32` (a list, and 16 px is the default in every folder) and `48, 64, 96, 128, 192` (a grid of tiles with the name under the icon in the same text size, each tile growing with its icon). Ctrl+wheel, Ctrl+plus and minus step one level, Ctrl+0 returns to 16 px in a list, a pinch picks the nearest level, and a badge shows the size and view for a moment. Choosing a list or a grid (toolbar, View as menu, Ctrl+1 and Ctrl+2) goes to the nearest level of that view. Nothing picks a larger level for you, neither the window size nor what the folder holds; the one limit is that a tile must fit twice across, so a narrow view stops at a smaller level. Images, and videos and documents that have a desktop thumbnail, show their picture, uncropped, at every grid level and at every list size (at the default 16 px a list row shows only a picture the desktop's thumbnail cache already has, and keeps the file-type icon in the same slot until then; from 24 px it decodes an image too); folders and everything else keep their icon. The level is remembered per folder in the same file (`icon_px`). Right-click gives a menu for a file, several files, empty space and the headers; Menu or Shift+F10 opens it from the keyboard. Rename in place with F2, rubber-band from blank space, drag files onto a folder (Ctrl copies), hover for a detail card. Rows are drawn only when in view, sizes and dates are asked of the loader only for those rows, and thumbnails load after a short rest in view and never on network drives. Keys: Space (Quick Look), Ctrl+A, Ctrl+Space, F2, Delete, Shift+Delete, Ctrl+C, X, V and Z, Ctrl+Shift+N, F5, Alt+Enter or Ctrl+I (properties), Ctrl+1 and 2 (list, grid), Ctrl+Alt+1 to 4 (sort) and Ctrl+Alt+R (reverse).

**The detail card.** Resting the pointer on a row or tile for 700 ms shows a card with the name, the type and size, the date, and in Recent and in search results of subfolders the folder (`files/hover_card.rs`). The card belongs to the view, not to a row, so it is not remounted as rows come and go while the list scrolls. The decision is `Track`, a pure state machine with its own unit tests (`cargo test --lib files::hover_card`), and the rules are:

- **Scrolling hides it at once and keeps it down.** Rows move under a resting pointer, so a card made for one is wrong a frame later. It hides in the frame the content starts to move (wheel, scrollbar, keys, drag), no row enter or leave brings it back while the window scrolls, and it comes back 700 ms after the content last moved (the window's 200 ms of stillness, then the rest of the wait). The row it is for is looked up again then from the geometry (`Ctx::row_at`: the lattice, the viewport and the scroll position, at the last pointer position), not taken from the last row event: Freya sends a pointer move after the layout of a wheel notch but none during the glide that follows or after a key's scroll, so the last row an event named is from the middle of the scroll and the pointer ends on another.
- **Moving to another row while it is up swaps it in place**: the same card element with the new text at the new place, with no wait and no frame without a card. Leaving every row hides it a frame later, so a leave and the enter that follows in the same frame are one swap.
- **One State write a frame, only when the row changes.** Rows report `enter` and `leave`; the same row again is ignored, and the one State the card reads is written by a task that runs at most once a frame and only when the card it would draw changed. The pointer events that scrolling makes (the synthetic move after the layout of a wheel notch) therefore re-render neither the list nor the view.
- **No disk, no waiting.** The text comes from the entry the listing already holds (`card_lines`, pure). A size and date the listing has not loaded yet (a network folder is statted in the background, as the rows are) read as `…` in a line of the same height, and the card draws again when the listing arrives, so it never changes shape. A view that shows a folder line (Recent, search results of subfolders) keeps that line, blank, on the cards that have none.

`files/hover_tests.rs` runs the view headless over three thousand files: the card down within two frames of the first notch and not drawn in any frame of a second of scrolling, back for the row then under the pointer once the content is still, the same after a scroll by keys (which sends no pointer event at all), a swap between rows with no frame without a card, and the view and list not rendering again for sixty scroll ticks. A zoom or a resize lays the rows out again with no pointer event, so it forgets the card too.

**Context menus of the file view.** A menu is grouped by task and holds only what can apply: a row that can never apply in that place is left out, not greyed. The empty-space menu is, in order: New folder, New file (a submenu of Empty file and the files of the XDG Templates folder when that folder has any, otherwise one row); Paste (greyed until something is copied, left out in Recent) and, only when there is something to take back, a named Undo such as "Undo Rename" (there is no Redo); Select all and Invert selection (left out in an empty folder); one View submenu; then Refresh and Properties. View holds List and Grid, Icon size (the sizes of the view on show, from the zoom ladder), Sort by (Name, Size, Type and Modified; Ascending and Descending; Folders first) and Show hidden files. Choices are ticked rows in one leading column, so labels line up. The header menu's Sort by is the same submenu. Shortcuts show on the right.

**Quick Look.** Space on the selected file opens a large card over a dimmed scrim, as macOS Quick Look and GNOME Sushi do; the context menu has Preview too. Space or Escape closes it; Left and Right (Up and Down too in a grid, and in a list they step one file) move the selection to the neighbouring file and update the card without closing it; Enter closes it and opens the file as a double click does; Alt+Enter or Ctrl+I closes it and shows Properties. Space only opens it when the file view itself has the keyboard: never while a rename field, the address bar or a filter has it, and not in the middle of a word typed to jump to a file. It works in the chooser dialog as well. What it shows depends on the type: a picture scaled to fit with its proportions kept; the first 64 KiB of a text, code, markup or JSON file in a monospace face, with a notice that says how much is shown when the file is longer; the largest thumbnail the desktop has made for a PDF, document, song or film, with the type's icon when there is none; a folder's item count and total size, counted while the card is open; an unknown type as text if it is text, else as its icon. The footer says the type, size, picture size or length and codec, and date. Everything loads off the UI thread. The stage is a fixed box and the type's icon is the placeholder at the size the content will have, so nothing moves when the content arrives, and stepping to another file drops the load of the last one (a folder count stops after its current entry).

**Properties.** Alt+Enter, Ctrl+I or the menu's Properties row opens a tabbed dialog for the selection, or for the folder on show when nothing is selected. The tabs are `segmented(...).tabs()`, the same bar the inspector uses, under the standard `HeaderBar`: Left, Right, Home and End move between them, Ctrl+1 to Ctrl+4 pick one, Ctrl+Tab and Ctrl+Shift+Tab step through them and wrap, Escape closes. On a phone the bar scrolls sideways when four titles do not fit. The last tab you used is the one the next dialog opens on (kept for the session; a selection that has no such tab opens on Basic).

| Tab | One file | One folder | Several items |
| --- | -------- | ---------- | ------------- |
| Basic | picture or icon, name, type with MIME type; size with the byte count; location (press it to open that folder); where a link leads; created, modified and accessed times; the drive with its name, device, mount point and a free space bar; in a view that can open files with another application, the applications that open the type | the same, with the size and item count added up on a thread (Stop ends it) | combined count, size and types, where they are, the drive |
| Permissions | a grid of read, write and execute boxes for owner, group and others, editable by the owner (and root) with `chmod` on a drive that is not read-only; the numeric mode, the special bits (set user ID, set group ID, sticky), "Allow executing as program"; owner and group; the SELinux context and an ACL marker where the system has them | the same, with the third column called Access, and **Apply to enclosed items**, which asks first, then sets everything inside to match (files keep an execute bit only where they had one and the folder allows it) with Stop | the grid acts on every item; a box where they differ is mixed (a dash), and pressing it sets the bit on all of them |
| Details | for a picture its size in pixels and, for JPEG, camera, lens, date and exposure; for audio and video the length and codecs (WAV, FLAC, Ogg Vorbis and Opus, MP4, M4A and MOV, Matroska and WebM; the length of an MP3 and the details of SVG, HEIF, AVIF, TIFF and JPEG XL are not read); the SHA-256 on demand, read on a thread with a bar, how far it has got and a Stop button, shown in two lines with a Copy button; inode, device, hard links, block size, disk usage; the extended attributes | the same without the checksum | none |
| Contents | none | item count, total size with Stop while it counts, and the five largest items with their sizes | none |

The card is one size for every tab: the header and tab bar stay and the page scrolls inside. Every row of a tab exists from the first frame, with "…" until its fact arrives and "—" when it has none, so nothing moves while facts load; what can only be listed once read (extended attributes, a folder's largest items, the security context) is the last group of its tab. What each tab holds while it is not shown (a running checksum, the modes just set, a count, each tab's scroll position) is kept until the dialog closes; closing it stops the count, the checksum and Apply to enclosed items. Values can be selected and copied, and a long path or name loses its middle with the shared elision (`path_label`). The pure parts are tested: `files/properties_tabs.rs` (which tabs, which opens, the keys), `properties_permissions.rs` (the bit of each box, mixed boxes, what a press does to each item's mode), `properties_words.rs`, and in `bingux_files::ops` the chmod of a tree (`tree_mode`, `chmod_tree`), the SHA-256 (`Hashing`, `Sha256`), `file_details` and the count's breakdown (`Counting::start_breakdown`). The card's size on every tab is a headless test.

**Moving, copying and removing safely.** The file operations are `bingux_files::ops`; the views call nothing else.

| Operation | What it does | What it promises |
| --------- | ------------ | ---------------- |
| Rename, and a move on one drive | One `renameat2(RENAME_NOREPLACE)` | Atomic. Nothing is copied or written. A name that is taken is never replaced unless you chose Replace. |
| Move between drives | Copy, flush, check, then remove the source | The source stays until the copy is durable and checked: every copy is as long as what was read, and nothing changed while it was read. A failure, a cancel, a full drive or a pulled cable leaves the source exactly as it was. Items you skipped are never removed. |
| Copy | Reflink (`FICLONE`) where the file system has one, else `copy_file_range`, else a 4 MiB buffer | Holes stay holes (`SEEK_DATA`/`SEEK_HOLE`), so a sparse disk image stays sparse. A full drive fails at the start of a file, not at the end. Many small files are copied by up to 8 threads (4 on removable media and shares); a big file is copied alone. Mode, times and extended attributes are kept as far as the drive has them (FAT has no modes and says so, which is not an error). |
| Any write | Data goes to `.bingux-*.part` beside the destination and is renamed into place | A half-written file never has its real name. A failed or cancelled copy removes its part files. Replacing swaps the new file in whole: the old one is intact until then. |
| Delete, rename and new folder | A single call | On removable media the folder is written out before the call returns. |

**Removable media.** A copy, move or delete on a USB stick, SD card, external disk, network share, or a FAT, exFAT or NTFS drive is only *done* when the data is on the device. The drive is recognised from the mount table, the device's `removable` flag and a USB bus in `/sys` (also under an encrypted layer). For those drives every file is `fsync`ed before it gets its name, then every folder that changed, then the whole drive is written out (`syncfs`). While that last step runs the status line says **Finishing… writing to <drive>** with a bar that keeps still and no Stop button; the line that follows says **Everything is written to <drive>.** An ordinary internal disk is not flushed: that is the kernel's job, and flushing would only make copies slower. **Eject** (and Unmount) in the sidebar write the drive out first, then unmount; if the drive is busy it says so in the tool's own words, and when it is ejected the toast says the drive is safe to remove. A drive that cannot be written out is not detached.

**Removing.** Delete and *Move to Trash* put the item in the Trash: `$XDG_DATA_HOME/Trash` on the home drive, else the bin of its own drive (`$topdir/.Trash/$uid` when the admin made a sticky `.Trash` that is not a link, else `$topdir/.Trash-$uid`, mode 0700, ours). The choice is made from the mount table (`ops::trash` has a pure `plan_bin` the tests drive with a made-up one); an item is never copied to another drive to bin it, except on the home drive's own mount points when a `rename` there says `EXDEV`, and then only for a small tree: a big one answers `OpError::CopyNeeded` and a dialog asks **Copy to Trash?** (`trash_copying`). Each item gets an `info/NAME.trashinfo` made with `O_EXCL` (`Path` percent-encoded from the raw bytes, `DeletionDate` in local time), a clash is numbered `name.2` (`name.2.tar.gz`), a folder's disk use goes into `directorysizes` (written through a temporary file and renamed), and on removable media, shares and FAT the info file, the folders and the drive are made durable before the move reports done. The toast says **Moved 3 items to Trash** (**Moved “a.txt” to Trash**) with a quiet **Undo**, and Ctrl+Z does the same. *Delete permanently* is **not** in the ordinary context menu outside the Trash. It appears when you open the menu with Shift held (Shift and right click), and Shift+Delete does it from the keyboard. Both open a question first: it names the item (or says "3 items"), says that it cannot be undone and will not go to the Trash, has a red **Delete permanently** button, and gives the focus to Cancel, so Enter keeps the files. What the Trash cannot take is told with its reason (`TrashRefusal::reason`) and never deleted quietly: a drive with no Trash, a read-only drive, something in the Trash already and a refused permission open the same question with the reason; the root, the home folder, your Documents/Pictures/… folders, a folder that holds home or the Trash and the Trash itself are refused with a message and no offer. Rename, move and Move to Trash can be undone; deleting for good cannot, and the question says so.

**Inside the Trash.** A folder that is a bin's `files` folder (the Trash place, or `$topdir/.Trash-$uid/files` of a drive) lists each item with its `.trashinfo` (`Entry::trash`: original place and deletion date; the loader never takes this listing from its cache). The list has **Original Location** (the folder it was in, drawn with the shared path label) and **Deleted** (how long ago) in place of Type and Modified, sortable by Deleted; hidden items are shown; the status line says **Trash · 12 items**; an empty Trash is the status page **Trash is empty**. The context menu has **Restore**, **Restore to…**, **Delete permanently** (always, no Shift, and the plain Delete key does it; the question has the focus on Cancel), Copy, Open, Preview, Copy path and Properties (which add **Original location** and **Deleted**), and none of Move to Trash, Rename, Compress, Cut, Paste or New folder (`actions::in_trash`). **Empty Trash** is in the empty-space menu (greyed while empty); its question first counts the items and their disk use on a worker with a Stop button (`ops::trash_summary`), says them ("124 items (3.2 GB) in the Trash will be deleted for good"), and the work runs off the UI thread with a progress bar and Stop. Restore asks only what needs asking: to make a folder that is gone again, and, for a name that is taken, Keep both, Replace (what was there goes to the Trash, so it can be undone) or Cancel; an item with no record of its place is told so and offered Restore to…; a read-only drive, a missing permission and a file where the folder should be are reported with the item and the folder. It says **Restored 3 items** with an Undo that puts them back in the Trash (`Step::Restore`). Dragging files onto the sidebar's Trash row trashes them (Ctrl cannot copy into it), dragging items out of the Trash into a folder restores them there, and nothing can be dropped inside a folder of the Trash. The sidebar's Trash row is drawn by `FileIcon::for_trash(full)` (faint while empty) and its menu is Open, Empty Trash (greyed while empty), Properties.

A filter takes file extensions, globs and MIME types in one string:

```rust
ChooserRequest::open()
    .filter_spec("Pictures", "png jpg image/webp")   // extensions and a MIME type
    .filter_spec("Documents", "*.pdf *.odt")          // globs
    .presets(true)                                     // and the built-in ones
```

Keyboard: Tab walks the toolbar, list and the strip under it. In the list the arrows, Home,
End and Page keys move (Shift extends when several files are allowed), typing a
name starts a search of the folder (see [Search](#search-in-the-file-views)),
Ctrl+A selects all, Enter opens. Ctrl+L, or typing `/` or `~`,
edits the path as text. Backspace or Alt+Up goes to the parent folder, Alt+Left
back, Alt+Right forward, Alt+Home home, Ctrl+H toggles hidden files, and Escape
cancels. On touch a tap opens a folder and selects a file.

Folders at the XDG user directories (Documents, Downloads, Music and so on)
get their own icon. It is decided by the path alone, as the person sees it:
`~/Downloads` keeps the Downloads icon when it is a link to a share, and the
share's own folder, reached by its real path, is a plain folder. No call
touches the disk, so it costs nothing on a network mount.

### Search in the file views

The address bar is the search box. One text field takes a path or a search, and
it decides from what is typed, without touching the disk:

| Typed | Is |
| ----- | -- |
| starts with `/`, `~`, `./`, `../`, or a scheme such as `smb://` | a path |
| the name of an entry in the folder (`Documents`, `Documents/Sub`) | a path |
| anything else (`report`, `2024 invoice`) | a search |

The bar has three states that share one frame and one height, so nothing moves
when it changes: the **trail** of crumbs, the **path** field with folder
completions (Ctrl+L), and the **search** field, which has the search glyph and
a chip `in <Folder>` inside it. The chip's close button, or Escape, ends the
search. Going to another folder ends it too.

**Filter this folder.** Ctrl+F, or typing a letter on the list, focuses the bar
in search mode (the list's jump-to-a-name is replaced by this). The loaded
listing is filtered as each letter is typed; nothing is read from the disk.
Matching ignores case and looks at names only, in this order: the exact name,
a name that starts with the text, a word that starts with it, the text
anywhere, every word of it in any order, and last the letters in order (`rpt`
finds `my_report.txt`, for three letters or more). The list keeps your sort
inside each of those groups. Matched letters are drawn on an accent tint, and
the status pill reads "3 of 120 match". Up and Down move the selected
match without leaving the field, Enter opens it (a folder is entered, a file is
opened or chosen; with no match it searches the subfolders), and Escape clears
the text, shows the whole folder and gives back the selection you had before.
Clicking a result keeps the filter until Escape.

**Search the subfolders.** While there is text, the last row under the field is
always *Search subfolders for "text"*; Ctrl+Enter or Shift+Enter does the same.
Results stream into the list as a background walker finds them, shallow folders
first, with a live count in the row and the status pill; the row says **Stop** and
pressing it ends the walk (what was found stays). Changing the text, going to another folder or
closing the dialog cancels the walk. Each result shows its name and, in a list,
a **Location** column with its folder below the folder searched (a grid shows it
on the detail card, and a phone's list on the second line). The walker:

- reads one folder at a time on one thread, breadth first, and stops at
  10,000 results and says so;
- skips hidden folders and hidden files unless *Show hidden files* is on;
- never follows a symbolic link, never enters another filesystem (a mounted
  drive or network share, `/proc`) and never enters a folder twice;
- looks in everything you could browse to, including `node_modules`, `target`
  and `.git` (when hidden files show), because the view menu's **Search build and
  dependency folders** switch is on by default. Turn it off to skip
  `node_modules`, `.git`, `.hg`, `.svn`, `target`, `__pycache__`, `.venv`, `.tox`,
  `.gradle` and `.cache`. The switch lasts for the dialog.

The Bingux search service (`bingux-searchd`) is not used: it is a launcher
search over a fixed set of roots, with results capped at the launcher's size,
over a private socket; it cannot be limited to a folder or say which folders it
covers. File **contents** are not searched, only names.

The file type filter of a chooser is a separate thing. It stays on during a
search, so searching a "Pictures" chooser finds only pictures (folders still
appear). In the folder it narrows the list silently (the empty state says when it hides everything); in
the subfolders the walker applies the filter before it counts, so rejected files
neither show nor use up the 10,000 results. In the model
(`bingux_files::search`) a search is a `Query`; a filter is a `Filter`; the two
never share a type. Apps build the same behaviour with
`AddressBar::search(SearchLink::new(find, selection, folder)...)`,
`FileView::search(..)` / `.on_search(..)` and `use_subfolder_search(..)`; the
chooser is the reference.

### File icons

One API decides which icon a file, a folder, a place or a drive has, and one
element draws it. Every surface asks it: the sidebar's places, bookmarks and
drives, list rows, grid tiles, the address bar's crumbs and their menus,
typed-path suggestions, Properties, Quick Look, the drag chip, the chooser,
the Files app's tabs and the file-type rows of Settings > Applications. No
surface picks a glyph itself, so one folder looks the same everywhere.

```rust
use bingux_ui::files::{FileIcon, FileKind, file_icon_view};
use bingux_ui::style::IconSize;

let spec = FileIcon::for_entry(&entry);                  // a listed entry: link, hidden, type
let spec = FileIcon::for_path(path, bingux_files::Kind::Directory); // a path: its text decides
let spec = FileIcon::for_place(&place);                  // a sidebar place or bookmark
let spec = FileIcon::for_mime("application/pdf");        // a type the system reports
let spec = FileIcon::for_name("report.ods");             // a name with no entry yet

file_icon_view(&tokens, &spec, IconSize::Row, selected)  // a token size ...
file_icon_view(&tokens, &spec, 64u16, selected)          // ... or pixels (the view's zoom)
```

A `FileIcon` is data: the `glyph`, the `kind` (`FileKind`), an optional
`emblem`, a `tint` and `thumbnail`, where a picture of the file may come from
(the view replaces the glyph with it when it can; a folder never has one).
`FileKind::of_mime` is the one classification of a MIME type, shared with the
file-type rows in Settings.

**Which glyph.**

- A **folder is decided by the text of its path** and never by where a link
  leads. The special folders are the home folder, the XDG user directories
  (the paths `user-dirs.dirs` configures, and the usual names under home:
  Desktop, Documents, Downloads, Music, Pictures, Videos, Templates, Public),
  the trash (`~/.local/share/Trash/files`) and the filesystem root. A key that
  `user-dirs.dirs` writes as `$HOME/`, how `xdg-user-dirs` disables a folder,
  does not make home that folder, and the usual name still counts. So
  `~/Downloads`, a link to `/mnt/x/Downloads`, has the Downloads glyph and
  `/mnt/x/Downloads` has the plain folder. Case counts and a trailing slash
  does not. Any other folder, a bookmark such as Projects included, has the
  plain folder, in the sidebar and in a list alike. Only a directory is asked:
  a file called `Downloads` is a file.
- A **file** is its MIME type's kind: image, audio, video, text, code, PDF,
  archive, spreadsheet, presentation, document, font, program or unknown. A
  type's own name decides first (`application/pdf`); the extension table's
  coarse category decides when the name says nothing; an extension-less file
  that is executable and of no known type is a program.
- A **drive** has the glyph of its kind (disk, removable, network, optical).
- A **symbolic link** keeps the glyph of what it is and gains the link emblem,
  a disc with an arrow, at half the glyph's size and never below 8 px, over the
  lower right corner. Only a listed entry carries it: places and crumbs never
  read the disk.
- A **hidden** file or folder is drawn faint.

**Which colour.** One rule on every surface: the glyph is the **accent only
when its row or tile is selected**, muted otherwise, fainter when hidden or
when a drive does not answer. A folder is told from a file by its shape, not
its colour. On a floating surface (the drag chip) `file_icon_view_floating`
uses the ink that reads there.

**Adding a glyph.** If a kind or special folder needs a glyph the set lacks, add
it through `scripts/icons` (see the icon set above): a row in `labels.tsv` with
where it is used, `build.py --check` passing, then the `Icon` variant. Never draw
a surface's own glyph.

The inspector shows each icon as `FileIcon` with its glyph and size; the
taxonomy page "File icon" shows every kind at 16, 32, 64 and 128 px, resting and
selected, the folders a path decides, places and drives, and the emblem.

### Layers

| Layer | Where | Depends on |
| ----- | ----- | ---------- |
| Filesystem model: listing, MIME, places, bookmarks, recent files, filters, name search and the subfolder walker, sort, inotify watching, selection, request and answer types, what accepting does | `packages/bingux-files` (crate `bingux-files`, `libc` only, unit tests) | nothing |
| Views: `PlacesSidebar`, `PlacesMenu`, `AddressBar`, `FileView`, `use_directory`, `use_places`, `empty_state`, `error_state` | `bingux_ui::files` | the model, bingux-ui controls |
| The dialog: `choose` | `bingux_ui::files` (also in the prelude as `files`, `ChooserRequest`, `Chosen`) | the views |
| The Files window: `browse` | `bingux_ui::files`, hosted by `packages/bingux-files-app` | the dialog's body |

The model has no UI and no async runtime, so the future Files app, a command
line tool and a portal backend use it as it is. Listing blocks on the disk
and takes a `Cancel`; `bingux-ui` runs it on its worker pool and keeps the
folder current with an inotify watch (bursts merge, a changed folder lists
again without showing the loading state). MIME types come from a table of the
common shared-mime-info globs embedded in the crate: it needs no host files
and no dependency, and, like GIO's fast content type, looks at the name
only, never at file contents. The list builds only the rows in view, so a
hundred thousand files scroll as one hundred do, and the grid decodes each
thumbnail only when its row scrolls in. The views are ordinary components:
`PlacesSidebar::new(places, current, on_open)`,
`AddressBar::new(location, home, editing, draft, on_open, on_file)` and
`FileView::new(listing, order, selection).icon_px(level).sort(..).multiple(..)
.on_pick(..).on_activate(..)`. The Files app (`packages/bingux-files-app`) uses them as they are, through `browse`.

### The file view in a window

`files::browse(Browse::new(memory))` is the dialog's body, unchanged, in an application window: the same toolbar, sidebar, address bar, list or grid, search, previews and operations. The dialog and the window are one component, `ChooserBody`, with a host: a dialog answers a request; a window has none to answer. In a window there is no Cancel or accept button, no file type filter, no name field and no choices, and what opening a file means changes: it opens in the application `xdg-open` names for its type, where a dialog answers with it. `Capabilities::FILES_APP` turns on **Open with**, **Open in new tab** and a middle click on a folder.

```rust
let memory = TabMemory::new(Where::Folder(home));    // one per tab
files::browse(
    Browse::new(memory)
        .on_location(move |at| title_follows(at))    // the tab went to another folder
        .on_new_tab(move |path| open_tab(path)),     // middle click, "Open in new tab"
)
```

A tab's `TabMemory` is where its body keeps the folder shown and the Back and Forward lists, so a tab that was hidden (and unmounted: it holds no listing and no watch) is where it was left. It also has a number of its own, which the view uses to key what it remembers of each folder it leaves (how far the list was scrolled and what was selected): a tab shown again, and Back, Forward and the parent folder, put the list back where it was, and two tabs on one folder keep separate places. Where the body is not in a tab (the chooser) the number is 0. Remembered state is shared with the chooser: `files::remember(key, &value)` and `files::remembered(key)` keep a value in the same file as the per-folder views (Files keeps its window size there).

**Scroll position.** The list or grid keeps its offset in a `ScrollController` that the file view owns, and the offset changes only when the person scrolls (wheel, scrollbar, touch), a key reveals the cursor row, a zoom step or a width change anchors the content under the pointer, or a search that changes the rows starts at the top (`files/scroll_memory.rs` holds that rule and the per-folder memory, both tested without a window). A thumbnail arriving, the folder changing on disk, F5, a selection or the pointer resting on a row never moves it. Do not call `ScrollController::scroll_to_*` from `use_side_effect`: it reads the controller's bounds, which the scroll view rewrites on every layout, so the effect subscribes to them and runs again after every wheel notch (the file view once did this and put the list back at the top a moment after each scroll). Decide in render, and apply from an event handler or a `spawn`ed task.

**Dragging between parts of a window.** A view drags files by itself onto the folders of its own list. A host with other targets calls `files::provide_file_drag()` once, above them and the view. The view then says a drag is on; a target calls `FileDrag::hover(folder)` and `leave(folder)`, and highlights itself while `FileDrag::is_over(folder)`; the view moves (or, with Ctrl, copies) into that folder when the pointer is released, through the same operation as cut and paste. The places sidebar is a target already, and `TabStrip::on_hover` is the hook for tabs. Without a provided `FileDrag` a drag stays inside its list, as in the dialog. Dragging to or from other applications needs drag and drop in the windowing layer, which Freya and its winit version do not have on Wayland.

**Drives that are not mounted.** The sidebar's Drives group also lists each USB stick, card or disc that is plugged in and not mounted (`bingux_files::volumes::list_volumes`): the file systems in `/dev/disk/by-uuid`, on removable media or a USB bus, read from `/sys` and udev's database, so listing opens no device. Its row says "Not mounted" and its size; opening it runs `udisksctl mount` on a worker and then opens the folder it was mounted at, or says in a toast what `udisksctl` said. The list follows plugging and unplugging through an inotify watch beside the mount table's, so nothing polls.

### Opening a folder fast, on any drive

`bingux_files::loader::open` is how every view opens a folder. It never
blocks the caller and sends events: `Kind`, `Cached`, `Batch`, `Done`, `Diff`,
`Stats`, `Unresponsive`, `Failed`. A view applies each with
`Listing::apply_event`, which returns true when it should select again, and
asks `Load::want_stats(&listing, rows_in_view)` on every frame.

| Step | What happens |
| ---- | ------------ |
| Cache | The folder's last listing is shown at once from `$XDG_CACHE_HOME/bingux/files/v1` (about 10 bytes an entry; 32 MiB and 512 folders at most, least recently used deleted first). One `statx` of the folder says whether it changed. If not, nothing else is read, locally or over NFS. If it did, only the difference arrives as `Diff`, so rows are patched and the scroll position and selection stay. A name is stored as the bytes of the file (not the text it is shown as), so a name that is not valid UTF-8 comes back as the same file; files of an earlier format (`BFC1`) stored it as replacement characters and are dropped. |
| Streaming | With no cache the first 50 entries arrive after about 2 ms and the rest in batches that grow, so a 20,000-entry folder is complete in about 50 to 90 ms. |
| Drive kind | The kind (`Local`, `Removable`, `Network`, `Optical`, `Other`) comes from the folder's own device and the mount table, not its path: `~/Downloads` that links to an NFS share is `Network`. |
| Network drives | Names and types only, one worker, no thumbnails, no reading of file content (`Listing::mount.thumbnails()` is false). Size and time come only for the rows in view, newest request first. |
| Slow drives | A drive that stops answering gives `Unresponsive` after two to three seconds. The listing stays as it was and leaving the folder never waits. |
| Changes | inotify on local disks; a stamp check every five seconds on network ones. Both end as a `Diff`. |

Sorting and filtering 100,000 entries take tens of milliseconds
(`Listing::select`, `select_counted` also returns how many entries the filter
and the hidden setting left out). Filters are built from what an author writes:
`Filter::parse("Pictures", "png jpg image/webp")`, or the presets
(`Filter::images()`, `audio()`, `video()`, `documents()`, `archives()`,
`text()`); `ChooserRequest::effective_filters()` is the list the menu shows.
Drives, with size and free space, come from `bingux_files::mounts`
(`list_mounts`, then `fill_space` on a worker, `watch_mounts` for changes).
Which folders get the Documents, Downloads, ... icon is decided from the path
text only (`places::special_folder`): no `stat`, no `readlink`, so it costs
nothing on a network mount, and `~/Downloads` keeps its icon when it is a link
while the target folder, reached by its real path, shows a plain folder.
Timings: `packages/bingux-files/examples/bench.rs`.

### Portal mapping

`ChooserRequest` and `Chosen` map one to one onto
`org.freedesktop.portal.FileChooser`, so a later `bingux-contrib-portals`
backend (`org.freedesktop.impl.portal.FileChooser`) can host this dialog:

| Portal | Here |
| ------ | ---- |
| `OpenFile`, `SaveFile`, `SaveFiles` | `Action::Open` / `Action::SelectFolder`, `Action::Save`, `Action::SaveFiles` |
| `title` | `title` |
| `accept_label`, `modal`, `multiple` | the same names |
| `directory` | `Action::SelectFolder` |
| `filters` `a(sa(us))` | `filters`; `Filter::from_portal`, `Filter::to_portal`; rule kind 0 is a glob, 1 a MIME type |
| `current_filter` | an index into the effective filters: `filters`, or the presets when it is empty, or `filters` then the presets after `presets(true)` |
| `choices` `a(ssa(ss)s)` | `choices`; no options means a yes/no choice with `"true"` or `"false"` |
| `current_name`, `current_folder`, `current_file`, `files` | the same names |
| response `uris` | `Chosen::paths` through `location::file_uri` |
| response `current_filter`, `choices` | `Chosen::current_filter`, `Chosen::choices` |
| response code 1 (cancelled) | the callback gets `None` |

Globs ignore case, so `*.png` shows `A.PNG`. `ViewMode` is the one option the
portal does not have; a backend leaves it on `Auto`.

### Versions and the system service

`files::VERSION` numbers the compiled-in dialog and rises when a request option
or the dialog's behaviour changes. The design for sharing the dialog with other
apps: `bingux-contrib-portals`, a package that depends on `bingux-ui` and
`bingux-files`, runs the portal backend and hosts the same dialog body in its
own window, exposing its version as `org.bingux.Portals.Version`. `choose`
then picks the newest of the compiled-in dialog and the service: it asks the
service its version, calls the portal when that is higher, and uses the
compiled-in dialog when the service is missing, older or fails. The compiled-in
dialog always works. The package is not built yet: it needs a D-Bus client
(the `dbus` crate over libdbus is the only one in the offline cache; no
`zbus`) and a hosted native window.

Capture the dialog with `scripts/file-chooser-fixture.py DIR` and
`HOME=DIR cargo run --example file_chooser --features testing -- OUT` (every
mode at 1100x820 and 390x800). Add `--link-downloads` to the fixture script
to make `~/Downloads` a link to a folder outside the home. Set
`BINGUX_FILES_STATE=FILE` to keep your own remembered views out of the
captures. With `FILE_CHOOSER_LIVE=SCENARIO` the example opens a real window
that reopens the chooser after every answer, for driving with real input.

## Search

`bingux_ui::search` is the search every Bingux app shares. An app describes
what can be found, registers it once, and asks for hits as the user types.
It writes no matcher: typo tolerance, ranking, highlights, filters, facets and
Spotlight come with the SDK. The engine is the UI-free crate
`packages/bingux-search`, re-exported as `bingux_ui::search`; the design and
the numbers behind it are in `design/search.md`.

### Register what is searchable

A document is an `id` and JSON fields, like a Meilisearch document. The
schema says which fields are searched (with a weight), filtered, sorted and
shown.

```rust
use bingux_ui::search::{self, Document, Schema, Searchable};

struct Pages;

impl Searchable for Pages {
    fn name(&self) -> &str { "pages" }

    fn schema(&self) -> Schema {
        Schema::new()
            .searchable("title", 10.0)        // a title match outranks...
            .searchable("keywords", 5.0)      // ...a keyword match...
            .searchable("breadcrumb", 3.0)    // ...a path match
            .filterable(["category"])
            .displayed(["title", "breadcrumb", "icon", "uri"])
            .synonym_group(&["colour", "color"])
    }

    // Called on a worker thread, never on the UI thread.
    fn documents(&self) -> Box<dyn Iterator<Item = Document> + Send + '_> {
        Box::new(PAGES.iter().map(|p| {
            Document::new(p.id)
                .title(p.title)
                .breadcrumb(["Network", p.title])
                .keywords(p.words)
                .category("Network")
                .icon("network")
                .uri("my-app://network/proxy#method") // where choosing it goes
        }))
    }
}

search::register(Pages)?;       // once; the index exists at once and fills on a worker
search::reload("pages");        // when the data changed (apps installed, a list refreshed)
```

Conventional fields (builder helpers, nothing special in the engine): `title`,
`breadcrumb` (the path, outermost first), `keywords`, `body`, `category`,
`icon` and **`uri`**, the app's own address for the thing, such as
`bingux-settings://network/proxy#method`. A hit is opened by handing its `uri`
to the router (`router.navigate(uri)`); Spotlight does the same.

| Schema setting | Meaning | Default |
| -------------- | ------- | ------- |
| `searchable(field, weight)` | Searched fields; weight scales BM25F and sets the attribute rank | required |
| `filterable([..])` / `sortable([..])` | Fields usable in `filter` / `sort` and as facets | none |
| `displayed([..])` | Fields returned in `_source` | all |
| `ranking_rules(vec)` | Order of `Words, Typo, Proximity, Attribute, Exactness` | all five |
| `synonym(word, [..])`, `synonym_group(&[..])`, `stop_words([..])` | Query-side | none |
| `typo(TypoSettings)` | One typo from 4 letters, two from 8, first letter exact | on |
| `external(bool)` | Other processes (Spotlight) may search this index | **false** |

### Search as the user types

```rust
let state = search::use_search("pages", &text);   // never blocks
for hit in &state.hits {
    // hit.title(), hit.uri(), hit.breadcrumb(), hit.category(), hit.icon(),
    // hit.ranges("title") -> Vec<Range<usize>> of the matched bytes
    row(hit.title().unwrap_or_default())
        .title_marks(hit.ranges("title"))             // draws the match
        .on_press(move |_| router.navigate(&uri))
}
if state.hits.is_empty() && !state.loading { /* "No results" */ }
```

`SearchState` holds `hits`, `total` (`total_exact` says whether it is a lower
bound), `took`, `loading` (a search is running or the index is still loading),
`query` (the text these hits answer) and `error`. While a newer search runs the
previous hits stay on screen, so a list never blinks empty between keys. A
blank query has no hits. `use_search_with(Search, active)` takes a whole
request: filters, facets, paging, several indexes.

The search runs on the worker pool of `bingux_ui::task`: a new keystroke drops
the one before (a queued search is skipped, a running one stops within about a
thousand candidates), so there is no debounce. Do not call `Index::search` from
`render` or an event handler.

### Run a query yourself

```rust
use bingux_ui::search::{Cancel, Search, service};

let r = service().search(
    &Search::new("blutooth")                      // typo tolerant, last word is a prefix
        .index("pages")
        .filter("category = 'Network' AND price > 3")
        .sort("price:asc")
        .highlight(&["title"])
        .facet("category")
        .limit(8),
    &Cancel::new(),
)?;                                               // blocking: use a worker
let json = r.to_json();                           // OpenSearch `_search` body
```

Three ways to say the same thing: the builder above; the Meilisearch shorthand
`Search::from_json(&json!({"q": "proxy", "filter": "category = Network",
"facets": ["category"], "limit": 8, "attributesToHighlight": ["title"]}))`; and
an OpenSearch body.

**Query text.** Words are matched with typo tolerance; the last word is a
prefix unless the text ends with a space; `"quoted words"` must be adjacent in
one field; `Wi-Fi`, `wifi` and `wi fi` all match. `matching_strategy`:
`Last` (builder default: all words first, then fewer), `All`, `Any`.

**Filters** (`filterable` fields): `=`, `!=`, `<`, `<=`, `>`, `>=`, `a TO b`,
`IN [..]`, `EXISTS`, `NOT`, `AND`, `OR`, parentheses.

### The response

The response is OpenSearch's `_search`:

```json
{ "took": 0, "timed_out": false,
  "_shards": { "total": 1, "successful": 1, "skipped": 0, "failed": 0 },
  "hits": { "total": { "value": 1, "relation": "eq" }, "max_score": 0.97,
    "hits": [{ "_index": "pages", "_id": "network/proxy", "_score": 0.97,
               "_source": { "title": "Proxy", "uri": "my-app://network/proxy" },
               "highlight": { "title": ["<em>Pro</em>xy"] } }] },
  "aggregations": { "category": { "doc_count_error_upper_bound": 0,
    "sum_other_doc_count": 0, "buckets": [{ "key": "Network", "doc_count": 1 }] } } }
```

`_score` is in `[0, 1]` (1 for a match-all); `relation` is `gte` when the count
is a lower bound (more than `track_total_hits`, 10 000 by default, or a scan
cut off, below). The typed `SearchResponse` and `Hit` carry the same data plus
`matches`, the matched byte ranges per field, so a UI draws marks without
parsing `<em>`.

**OpenSearch subset accepted** (anything else is a `parsing_exception` naming
the key): `query` with `match_all`, `match`, `multi_match` (`fields` with
`^boost`, `operator`, `type`), `match_phrase`, `match_phrase_prefix`, `prefix`,
`term`, `terms`, `range`, `exists`, `bool` (`must`, `should`, `filter`,
`must_not`, `minimum_should_match`); `from`, `size` (at most 1000;
`from + size` at most 10 000), `sort` (a field, `_score`, `_id`),
`search_after`, `_source`, `highlight`, `track_total_hits`, `timeout`,
`aggs` with `terms`. `fuzzy`, `nested`, `script`, other aggregations and
`function_score` are not supported.

**Limits.** An index holds up to 2 000 000 documents of up to 64 KiB and about
1 GiB (`Limits`); an upsert past a limit fails and keeps what came before. A
word scans at most 100 000 postings: a one-letter prefix in a million
documents reports a lower bound for `total`, and a term in more than 10 000
documents skips the length part of BM25. Typing more letters narrows both.

### Show Spotlight what the app searches

Search is shared with Spotlight (`bingux-searchd`) only if the app says so,
index by index:

1. `Schema::external(true)` on the index. The default is private. A document
   may hide itself (`Document::external(false)`), never expose itself.
2. `search::serve_external(ExternalConfig { app_id, app_version })` opens
   `$XDG_RUNTIME_DIR/bingux/search/<app-id>.sock` (mode 0600 in a 0700
   directory) while the app runs and keeps a snapshot of the external
   documents in `$XDG_STATE_HOME/bingux/search/<app-id>/<index>.idx`, so the
   app can be found while it is closed. Call it from the window root: a second
   launch, which only hands its address to the running copy, must not take the
   socket.
3. Install `<data dir>/bingux/search-apps/<app-id>.json`
   (`packaging/search-apps/bingux-settings.json`): `{"kind":
   "bingux.search-app", "id": "bingux-settings", "displayName": "Settings",
   "command": ["/usr/bin/bingux-settings"]}`. Spotlight asks only apps that
   have one, and opens a hit by running `command` with its `uri` (a second
   launch hands the address to the running window).

The user switches an app off with the `search.disabled_providers` preference
(the provider id is the app id) and weights it with `appWeights` in the
daemon's configuration. Private
data (Wi-Fi networks, devices, anything about the person) stays in a private
index. The socket speaks newline-delimited JSON, as the other Bingux sockets do:

```sh
$ printf '%s\n' '{"v":1,"type":"hello"}' \
    | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/bingux/search/bingux-settings.sock
$ printf '%s\n' '{"v":1,"type":"search","id":"q1","index":"settings","body":{"q":"proxy","limit":3}}' \
    | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/bingux/search/bingux-settings.sock
```

`hello` lists the external indexes; `search` takes any request body above and
answers `{"v":1,"type":"result","id":"q1","response":{...OpenSearch...}}`;
`activate` (`index`, `doc`) hands the document's `uri` to the app;
`subscribe` streams `changed` events. A private or unknown index answers
`index_not_found_exception`. The whole protocol is in `design/search.md`
section 8.

### Searching a folder is not this

The file view's address-bar search (`bingux_files::search`) matches file names
of one folder, keeps the user's sort and reads no index; it stays as it is.
The engine is for records an app owns (settings, applications, contacts,
notes, devices), up to about a million short documents.

## Charts

`charts` draws sparklines, line and area charts, bar charts, rings and
donuts. Each chart fills the width it is given and keeps a fixed height from
its first frame, so data that arrives later never moves the layout. Before it
has points it shows a muted baseline and "No data yet" at the same height.
A single x value is centred with one tick at the value, not a -1 to 1 axis. A constant series sits in the middle of a window and labels its one value. Time axes span the data, not whole hours.

A chart is free content, so it has no padding of its own: put it in a
`boxed(tokens)` card, or on a group with `Group::content(chart)`, which keeps it
16 px from the card's edge on every side like any free content. (`Group::row` is
for rows, which sit flush against the card.) Inside the box, the y labels have
12 px of air before the plot, the x labels 8 px under it, and a legend sits
16 px under the x labels, lined up with the left edge of the plot.

```rust
use bingux_ui::prelude::*;

line_chart([Series::new("Charge", points)])
    .y_unit(Unit::Percent)
    .curve(Curve::Monotone)
    .area()
    .label("Battery charge over the last day")
```

| Builder              | Use                                                                            |
| -------------------- | ------------------------------------------------------------------------------ |
| `sparkline(values)`  | A small trend with no axes. `colour`, `unit`, `format`, `area`, `label`.       |
| `line_chart(series)` | Several series over x. `y_unit`, `y_format`, `x_unit`, `curve`, `area`, `label`, `height`. |
| `bar_chart(series)`  | Grouped bars, or `stacked()`. `categories`, `details`, `y_unit`, `y_format`, `label`, `height`. |
| `ring(value)`        | A 0 to 1 gauge with a centre caption. `caption`, `colour`, `label`.            |
| `donut(segments)`    | A share of a whole, with a legend. `unit`, `caption`, `label`.                 |
| `timeline_chart(bars)` | Bars along a time axis, a limit line and a draggable range. |
| `flame_graph(tree)`  | Weighted calls as stacked bars, widest first.                                  |

`Series::new(name, points)` takes `(x, y)` pairs, and `Series::values` reads
values by position. `Segment::new(name, value)` is one slice of a donut.

`timeline_chart(bars)` takes `Bar::new(at, width, value)` in ascending order of
`at`. `x_range` and `y_max` fix the axes, `x_unit` and `y_unit` format them,
and `limit(value, name)` draws a dashed line and turns the bars over it red.
`selection(range)` and `on_select(handler)` make the range the user drags
yours to keep: the handler gets `Some((from, to))` when a drag ends and `None`
for a click or Escape. Left and Right walk the bars, and Shift with an arrow
selects.

`flame_graph(tree)` takes an `Rc<FlameTree>`. Build the tree with
`push(parent, name, weight)` (node 0 is the root), `add` to grow a weight,
`label` for the shorter text a bar shows, `highlight` to tint a bar in the
accent colour, and `sorted()` last. Bars show their label cut with an
ellipsis; the detail line beneath gives the full name and shares of the whole.
A click zooms into a bar, Escape zooms out, and the view starts past the calls
at the top that only call one thing. Down, Up, Left and Right walk the tree.

`Unit` formats values: `Number`, `Percent` (values already in percent),
`Bytes` (binary units, `3.2 GiB`), `DecimalBytes` (decimal units, `3.2 GB`,
with ticks on round decimal numbers), `Duration` (seconds as `2 h 15 min`) and
`Clock { offset }` (Unix seconds in local time). For anything else, `y_format(|value| ..)`
on a line chart, bar chart or timeline (and `format` on a sparkline) writes the
axis ticks and the tooltip values with your own function: `.y_format(|mbit| format!("{mbit:.1} Mbit/s"))`.
It receives round numbers for ticks and the data as it is for tooltips, so it
rounds for itself. `Curve` is `Linear`, `Step`
or `Monotone` (the default, a smooth curve that never overshoots).

Data handling: a `NaN` or infinite point breaks the line instead of joining
across it. Points are sorted by x, and a repeated x keeps its last value.

Motion: lines draw in from left to right, bars grow from the baseline and
rings sweep, over 700 ms. Under `BINGUX_REDUCED_MOTION=1` they appear at once.

Hover: a line chart draws a crosshair and a dot on each series at the nearest
x, and a bar chart fades the other bands, so the hovered bar stands out. A
tooltip card (the popover surface, `Raised` elevation) lists the x label, then
each series with its swatch, name and formatted value, and for stacked bars the
total under a hairline. It sits beside the stop on the side with room, flips
near an edge, and moves from stop to stop, never with the pointer, so it does
not jitter; it goes when the pointer leaves the chart. A bar chart's axis labels
thin to every few bands from the first, so they stay evenly spaced; give the
tooltip a longer heading than the axis label with `details(names)`
(`"14:00 - 15:00"` for a band the axis calls `"14:00"`). On touch, press and
hold shows the tooltip and sliding the finger moves it; lifting hides it, and
a swipe that scrolls the page over a chart shows nothing.

Keyboard: a chart with points is focusable. Left and Right move between
points, Home and End jump to the ends, and the tooltip follows the point.

## Typography

`typography.rs` holds the type ramp. Text picks a named style and nothing
sets its own size or weight, so each level reads apart at a glance: size
steps of at least 1.25x, and a weight or colour change at each step. Use
`styled_label` and `styled_paragraph`, or `TextStyle::label` and
`TextStyle::paragraph`, to render a style.

| Style        | Size | Weight   | Colour  | Use                                     |
| ------------ | ---- | -------- | ------- | --------------------------------------- |
| `HERO`       | 36   | light    | primary | One figure a page is about.             |
| `TITLE`      | 28   | semibold | primary | A name in an identity block.            |
| `HEADING`    | 18   | semibold | primary | A large value inside a chart or ring.   |
| `PAGE_TITLE` | 28   | medium   | primary | The name at the top of a page.          |
| `SECTION`    | 16   | semibold | primary | A group title above its card.           |
| `BODY`       | 14   | normal   | primary | Row titles and body text.               |
| `EMPHASIS`   | 14   | semibold | primary | A name or state inside body text.       |
| `VALUE`      | 14   | normal   | muted   | Read-only value at the right of a row.  |
| `NUMERIC`    | 14   | normal   | primary | Changing numbers, with tabular figures. |
| `CAPTION`    | 13   | normal   | muted   | Row subtitles, descriptions and hints.  |
| `BADGE`      | 11   | semibold | accent  | Short tags such as "New".               |
| `OVERLINE`   | 11   | semibold | muted   | Tracked small capitals above a block.   |
| `LINK`       | 14   | medium   | accent  | Inline links and actions.               |

`Ink::Danger` recolours any style for destructive text. Weights come from
`theme::with_font`. In the taxonomy's Type ramp semibold reads clearly heavier than
normal in Adwaita Sans; medium is only checked by eye, so re-check it if
`LINK` ever looks like normal text.

Row values are drawn with `SelectableText` and `TextStyle::span`, not a
paragraph, so a user can select them with the mouse and copy them with Ctrl+C
or a right-click Copy. This covers the `value` of `row` and the value of
`property_row`. A value stays on one line, and a long value is cut with an
ellipsis.

`TextStyle::selectable` is the one path for selectable text that wraps. It
backs the `centered_caption`, the status block, the text field's invalid
message, and the subtitle of a row or level row that does not press. A
selectable text takes the pointer-down that a press starts from, so a row with
`on_press` keeps its subtitle plain: its press keeps working, and the subtitle
cannot be selected there. While a search marks words in a text, it is a
paragraph with the marks instead, so it is not selectable until the search
clears.

## Styling with tokens

Every box is styled by data, as Tailwind classes style HTML. The style
system is in `src/style/`:

- `tokens.rs` holds the values. Each kind is its own type, so a number does
  not compile where a token is expected:
    - `Space`: `S0`, `S0_5`, `S1` to `S8`, `S10`, `S12`, with Tailwind
      numbering (`S4` is 16 px, as `p-4` is). `GUTTER` is `S4`.
    - `Radius`: `Mark`, `Control`, `Card`, `Overlay`, `Tile` and `Pill`.
    - `Dim`: sizes by role, such as `Row`, `Control`, `Button` and `Field`.
      Square boxes are named by Tailwind size step: `Box5` (20 px), `Box7`,
      `Box9`, `Box11` and `Box24` (96 px).
    - `IconSize`, `Paint` (colour roles), `Line` (borders) and `Elevation`
      (shadows).
- `mod.rs` holds the utilities. Each is one fact about a box, for example
  `px(S4)`, `gap(S2)`, `rounded(Radius::Card)`, `bg(Paint::Card)`,
  `border(Line::Hairline)`, `h(Dim::Row)`, `flex_row()`, `grow_children()`,
  `items(Align::Center)`, `surface(base, hover, press, selected)`,
  `shadow(Elevation::Raised)`, `ink(Paint::Text)` (the
  text colour children inherit) and `font(BODY)` (for a component that draws
  its own text).
- `recipes.rs` holds the recipes: named lists of utilities such as `ROW`,
  `CARD`, `GROUP`, `CONTROL`, `MENU_ITEM`, `NAV_ITEM` and `LIST_ITEM` (the
  shape of a selectable list item, shared by sidebar pages, places and file
  rows, with `LIST_EDGE` keeping the list's sides clear). It also holds the
  spacing rhythm table, which sets the space between the page title,
  groups, cards and rows.

Apply utilities and recipes with `Tw::tw`. Later utilities override earlier
ones, and padding and margin merge side by side:

```rust
use bingux_ui::prelude::*;

let tokens = use_tokens();
rect()
    .tw(&tokens, ROW)
    .tw(&tokens, &[surface(Paint::Transparent, hovering, pressing, selected)])
    .child(BODY.label(&tokens, "Wi-Fi"))
```

Text uses a `TextStyle` from `typography.rs` with an `Ink`. An icon takes
an `IconSize` and a token colour: `Paint::Muted.color(&tokens)`.

`scripts/ui-lint.py` (run by `make check`) fails when code outside the style
system does any of these:

- Calls Freya's styling methods (`padding`, `spacing`, `margin`,
  `corner_radius`, `background`, `border`, `shadow`, `font_size`,
  `font_weight`, `direction`, `content`, alignment and `overflow`).
- Reads a colour field of `Tokens`.
- Uses a `Color` or `FontWeight` value.
- Gives a box a fixed size that is not a `Dim`.

A value computed at run time can still be passed to `Size::px`, such as a
measured width, a drag offset or a chart's geometry. To add a value, add a
token or a recipe first.

### Size class and density

Sizes come in two classes, `SizeClass`. `Regular` is the mouse density of a
desktop window; `Compact` is the fingertip density. The window root
(`WindowHost`) sets the class at the start of every frame, before any child
renders:

- `Compact` when the window is narrower than `theme::NARROW_BELOW` (740 px),
  a phone.
- `Compact` when the latest pointer event was a touch. A finger on a wide
  window flips the class to `Compact`; the next mouse press or move flips it
  back to `Regular`. Pointer kind is latched from the root's global pointer
  listeners, so this is the only way the window switches density by itself.

Every size token resolves against the current class when it is applied. Each
kind has one table in `style/tokens.rs`, so tuning a size is one edit. The
values are:

| Token                               | Regular         | Compact         |
| ----------------------------------- | --------------- | --------------- |
| `Dim::Row`                          | 56 px           | 60 px           |
| `Dim::RowSubtitle`                  | 63 px           | 68 px           |
| `Dim::Control`                      | 36 px           | 44 px           |
| `Dim::Button`                       | 40 px           | 48 px           |
| `Dim::MenuItem`                     | 36 px           | 52 px           |
| `Dim::NavItem`                      | 40 px           | 56 px           |
| `Dim::Header`                       | 48 px           | 56 px           |
| `HEADER_BUTTON` (`Dim::Control`)    | 36 px           | 44 px           |
| `Dim::SwitchWidth` x `SwitchHeight` | 36 x 20 px      | 48 x 28 px      |
| `Dim::SliderThumb`                  | 16 px           | 22 px           |
| `Dim::StepperHalf`                  | 28 px           | 36 px           |
| `Dim::Field`                        | 240 px          | 168 px          |
| `Space::S3` / `S4` / `S5`           | 12 / 16 / 20 px | 14 / 18 / 22 px |
| `Space::S6` / `S8` / `S12`          | 24 / 32 / 48 px | 26 / 36 / 54 px |
| `TextSize::Sm` / `Md` / `Lg`        | 13 / 14 / 16 px | 14 / 15 / 17 px |
| `TextSize::Heading` / `Display`     | 18 / 28 px      | 19 / 30 px      |
| `IconSize::Control` / `Row`         | 16 / 18 px      | 18 / 20 px      |
| `IconSize::App`                     | 32 px           | 36 px           |

`HEADER_BUTTON` (an alias of `Dim::Control`: 36 px, 44 px on a phone) is the size of an icon button in a `HeaderBar`. It is the control height, the same token the segmented control and the split button use, so header controls line up on one height and an icon button is never taller than the tabs beside it. `HeaderBar` marks its subtree, so every `icon_button` and icon `menu_button` in `leading`, `trailing` or `title_slot` takes it with no per-app sizing; do not call `.size(..)` on a header button (`scripts/ui-lint.py` fails it). The 44 px on a phone is the touch minimum, so the hit area never falls below it. The bar is 48 px (56 px on a phone), leaving 6 px above and below the button.

**An icon button has no fill at rest.** In a header, a toolbar or anywhere else, `icon_button` and the icon `menu_button` paint nothing until the pointer is over them, they are pressed or (for a menu) open; `.filled()` and `.active(true)` are the two ways to ask for a fill, and neither is the default. A labelled `menu_button` is a control and keeps its `Paint::Control` fill. The Files main-menu button once drew that fill at rest because its icon variant took the labelled button's paint; `Snapshot::icon_buttons()` reads the fill every icon button of a headless frame paints, and `shell::rest_fill_tests` (and `header_rest_tests` in `bingux-settings`) fail for a header or toolbar button that has one.

Sizes that are not in the table are the same at both classes. The chart
geometry, menu edge and display arrangement use `Space::fixed_px()`, which
reads the regular value whatever the class, because they are constants.

The API, all in `bingux_ui::style` and the prelude:

- `SizeClass::{Regular, Compact}`, with `is_compact()` and `pick(regular,
compact)`.
- `size_class()` and `is_compact()` read the class now. Reading either while a
  component renders subscribes that component, so it re-renders when the class
  changes. `pick(regular, compact)` is the same read for a value that is not a
  token.
- `PointerKind::{Mouse, Touch}` and `pointer_kind()` give the latched pointer
  kind.
- `Space::px()`, `TextSize::px()`, `IconSize::px()` and `Dim::px()` resolve at
  the current class. A style applied with `tw` or `font()` needs no call.

The class is one `State` created once per thread, so it survives every
re-render. The root writes it before any child renders, so a frame that
changes the class is consistent: no child sees a mix of densities.

## Rules

`bingux-ui` is the shell's look in Freya. The source of truth is the shell's
own QML, not a reference app: `shell/bingux/Theme.qml`, `ControlRow.qml`,
`ControlSwitch.qml`, `ControlCentreButtonSurface.qml`, `ActionButton.qml` and
`SeekSlider.qml`. When the shell changes, change these to match.

- **Colours are the shell's `flatSurfaces` palette** (`Tokens::new`): the
  window is `popupSurface`, the sidebar `shellSurface`, group cards, dialogs
  and popovers `elevated`. Buttons, fields, dropdowns and segmented controls
  use `control`, one step lighter than `elevated`, so they stay visible on a
  card. Hover, pressed and selected are alpha layers drawn over
  any base, as `ControlCentreButtonSurface` does; selection is a neutral
  light layer. Text is `#fafafa`, secondary text `#b0b0b0`.
- **The token system is the only way to style a box.** See
  [Styling with tokens](#styling-with-tokens). A row is 56 px, or as tall as
  its subtitle needs. A control in a row (dropdown, field, segmented
  control, stepper, compact button) is 36 px. The page column is 720 px
  wide. A standalone button is 40 px and a menu item 36 px. Dropdowns and
  fields in a row share one 240 px column (`Dim::Field`), so their left
  edges line up; a field for a URL or a path uses `Dim::FieldWide` (440 px).
- **Compact density is the fingertip size class.** A window narrower than
  `NARROW_BELOW` (740 px), or one a finger has touched, resolves every size
  token to its compact value (see
  [Size class and density](#size-class-and-density)). A dropdown in a row is
  capped at `Dim::Field` (168 px) so the row's title keeps its room. A
  segmented control becomes a dropdown, since a track of segments does not fit
  beside a title. A row title wraps onto a second line instead of being cut. A
  change of class re-renders the components that read a size, in place, with
  no remount.
- **One inset.** `GUTTER` (16) is the only horizontal inset inside a card:
  row text, free content and the hairline start `GUTTER` from the card's
  edge.
- **A group is a card.** A 16 px semibold title sits above a 16 px card with a
  hairline edge, aligned with the card. Rows sit flush in it with a hairline
  between them, inset by the gutter on both sides. Every boxed surface (a group,
  the action strip, a free-content box) draws its fill, hairline and radius from
  the one `BOX` recipe, and `boxed(tokens)` builds one; do not draw a box by hand.
  A row is an optional icon, a normal-weight title, an optional muted subtitle
  that wraps to three lines and trailing content. A row with a subtitle grows to
  fit it. `subtitle_one_line()` keeps the subtitle to one line, so the row has
  its natural height, `Dim::RowSubtitle` (63 px at mouse density). A list that
  mounts rows lazily sizes its slots with that token.
- **Row hover.** Only an activatable row shows a hover fill and a pointer: one
  that navigates (a chevron), has `on_press`, or opens something. A switch,
  slider, dropdown, segmented, value-only or entry row shows neither. A switch
  row still toggles when its label is pressed, but it has no hover fill. The
  hairline above and below a row goes while an activatable row is hovered or
  pressed, or while it is selected, so its highlight reads as one block (`Row`
  reports this to its group).
- **Focus ring.** A row with `on_press` is focusable (a switch row included), and a keyboard-focused one draws the focus ring, as every control does.
- **A row's menu and sub-rows.** `Row::context_menu(items)` opens a menu of
  `menu_item`s at the pointer on a right click, as the menu button does. A
  `switch_row(title, on, handler).nested(on, rows)` reveals its sub-rows under
  the row while it is on, animated; the sub-rows sit inside
  the group's card, indented by the gutter.
- **Optimistic changes.** A control whose change is applied by a command
  (gsettings, a service, a file) calls `runtime::optimistic(shown, overlays,
previous, value, work)`. It shows `value` at once, runs `work` in the
  background and, if `work` returns `Err`, reverts `shown` to `previous` and
  toasts the error. `runtime::optimistic_then` also takes `done`, which runs on
  the UI thread after success, for a reload of a cached model. The control never
  waits on the command.
- **Blocking work runs on the executor.** `task::spawn_blocking(f)` runs `f`
  on a bounded worker pool (four to eight threads, following the CPU count) and
  returns a future a Freya `spawn` task can `.await`. `task::use_resource(key,
load)` loads a `Resource<T>` in the background: it is `Loading`, then
  `Ready(T)`, or `Error(String)` when `load` fails; `refresh()` reloads it and
  keeps the old value as `Refreshing(T)`. The load stops when its component
  unmounts. The `runtime` helpers (`background`, `background_then`,
  `use_cached`, `prefetch`, `optimistic`) use the same pool.
- **Persistent cache.** A page's last good value survives a restart.
  `runtime::load_persistent(key, load)` runs `load` and saves its `Ok` value as
  JSON at `$XDG_CACHE_HOME/<app>/<key>.json` (or under `~/.cache`). The app
  name is the program's file name. `prefetch_persistent(key, load)` at start-up
  runs that load in the background. `cached_persistent::<T, E>(key)` in a hook
  gives the state, seeded from the saved file when the state is first created.
  That is one small local read, the only file read allowed in a hook, so the
  page has its data at its first render. A missing, unreadable or
  other-format file counts as no value. Use it for a page's slow model, not for
  live state.
- **Never on the UI thread.** Do not run `Command`, `runtime::run`, a file
  read, `apps::installed()`, a gsettings or `bingux-config` call in `render`,
  a `use_hook` or `use_memo` body, or directly in an event handler. Put it in
  `use_resource`, `background` or `optimistic`, and render the previous value
  or `Loading` until it returns.
- **Controls are the shell's.** The switch is 36 x 20 with a 16 px thumb. The
  slider has a 6 px track (8 px while dragging), an accent fill and a 16 px
  white knob on a soft shadow. Buttons are filled with
  `control` and have no idle outline; no control has one. A disabled row
  (`row(..).enabled(false)`) dims and disables every control in it.
  A slider with `gradient(stops)` draws its track in those colours (8 px tall) in place of the accent fill; the knob is unchanged.
- **Segmented controls** sit on a `Control` rail with `Radius::Inner` segments.
  The selected segment is a `Card` chip with a `Knob` shadow. A usage meter's
  fill is `Accent`, not text.
- **Search fields** centre the search glyph vertically. The clear button
  acts on left pointer-down: a press on the Freya input's own handler does not
  reach a child's `on_press`. Scrollbars in the dropdown list and the popup
  auto-hide, so they are not pinned open.
- **An action row** (`action_row`) is a form's last row: a small muted
  note at the left and a compact button at the right, on the row height and
  the gutter. Put it inside a group, or under the last group on a page.
- **A message** (loading, empty, error) is a `status` row inside a group; only
  an error is tinted. A page that has no data yet shows `skeleton()`, or `skeleton_shape(&[rows per section])` when its loaded shape is known, so the final sections are in place before the data arrives.
- **Chrome.** The header bar carries the window buttons, Back and the busy
  spinner. On a desktop window the sidebar and the page are flush, split by a
  hairline, and the page header sits on the content colour. A page starts with
  its title block: `PageHead::new()` as the first child of `page_column`, which
  draws the page's name (`PAGE_TITLE`), a `PageHead::subtitle` status under it
  and a `PageHead::action` at the right. A phone draws the same title block,
  under a header that carries only Back.
  The header title is a pure function of the scroll offset (`ScrolledTitle`): it
  fades in and rises between 24 and 56 px of scroll, so it reverses exactly as
  the page scrolls back. A hero page (one whose hero already names it, such as
  About, an application's identity, a wired or VPN connection, a Bluetooth
  device or an account) starts with its hero and adds no `PageHead`.
  The sidebar is a search field, the signed-in person
  (`NavigationSplitView::profile`, which opens `Profile::target`), then rows
  under `NavItem::group` names. Every sidebar row, page, profile or search
  result, is the public `SidebarRow` component (`SidebarMark::Icon` or
  `SidebarMark::Avatar`), so they share one icon column and label inset, a
  neutral selection layer and an accent icon on the selected row.
- The window is opaque and square. Gnoblin draws the corners, border and
  shadow, so an application sets no corner radius, transparency or shadow on
  its root.
- `Application::run` loads the font; there is nothing to call. The family is
  `appearance.font_family` from `bingux-config`, else Adwaita Sans, else the
  system sans-serif; GNOME's font setting is not read. Many system fonts are
  variable. Freya only picks real weights for fonts it loads itself, and
  otherwise fakes bold, which looks blotchy, so the launch function loads the
  files.
- **Application icons** (`app_icon`, `Row::leading_app`) are looked up as
  the dock looks them up: the current GTK icon theme, the themes it
  inherits, `hicolor`, then `/usr/share/pixmaps`; an absolute path is used as
  it is. Sizes are `IconSize::AppSmall` (20, in a dropdown), `IconSize::App`
  (32, in a row) and `IconSize::AppLarge` (64, at the head of a page). An icon that is not
  found draws a tile with the generic application glyph.
- **A subpage** is a route (`.route("network/wifi")` in the app's
  [route table](#navigation)), opened with `router.push(&location)`. The header
  shows a Back button and the route's title; Alt+Left, Escape over a menu and
  the mouse Back button are the router's. A page never writes `on_back`.
- **Busy pages** call `use_page_busy(bool)` (from the prelude). While it is
  true the header shows a small spinner beside the title, with a "Working"
  tooltip. Never show a loading block in the page: it pushes the content down.
- **Page motion** runs in the split view's content column, not per page. A
  subpage slides in from the right over 320 ms, ease-out, over the page it was
  opened from. That page stays in place and dims slightly, and the new page is
  a card with a soft shadow on its leading edge, while the window's buttons
  stay at its top right. A pop slides the top page out to the right,
  revealing the page below, which undims. A second navigation during a slide
  starts from where the page is, with no snap. A sidebar switch cross-fades over
  120 ms. Motion is off under reduced motion (`tokens.motion(ms)` returns 0).
- Icons are the Bingux set under `packages/bingux-ui/icons/`; see
  [Icons](#icons) for how to use one and how to add another.
- A dropdown draws a symbolic icon before each option with `.icons(Vec<Icon>)`,
  in the same order as the options. Sound uses it for each device's kind.

## Icons

The icon set is the chat app's icon fork (`@chat/icons`, a rounded fork of
Phosphor: continuous "squircle" corners, its fill and weight choices and its own
custom icons). Icons are the fork's drawings, not redrawn: `scripts/icons/build.py`
takes the fork's outline, fits it to a 16 px grid (size, centre, whole-pixel strokes,
edges on pixel lines, see [Fitting](#fitting-the-pixel-rules)) and writes one SVG per
icon, tinted with the text colour at draw time. The Rust names (`Icon::NetworkWired`)
and the SVG file names (`network-wired-symbolic.svg`) follow Adwaita's symbolic
vocabulary, which the QML shell also uses.

```rust
use bingux_ui::icons::{Icon, icon};
use bingux_ui::style::{IconSize, Paint};

// A 16 px glyph in the muted text colour. Pass a token colour, never a literal.
icon(Icon::Search, IconSize::Control, Paint::Muted.color(&tokens))
```

| `IconSize`  | Regular / compact | Use                                         |
| ----------- | ----------------- | ------------------------------------------- |
| `Mark`      | 12 / 14 px        | Chevrons, ticks and marks inside a control  |
| `Text`      | 14 / 16 px        | A symbol in a line of text                  |
| `Control`   | 16 / 18 px        | Glyphs in buttons, fields and menu items    |
| `Row`       | 18 / 20 px        | Row and sidebar icons                       |
| `Hero`      | 48 px             | The icon of an empty or status page         |

The icons are designed on 16 px and are crispest there; 18 and 20 px are scaled
copies and are checked against a looser bound (below).

### Every fork icon is an `Icon` (`chat-icons` feature)

`bingux-ui` has the named icons above, one per use. All 1513 icons of the fork, in all
three weights, are also available as `Icon::Chat(ChatIcon::House, ChatWeight::Fill)`
behind the `chat-icons` feature (off by default: 3 MB of outlines no application
should carry). `ChatIcon::ALL` lists them; `ChatIcon::weights()` says which weights an
icon has (a missing weight draws as regular). The data is
`packages/bingux-ui/icons/chat/geometry.json` and `src/chat_icon_list.rs`, both written
by `scripts/icons/import-chat.py` from `~/dev/chat`; they hold every fork icon, not
only those in use. A catalogue icon is the fork's outline scaled by 1/16, not fitted,
so it can look softer than a named icon. `scripts/icons/review.py catalogue` renders
the whole fork, fitted, to PNG pages; `review.py candidates House:fill Mountains:regular`
shows chosen glyphs side by side; to adopt one, give it a row in `labels.tsv`.

### Fill and outline: the policy

Each icon in `scripts/icons/labels.tsv` has a category (`#policy <category>: ...`
lines) and the category decides its fork weight; `build.py --check` fails an icon
whose weight does not match.

| Category    | Weight           | What belongs there                                                                                  |
| ----------- | ---------------- | --------------------------------------------------------------------------------------------------- |
| `object`    | `fill`           | Things: devices (laptop, printer, webcam, headphones), places (folders, home, trash), tools, lock, bell, sun/moon |
| `status`    | `fill`           | Badges whose meaning is the mass: error, info, warning, help, secure/insecure, VPN, pause, eject, starred |
| `detail`    | `regular`        | Things recognised by their inner structure: globe, envelope, calendar, phone, tablet, keyboard shortcuts, text and document files, eye, touchpad |
| `section`   | `regular`        | The settings sidebar: Network, Bluetooth, Displays, Sound, Power, Keyboard, Mouse, Appearance, Desktop, Search, Date and time, Users, Accessibility, About, Applications (`DotsNine` bold: its dots are not a stroke) |
| `action`    | `regular`        | Verbs: copy, delete, share, filter, refresh, list/grid view, sidebar toggle                         |
| `indicator` | `regular`        | Levels and states: battery, volume, Bluetooth, wired network, hotspot                               |
| `control`   | `bold` (2 px)    | Chevrons, tick, plus, minus, close, menu, more, window controls, outline star, power                 |
| `drawn`     | strokes          | Drawn by hand: the Wi-Fi family (two 1 px arcs and a dot), never a fork glyph                       |

Why `regular`, not `bold`, for outlines: `bold` strokes are 1.5 px, which fitting must
make 2 px to be crisp; at 2 px the counters of a keyboard, a mouse or an ethernet
tree close up into blobs. `regular` strokes are 1 px, already whole pixels, and keep
every counter open. Simple strokes (chevron, tick, plus) have no counters and keep the
heavier `bold` as the click target. A solid slab with one small cut (a phone, a tablet,
a shield) reads worse than its outline; a lone mass (a folder, a bell) reads better
filled. Default applications: Web, Mail and Calendar are `detail` outlines; Files, Photos,
Music and Video stay `object` fills; Text and Documents have their own icons
(`default-app-text`, `default-app-documents`) so changing them never changes
`document-edit` or `document-open`.

### Fitting: the pixel rules

`build.py` fits a fork glyph with five steps, each a per-icon attribute that
`map-fork.py` writes into the source from `labels.tsv`
(`CaretRight bold scale=1.1 dx=0.25 dy=-0.1`):

1. `mirror=left|right`: keep that half, replace the other with its mirror (the fork's
   `User` fill is lopsided: its right shoulder reaches about 0.8 px further from the middle than the left, so the
   avatar uses `mirror=left`);
2. `scale` about the middle of the box (the fork leaves 1 to 2 px of margin);
3. whole-pixel strokes: a bold stroke (1.5 px times the scale) is grown to the next
   whole pixel, a regular stroke (1 px) kept at 1 px, by offsetting the outline with
   round joins (at most 0.3 px a side). `stroke=none` keeps the fork's width;
4. `dx`, `dy`: move the whole drawing so edges meet pixel lines and the ink is centred;
5. snap: every long horizontal or vertical edge and curve extreme goes to the nearest
   pixel line by a monotone piecewise-linear warp (at most half a pixel, the outermost
   edges never outwards, `snap=none` turns it off). A symmetric icon is warped on
   one half and mirrored.

Curves and corners are never redrawn. Rules, checked on the rasterised icon
(`scripts/icons/raster.py`: exact-area coverage, no hinting; the ink box is read from a
64 px raster):

| Rule | Limit | Applies to |
| ---- | ----- | ---------- |
| Ink long side, `compact` (aspect < 1.3: round, square) | 13 to 15 px (about 14) | every icon not in `#mark` |
| Ink long side, `elongated` (aspect 1.3 or more: wide or tall) | 14 to 15.5 px (the long side fills the box) | the same |
| Ink long side, `mark` | 8 to 13 px | `#mark`: chevrons, tick, plus, minus, close, more, window controls |
| Margin | at least 0.25 px on every side | every icon |
| Centre of the ink from the centre of the box | at most 0.5 px each axis | every icon |
| Soft edge pixels at 16 px | at most 10 | every icon: pixels with 15 to 85 % coverage in a straight run of 3 or more on a horizontal or vertical edge |
| Soft edge pixels at 14, 18, 20 px | at most 100, 120, 140 | every icon: scaled copies cannot be crisp, the bound catches regressions |
| Symmetry | centre within 0.05 px; mirrored coverage differs by at most 0.15 at any pixel, 0.03 on average | `#symmetric`: avatar, Wi-Fi fan, accessibility, error, warning, clear, grids, close, plus, minus, chevrons, power, brightness, display, mouse, microphone, headphones, bookmark, lock, star, laptop, phone, tablet, gear, globe |
| Counters | a fork glyph keeps every hole and cut-out | every fork icon (`#exempt counters:` to waive) |
| Sidebar set | outline weight; ink covers 10 to 30 % of the box | the icons of `groups.txt` `[sidebar]` |
| Policy | weight matches the category | every icon |

`#exempt size|centre|soft: ...` lines waive a rule where the footprint is the point:
the volume and Wi-Fi level icons must keep the footprint of the full icon, so a bare
dot or a bare cone is not centred or sized by itself.

### Checks

```sh
scripts/icons/map-fork.py --check   # sources match labels.tsv
scripts/icons/build.py --check      # sources valid, output current, every rule above;
                                    # prints the icons with the most soft edge pixels
scripts/icons/build.py --measure    # every icon: class, ink size, centre, soft pixels
scripts/icons/build.py --lint --src DIR   # the rules on a draft directory (a probe icon)
scripts/icons/build.py --suggest [icon ...]  # scale, dx, dy that satisfy the rules
make icons-check                    # the first two
```

A failing check names the icon, the number and the fix: `probe-small: ink is 4.00 x
4.00 px (compact): the long side must be 13 to 15 px; change scale`, `probe-soft: 48
soft edge pixels at 16 px (limit 10)`, `avatar-default: not left-right symmetric`.
After editing `scale`, `dx` or `dy` in `labels.tsv`, run `map-fork.py`, `build.py` and
`build.py --check`; never edit `packages/bingux-ui/icons/*.svg` (they are written).

Icons that show a state (Wi-Fi signal, battery level, volume) are one drawing per
state with the same footprint, so the glyph does not jump when the state changes:
the battery levels share one `scale`/`dx`/`dy`, as do the four volume icons.

### Choose the icon for where it is used

Pick the conventional metaphor for the label the user reads next to the icon, not
the glyph whose name matches. A sidebar "Network" page needs a network glyph, not a
node diagram; "Share" is a box with an up arrow, not a paper plane.
`scripts/icons/labels.tsv` records, for every icon, where it appears, the metaphor
people expect there, the fork glyph or drawing, and its category. Mappings that were
weak, and what was decided:

| Icon | Was | Problem | Now |
| ---- | --- | ------- | --- |
| `input-touchpad` | `HandTap` bold | a scribble at 16 px | `HandPointing` regular |
| `network-vpn` | `Shield` fill | a plain slab, same as the security icons; `ShieldChevron` read as a cup | `ShieldCheckered` fill |
| `preferences-desktop-wallpaper` | `ImageSquare` bold | frame with a diagonal; read as "no image" | `Mountains` regular |
| `view-app-grid` | solid nine-dot square | a solid blob | `DotsNine` bold (nine dots, no mass) |
| `input-tablet`, `phone` | fill | solid slab with one tiny cut | `DeviceTablet`, `DeviceMobile` regular |
| `web-browser`, `mail-unread`, `x-office-calendar` | fill | solid blobs in the Default applications list | outline (`detail`) |
| Default applications: Text | `document-edit` (`NotePencil` fill) | an editor, not a text file | `default-app-text`: `FileText` regular |
| Default applications: Documents | `document-open` (`FolderOpen`) | a folder, not a document | `default-app-documents`: `FileDoc` regular |
| `help-about` | `Info` fill, same file glyph as `dialog-information` | the sidebar is outline | `Info` regular (`dialog-information` stays filled) |
| `audio-volume-low` | `SpeakerNone` | a bare cone can read as mute | kept: 0, 1, 2 waves are monotone and mute has the X |
| `battery-level-*` | five fork glyphs for eleven levels | levels share a glyph | kept: no finer glyph exists |
| `battery-level-*-charging` | `BatteryCharging` | the bolt is small at 16 px, level is lost | kept: the fork has no level-and-bolt glyph |
| `window-restore` | `CopySimple` | close to `edit-copy` | kept: two squares are the convention |
| `find-location` | `CursorClick` | no "inspect" glyph | kept for the inspector toolbar |
| `network-wired` | `Network` | a node tree, not a port | kept: the fork has no ethernet port |

### Add or change an icon

1. Find the fork glyph: `review.py catalogue` (all of them), or
   `~/dev/chat/packages/icons/src/generated/<Name>.ts`.
2. Add a row to `scripts/icons/labels.tsv` (`file | where it appears | metaphor |
   <ForkName> <weight>`) and name it in one `#policy` line, plus `#symmetric` or
   `#mark` where they apply. For an icon the fork does not have, mark the row `custom`
   and draw `src/<file>-symbolic.svg` by hand from plain SVG shapes on the 16 grid (see
   the header of `scripts/icons/build.py`: `stroke` 1 or 2, `fill`, `data-op="cut"`,
   `rx` for a continuous corner, and `<fork/>` pieces), category `drawn` or the
   category of the fork piece.
3. `scripts/icons/import-chat.py` vendors the whole fork (it only needs to run when the
   chat package changes).
4. `scripts/icons/build.py --suggest <file>` prints `scale`, `dx` and `dy`; put them in
   the row. `scripts/icons/map-fork.py` writes the source, `scripts/icons/build.py`
   compiles it into `packages/bingux-ui/icons/` and the shell's copy,
   `scripts/icons/build.py --check` judges it.
5. Add the variant to `Icon`, to `Icon::ALL` and to `Icon::bytes()` in
   `packages/bingux-ui/src/icons.rs`.
6. Look at it. `scripts/icons/review.py sheets --out /tmp/icons` writes one labelled PNG
   per group (Icon variant, file, category, glyph and fit, where it is used, metaphor;
   16 px dark and light, 18 px, 48 px) and `icons.txt`, the same table as plain text.
   `review.py compare BEFORE_DIR AFTER_DIR --group sidebar` shows a set before and
   after. `scripts/icons/sheets.py --out /tmp/icons` renders the same at 12 to 48 px
   with Freya's own rasteriser, beside the Adwaita originals, plus a top-bar mock.

The fork's geometry is MIT-licensed Phosphor work; `packages/bingux-ui/icons/COPYING`
carries the licence and attribution.

### In the QML shell

`scripts/icons/build.py` also writes `shell/bingux/icons/` (the same icons with
Adwaita's dark fill, which the shell's `SymbolicIcon` recolours) and
`shell/bingux/BinguxIconIndex.js` listing their names. The shell still resolves
icon names through `AdwaitaIcons.qml`, so nothing changes in the shell until it is
pointed at them: in `AdwaitaIcons.qml`, `import "BinguxIconIndex.js" as BinguxIcons`
and, after `canonical` is computed, return
`Qt.resolvedUrl("icons/" + canonical + ".svg")` when `BinguxIcons.names[canonical]`
is set. Names the shell asks for that are not in the set still fall through to the
Adwaita theme.

## Accessibility and contrast

Every colour pair Bingux UI draws is held to WCAG 2.2 and measured by code, not
by eye. The levels used:

| Pair | Ratio | WCAG |
|---|---|---|
| Text under 24 px (under 18.66 px bold) | 4.5:1 | 1.4.3 AA |
| Large text (24 px and up, or 18.66 px bold) | 3:1 | 1.4.3 AA |
| Icons, focus and error rings, control edges and tracks, state fills (accent, success, warning, danger) | 3:1 | 1.4.11 AA |
| Ink on a filled surface | 7:1 where the hue allows, else 4.5:1 | 1.4.6 AAA / 1.4.3 AA |
| Disabled controls | 3:1 target, exempt | 1.4.3 and 1.4.11 exempt inactive controls; the ratio is still measured and reported |
| Hairlines between rows | exempt | decorative; the rows are named by their text |

APCA (the WCAG 3 candidate) is reported beside every ratio as `apca`, signed Lc.
It is advisory: it never fails a pair.

### The rule: ink on a coloured surface is a darkened version of it

Text and icons on the accent, on a suggested (primary) button, on a
destructive button, and on success, warning and error fills are never plain
white. Use the on-colour token of the fill:

| Fill | Text and icons | Ink | Paint |
|---|---|---|---|
| `Paint::Accent` (suggested, selected, flame cell) | `Ink::OnAccent` | `tokens.on_accent` | `Paint::OnAccent` |
| `Paint::Danger` (destructive) | `Ink::OnDanger` | `tokens.on_danger` | `Paint::OnDanger` |
| `Paint::Success` | `Ink::OnSuccess` | `tokens.on_success` | `Paint::OnSuccess` |
| `Paint::Warning` | `Ink::OnWarning` | `tokens.on_warning` | `Paint::OnWarning` |
| `Paint::Floating` (tooltip, toast, status line) | `Ink::OnFloating` | `tokens.on_floating` | `Paint::OnFloating` |

`Paint::on()` maps a fill taken from data (a tree badge) to its on-paint.
`scripts/ui-lint.py` fails a plain `Text` or `Muted` ink drawn near
`bg(Paint::Accent | Danger | Success | Warning | Floating)`.

`bingux_ui::contrast::on_colour(bg)` (re-exported as `theme::on_colour`)
derives the ink. It converts the surface to OKLCH, keeps the hue and about half
the chroma, and binary-searches lightness for the lightest shade that reaches
7:1 against the surface (AAA), else 4.5:1 (AA). So a light blue fill gets a
deep navy, a salmon fill a dark brown-red, and it follows the user's accent.
When even near-black misses AA (a dark fill) it falls back to a tinted
near-black or near-white, whichever has the higher ratio.
`on_colour_over(&[states])` reaches the target against several colours at once;
`Tokens::new` uses it with the fill at rest and under the hover, press and
selection layers, so the ink holds in every state. If no ink can hold in all
states the fill itself moves the least in lightness that makes it so
(`fit_fill`), keeping its hue.

The tokens are also adjusted centrally so the pairs hold, keeping hue and
moving only lightness (`contrast::ensure_contrast`):

- `accent`, `success`, `warning`, `danger` show 3:1 on every layer (they are
  fills, marks and chart series). The Appearance swatches keep the
  unadjusted colours.
- `Ink::Accent`, `Ink::Danger`, `Ink::Success`, `Ink::Warning` are the same
  hues at 4.5:1 on every layer and on their own tints (badges). Use
  them for text and links; `Paint::Accent` etc. are for fills and marks.
  A banner or toast is not drawn with these: its text is the hue darkened
  (lightened on a dark scheme) against its own tinted fill, `Ink::Notice(kind)`
  (see Banners and toasts).
- `muted`, the code face colours and `outline` are moved to hold 4.5:1 (3:1
  for `outline`) on every layer and every hover, press and selection state.
- `focus_ring` / `error_ring` (`Paint::FocusRing`, `Paint::ErrorRing`) show 3:1
  on every layer including the raised control fill.

### The matrix

`bingux_ui::contrast_matrix` declares every pair as data in `pairs()`: the
foreground (`Fg::Ink` or `Fg::Paint`), the background (`Bg::layer`,
`Bg::over(fill, under)` with an optional `State` hover, pressed or selected),
the `Requirement` (`Text`, `LargeText`, `Component`, `Enhanced`), an `opacity`
for a dimmed control and an `exempt` reason. `evaluate(&tokens)` measures them,
`evaluate_all()` does so for light and dark and every accent the Appearance page
offers, and `table(..)` prints a readable table with the WCAG ratio, what the
pair needs, APCA and the verdict. The Taxonomy app's Foundations > Colour page
shows the same rows with pass and fail badges, and each Button, Badge and Chip
specimen shows its measured ratio.

To add a pair, add one line to `pairs()`:

```rust
out.push(pair("what it is", Fg::Ink(Ink::Accent), Bg::over(Paint::AccentSoft, Paint::Card), Requirement::Text));
```

Set `exempt` only when WCAG itself exempts it (inactive controls, decoration),
with the reason. Run the test:

```sh
cargo test --lib --features testing contrast   # in packages/bingux-ui
cargo test --lib --features testing every_token_pair -- --nocapture   # prints the totals
```

A failing pair prints the table of every pair under the requirement. Fix it in
the token (`theme.rs`), not in the component.

### The inspector's Contrast check

The Accessibility tab's Contrast group measures the running app. For each label
and paragraph it resolves what is on screen: the node's text colour with its own
alpha and every ancestor's opacity applied, over the background composited
from the nearest opaque fill down through each translucent one. A node under
AA is listed with the ratio, what it needs (4.5:1, or 3:1 for large text), the
APCA reading (advisory), the two colours as chips, and a click that selects the
node in the Elements tab. Open it with Ctrl+Alt+Shift+I, then the Accessibility
tab. Icons are drawn from SVG, so the check covers text only; the matrix covers
icons.

### Text must be readable: overflow, wrapping and stress

Unreadable text is an accessibility defect (WCAG 1.4.4 resize text, 1.4.10
reflow, 1.4.12 text spacing), and it is the one that shows only with a longer
label, a narrower window or a bigger text size than the developer had. A form
row whose label was squeezed into a column one letter wide, wrapped a letter to
a line, grew over the row below and covered a checkbox's name is what these
checks exist for. Validate every change against them.

**The rule for a label and its control** (`controls::adaptive`, used by
`form_row` and `form_field`): a label keeps its *floor*, its natural one-line
width or 40 % of the line when that is less and never less than its longest
word. The control gets the rest. When the control's natural width would take
the label's floor, a text field or dropdown shrinks (to at least
`Dim::FieldMin`, 120 px) and the label keeps its width; when even that leaves
the label less than its floor, the line stacks, the label above the control.
Text breaks between words only. `choose_line` is the rule as a pure function
of widths, with tests. A control that can shrink reads `use_line_fill()`; a
new control that takes a width in a form line should do the same. Do not put a
field inside a fixed-width wrapper: the wrapper does not shrink.

**A text field in a row on a phone** (`row(title).trailing(text_field(..))`,
`text_row`, `entry_row`) is not left at its 168 px (`Dim::Field` compact). At
compact density it fills the row: the title keeps its natural width, at most
40 % of the line (`LABEL_SHARE`), and the field takes every pixel after it,
at least `Dim::FieldMin`; a title with a word wider than its share, or a line
too narrow to leave the field its least, puts the field under the title at
the full width. `choose_filled_line` is that rule as a pure function, next to
`choose_line`, and the `Row` tells the field to fill through the same
`LineFit` a form line uses (`use_line_fill()`). On a desktop the field keeps
`Dim::Field`. Geometry tests at 390 and 320 px are in `rows::phone_fields`.

**The checks.** The inspector's Layout tab lists them (they are pure
functions over the measured tree, `inspector/layout_checks.rs`, each with
tests on hand-made trees):

| Rule | Finds |
|---|---|
| Broken text (`wrapped`) | a letter to a line (a line of 2 characters or fewer before the last, in text over 8 characters), a box narrower than two glyphs, a word split in the middle with no ellipsis, more lines than `max_lines` |
| Text overflow (`overflow`) | one-line text wider than its box with no ellipsis |
| Clipped (`clipped`) | ink cut off by an ancestor that clips |
| Outside its parent (`outside-parent`) | a box that reaches out of the box that holds it by more than 2 px (overlays, `position: absolute`, are left out) |
| Overlap (`overlap`), Text collision (`text-collision`) | siblings, or texts of different parents, lying on each other beyond 2 px (not across layers) |
| Sideways scroll (`horizontal-scroll`) | content wider than the window or than its scroll view |
| Truncated (`truncated`) | text cut off while its container has room on the same line |
| Hidden label (`hidden-label`) | a control none of whose text shows (hidden, collapsed, or under half of it inside the box that clips it) |

They are listed in the Layout tab, not the Accessibility tab: that tab is
about assistive technology and contrast; these findings are about what a
person sees, and the tab already shows the node, the numbers and the fix.

**Stress.** `python3 scripts/overflow-check.py` renders every Settings page,
subpage and popup and every taxonomy specimen under the conditions that break
layouts and fails on any finding the script's `ALLOWED` list does not justify:

- `BINGUX_PSEUDO_LOCALE=expand`: every string made through the type ramp
  (`TextStyle::label`, `paragraph`, `span`) is rewritten to an accented one at
  least 40 % longer, `Network name` becoming `[Ńéţẃöŕķ ńáḿé ~~~~~]`, and
  measured the same way. Debug builds only (`bingux_ui::stress`).
- `BINGUX_TEXT_SCALE=2`: the type ramp at 200 %, the largest text scale the
  Accessibility page offers.
- `BINGUX_STRESS_DATA=long`: names that come from the machine (Wi-Fi, Bluetooth,
  wired profiles, displays) get a 64-character unbroken tail. Wrap a new fake
  name in `bingux_ui::stress::data(..)`.
- Narrow windows: 390x800, 320x600, and every popup at 320 px, the least a popup
  can be dragged to (`BINGUX_POPUP_WIDTH=320`).
- Right-to-left is not run. The layout engine has no text direction to
  mirror; the script says so.

The taxonomy header has the same two toggles, pseudo-locale and text scale, so a
specimen can be looked at under them. `--list` shows the scenarios,
`--scenarios pseudo`, `--only settings --pages network` and `--keep DIR` (to
keep the PNGs of the findings) narrow a run.

## Motion

`motion.rs` holds the animated building blocks. Use them instead of hand-rolled
animation so motion never shifts the layout around it. Each keeps layout stable:

- `reveal(open, child)` is the one primitive for every conditional row or
  section: it animates the height of a clipped box from 0 to the content's
  measured height and back with the standard curve (quad, ease out, 160 ms), so
  the content below moves continuously and never snaps. The child fades in over
  the second half and settles `REVEAL_LIFT` (6 px) down into place. It starts at
  its target, so a reveal that mounts closed or open does not animate on the
  first frame. The height is measured with `on_sized`, so async rows and wrapped
  text still animate to the right height; a settled open reveal is auto-sized.
  Reversing mid-motion continues from the current height. The measured content
  stays mounted while closed; chain `.lazy()` to unmount it once settled closed
  (nothing hidden can take focus) and mount it again on open.
- `reveal_block(open, gap, child)` is `reveal` for a whole group in a column spaced `gap` apart: it grows with the gap it brings, and closed it takes no gap. It has `.lazy()` too.
- `reveal_if(Option<Element>)` and `reveal_block_if(gap, Option<Element>)` replace
  `.maybe_child(..)` for a banner, error line or section whose presence follows
  state: the child grows in while it is `Some`, is kept for the closing motion
  after it becomes `None`, then is unmounted. They are always in the column
  (empty and gapless while hidden), so siblings keep their place. Use them, not
  `maybe_child`, for anything that appears or disappears on a state change; a
  child that exists for the whole life of the page stays a plain `maybe_child`.
  `pending_bar`, the form error line, the Wi-Fi join phase line, the password
  and date/time banners and the VPN error lines are built this way.
- `expander_row`, `Row::nested` (the MAC address field under its choice) and
  `TreeView` branches use the same height motion: the row's chevron rotates in
  step, and a tree row that leaves the list stays, shrinking, until its motion
  ends (`controls/tree_motion.rs` holds that timeline with an injected clock).
- `presence(open, child)` fades and scales in place. It keeps its space until
  the exit ends, then releases it. Chain `.collapse()` to animate that space
  away too, as a `reveal` does.
- `crossfade(child)` swaps the child with a fade and holds the larger of the old
  and new sizes during the swap, so the container never shrinks mid-swap. When
  the new child is smaller, the box settles to it once the swap ends. Wrap the
  child in `reveal` if that step must animate.
- `use_tween(target, ms)` and `animated_number(value)` change values, not the
  size of the box they sit in. Give numbers a fixed width if neighbours must not
  move. `use_tween` starts at its target, retargets from the value on screen
  when the target changes (an interrupted motion never snaps), and jumps at 0 ms.

Durations go through `tokens.motion(MOVE_MS)`, so reduced motion makes every
motion an instant change. `use_presence` is the hook under `presence`; overlays,
menus and dropdowns use it directly.

Motion that follows a pointer is not a timed tween. `drag.rs` holds the one pointer-capture drag (`use_pointer_drag`: the press, every move and the release wherever the pointer is, Escape to cancel; the position goes into a cell, so a move re-renders nothing) and the frame clock (`use_frames`: a loop of about 16 ms that stops when its work is done, so a component writes one state a frame). `reorder.rs` holds the spring (`spring`, critically damped, `OMEGA` 38, about 120 ms and no overshoot, exact for any frame length, so a stalled frame cannot blow it up) and the arithmetic of carrying an item through a list. A spring, unlike a tween, takes a new target mid-flight and keeps its speed, which is what a row does when the pointer changes its mind. Reduced motion skips the loop: every target is reached in the call that set it.

### Keyboard focus

Every interactive control is a keyboard stop by default, and apps never opt in.
Buttons, icon buttons, switches, checkboxes, segments, dropdowns, sliders, text
fields, press rows, sidebar items, tabs, menu items, links and tree rows all take
focus themselves. A box an app builds from scratch (a swatch or a card) becomes a
stop through `focus_stop(content, label, role, enabled, radius, on_activate)` in
`controls/focus_stop.rs`, which gives it the same stop, ring, press and Enter/Space.
Non-interactive boxes are never stops.

Each control is exactly one Tab stop. Tab moves forward one stop and Shift+Tab
back one, in visual order, wrapping at the ends of the window. The window's header buttons are stops too, after the last page row and before the first, so Tab reaches them before it wraps. A composite control
(segmented, tab strip, radio group, tree, list) is one stop, and the arrow keys move
inside it (roving). A pointer click focuses without a ring, and the next Tab
continues from the clicked control.

Controls draw their focus ring by ending `render` with
`.focusable(focus())`, from the `Focusable` trait in `motion.rs`. The
node passed in is the control's **face**: the node that carries its fill and
its corner radii. `focusable` wraps the face in a row that hugs it (no
padding, border or margin of its own), measures that row with `on_sized` after
layout, and draws the ring from the measured box, so the ring is always the
painted box and never the padded content of a wrapper. A face must therefore
carry no margin: put the margin on the returned wrapper. Give the wrapper any
grow or fill style the control had (for example `w_full()` or `w_flex()`). For
a pane on a clipped container edge, such as the window's, use
`.focusable_inside(focus())`: the ring is drawn just inside the face's box, with
the face's own radii, so the clip cannot cut it off (the Files list does this,
and `FileView::corners` says which of its corners sit in the window's rounded
corners, so it paints `Radius::Window` there and is square elsewhere).
`.focusable_invalid(focus(), invalid)` draws the error ring.

**The ring's corners are the face's, read from it.** `focusable` reads the four
corner radii the face is painted with (`Radii`: top left, top right, bottom
right, bottom left) and gives each ring corner that radius plus the outset
(the nested radius rule), or zero where the face's corner is square. There is no
radius argument and nothing to keep in step: a square face (a flat field's line,
a list pane, a table header, a chart area, a row in a card) has a square ring;
a segment with two square corners, a tab (`Corners::Top`) or the primary half
of a split button (`Corners::Left`) has a ring that is square at exactly those
corners. A face that should have a rounded ring paints a radius
(`rounded(..)`, `rounded_on(radius, Corners::.., 0.)`); `focus_stop` paints its
`radius` argument on the box it is given.

The ring is one crisp 2 px line (`FOCUS_WIDTH`, the `Ring` dimension) set
`FOCUS_GAP` (2 px) off the face, so a gap of the surface shows between the face
and the ring and the ring never merges with a coloured fill (a suggested or
destructive button, a selected sidebar item). Its colour is `Paint::FocusRing`, which
shows 3:1 against every layer (see the contrast matrix). An invalid control
draws the same ring in `Paint::ErrorRing` always, not only on focus; while it
also has keyboard focus a second band, `Paint::ErrorHalo` (the error ring at
34 %), arrives just outside it, so focused and invalid read differently from
invalid alone.

**Focus changes blur in and out.** One progress value (0 gone, 1 settled)
drives the ring's whole look (`RingLook::at`): when focus arrives the ring
starts `FOCUS_TRAVEL` (4 px) outside its place, blurred (`FOCUS_BLUR`, 4 px of
gaussian blur through `glow(..)`, the shadow primitive that `lift_shadow` uses)
and transparent, and sharpens, comes in and fades in over `FOCUS_IN_MS`; when
focus leaves it softens to 4 px of blur, grows back out and fades out over
`FOCUS_OUT_MS`, quicker so the ring never lingers. Moving focus from one
control to another cross-fades: the old ring blurs out while the new one blurs
in, each animating only its own few nodes. A change part-way continues from the
value on screen and never snaps. Reduced motion makes every change instant, and
a window that loses focus loses its keyboard focus, so its ring blurs out too.
Pointer focus draws no ring. The Taxonomy app's Foundations > Motion page shows
the ring on each kind of face, square, rounded and mixed.

`scripts/focus-ring-check.py` checks every ring on every Settings page and every
Taxonomy page, at the desktop and the phone size: the ring's rectangle must be the
face's border box grown by the outset and each of its four corner radii the
face's painted radius there plus the outset, or zero where the face's corner is
square. A rounded ring around a square face and a square ring around a rounded
face both fail; there are no exemptions for faces that draw nothing or for rings
inside a row. The face is the node `focusable` was given, or the first box of the
same size inside it that draws (a text field's wrapper carries the radius, so the
input inside it must agree). The inspector's Layout tab reports a ring that is
off its face as "Focus ring off its face". `scripts/focus-ring-sheet.py` renders
the ring on each kind of control in light and dark, at both widths, into one
contact sheet.

Window focus lives in `focus.rs`. The window host and the overlay host share one
`FocusCore` (whichever is outermost creates it, with `provide_focus_core()`), so
the page and the menus, dialogs and sheets the overlay host draws beside it see
the same focus state. F6 moves between focus regions (the sidebar, the content,
the header), and a region draws its ring around the whole area, not around one
control. Region sentinels are never Tab stops: Tab skips them without spending a
press. Clicking a control claims the press, so a pointer press never starts a
ring.

A modal layer is a **scope**: dialogs, framed dialogs (the Files quick preview and
properties), alerts and popup windows wrap their card in `trap_inline()` (or
`trap()` for a popup that fills its parent). Tab and Shift+Tab cycle inside the
scope, through two zero-size sentinels that pass the focus straight on, so every
press lands on a control with a ring; the card itself is not a Tab stop. The
page behind is inert: its rings are not drawn, the inspector marks it `inert`,
and Escape returns the focus to the control that opened the scope. A dropdown
list and a context menu are not scopes: Tab closes them and moves on from their
owner, and Escape or a press outside gives the owner its focus back. A press
outside a menu or list is the shield's, not the page's: the shield claims it
(`claim_press`), so the page does not take the press as empty space, and a text
field that owns the menu keeps its focus through the release (`Input::keep_focus_when`).
Use `use_focus_core()` to reach the core: with no host (the snapshot runner, an
inspector window) it returns a detached core, so focus moves do nothing and
controls still work.
`step_focus(back)` moves focus to the next or previous stop as Tab does, with no
host needed. A list answers its Up and Down keys with it, so the arrows walk its
rows (the inspector's Accessibility list does).

## Taxonomy app

`bingux-taxonomy` (`packages/bingux-taxonomy`) shows every component, element,
widget, icon and token of bingux-ui running, in a real Bingux window. Open it
with `bingux-taxonomy`, or at a page with `bingux-taxonomy controls/buttons`:

```sh
CARGO_TARGET_DIR=/var/tmp/bingux-settings-target \
    cargo run --manifest-path packages/bingux-taxonomy/Cargo.toml
```

- The sidebar is the taxonomy: Foundations (colour, type, space, radii,
  elevation, layers, density, recipes, motion), Icons (every file in `icons/`,
  searchable), Controls, Containers, Navigation, Data, Files and Inspector.
- Each entry shows its Rust API in one line, a copy button for its anchor in
  this file, and every state as a real live component: Desktop and Phone
  density side by side, in the window's theme or in light and dark together.
  A state tagged `live` reacts to the pointer and the keyboard (hover it, press
  it, Tab to it); a state tagged `forced` is held by its props (disabled, error,
  loading, selected, empty, long text) so it is always on screen.
- The header switcher changes the whole window, not a mock: theme (System,
  Light, Dark, Both), accent, density (Auto, Desktop, Phone) and display scale.
  `BINGUX_TAXONOMY_THEME`, `_ACCENT`, `_DENSITY` and `_SCALE` set the start.
- Every specimen sits in a `Stage` that gives it its own tokens and density
  (`WindowDensity` forced to Regular or Compact) and an `InertControls`
  marker, so a specimen window's buttons never act on the real window.

`bingux-taxonomy --snapshot DIR [PAGE,...]` renders every page headlessly,
through `bingux_ui::snapshot`, with no compositor. Per page it writes
`<category>_<page>.png` (desktop width, as tall as the page),
`<category>_<page>.phone.png` (390 x 800) and one
`<category>_<page>.<entry>-<state>.png` for each state whose spec has a script
(a menu open, a dialog up, a chooser open). `BINGUX_SNAPSHOT_SCALE=1.5` renders
at another scale and the `BINGUX_TAXONOMY_*` variables at another theme or
density, so two runs can be diffed. Use `Snapshot::new(app, size)` with
`settle`, `click`, `hover` and `save` to script your own captures; Bingux
Settings exposes the same renderer as `bingux-settings --snapshot DIR` (see
[settings](settings.md)).

### Add a taxonomy entry

Every change to bingux-ui lands with its entry. In five steps:

1. Write the states as a `const` list of `Spec`s in
   `packages/bingux-taxonomy/src/demos/<area>.rs`: `Spec::live(label, || ..)`
   for the real component, `Spec::forced(label, || ..)` for a state its props
   hold. The closure builds an `Element` and may use hooks. `.wide()` or
   `.page(height)` give it room, and `.script("click-text Open; wait 300; save
   {out}")` adds a snapshot of a state a still image cannot reach.
2. Add an `Entry` to `ENTRIES` in `src/registry.rs`: its name, category and
   sidebar page, the API in one line, the `doc` anchor (a heading of this
   file), `covers` (the Rust names it shows) and its `body`.
3. Name every public item you added in some entry's `covers`. An internal with
   nothing to look at goes in `coverage-allowlist.txt` with its reason. A new
   icon needs nothing: the Icons page lists the `icons/` directory.
4. Run `python3 scripts/taxonomy-check.py` and `cargo test --manifest-path
   packages/bingux-taxonomy/Cargo.toml`. They fail on an exported item with no
   entry, a `covers` name bingux-ui no longer exports, an `Icon` variant not in
   `Icon::ALL`, and a doc anchor that is not a heading. `make check` runs both.
5. Look at it: `bingux-taxonomy --snapshot /tmp/taxonomy <category>/<page>`, or
   run the app and use the switcher. Report the desktop and the 390x800
   screenshot paths, or what you saw, in the change description.

### Layout checks

The inspector's Layout tab (beside Elements, Performance and Accessibility) lists visual defects that are not accessibility problems: uneven padding (a box with the same padding on both sides of an axis whose content sits nearer one side, measured as the ink bounding box against the padding box), an item off the centre line of a row that centres its items, siblings of one component with different heights in a row, a box with a fixed height and more than 32 px empty under its content, ink clipped by a box that clips, partly overlapping controls, one-line text cut off while its container has room, and unaligned baselines of same-size texts in a row. Each finding shows the rule, the measured numbers and how to fix it; pressing it selects the node in Elements and outlines it in the app. The rules are pure functions over the inspector's snapshot (`inspector/layout_checks.rs`, tested on hand-made trees). The touch-target minimum on a phone stays an Accessibility check.

`python3 scripts/layout-lint.py` runs the same rules headlessly (no compositor) over every Settings page and subpage, the Settings popups that open from a click, and every taxonomy page, at desktop size and 390x800, and exits 1 on a finding. `--build` builds the binaries first; `--only settings|taxonomy`, `--pages a,b`, `--sizes 390x800` narrow it. Fix the component or token. Where a box is lopsided on purpose, add an entry to `ALLOWED` in the script with the reason. With `BINGUX_UI_LAYOUT_LINT=FILE` any `bingux_ui::snapshot` run appends the findings of each saved capture to FILE.

## Inspector

Every bingux-ui app includes a devtools window, modelled on the Chrome and
WebKit inspectors. Press Ctrl+Alt+Shift+I to open or close it. It opens as a
separate native window, and the app needs no code for it: the inspector is a
plugin that `Application::run` installs in the one launch function, so no
window can start without it (see Application loop; `scripts/ui-lint.py`
rejects any other way to start one). The plugin sees a key press before the
window's content does, in whichever window has keyboard focus (the main
window, another window, an inspector), so a focused text field cannot swallow
the shortcut. While no inspector is open the plugin captures, times and
samples nothing.

### Targets: the inspector can be inspected

Each inspector is bound to one **target** window. Its Elements tree, inspect
mode, Properties, Layout, Computed, Performance, Accessibility and phone
preview all act on that window and no other, and its window is titled with it:
`Inspector: Settings`.

- Ctrl+Alt+Shift+I opens an inspector over the window that has focus, or closes
  the one that is open. Pressed in an inspector's window it opens an inspector
  *of that inspector* (`Inspector: Inspector: Settings`), whose Elements tree
  shows the first inspector's elements, not the app's. And so on, each
  targeting the window it was opened from. Nesting stops at 4 deep; one more
  shows "Inspectors nest at most 4 deep." as a toast in that window and does nothing
  else.
- The shortcut is matched on the physical key (`KeyI`) or the character, with
  Ctrl, Alt and Shift all held in any order; lock keys are ignored and a held
  key toggles once. If a desktop does not deliver it, the inspector header has
  an "Inspect this inspector" button, and `BINGUX_INSPECT_KEYLOG=1` prints every
  key event the plugin sees (`[inspector-key] window … key=… code=… mods=…`) to
  stderr. A new inspector window asks for keyboard focus when it opens and when
  the pointer enters it.
- Inspect mode (the button, or Ctrl+Shift+C) in an inspector picks in its own
  target: the second inspector's picks and hover boxes land in the first
  inspector's window. Ctrl+Shift+C pressed in an inspector's window is that
  inspector's own inspect button; in any other window it turns on inspect mode
  of the inspector over it, opening one if there is none. Escape cancels in
  either window.
- An inspector never watches itself: it takes snapshots of its target only, and
  a window is a target only once an inspector has been opened over it.
- A target that redraws every frame is copied at most ten times a second, and
  the inspector asks for one more layout when a copy was skipped so it catches
  up. Refresh (F5) is never held back.
- Closing a window closes the inspectors over it, and those over them, so
  closing the app ends the process. Closing an inspector leaves its target
  alone, and releases a phone preview it had set.
- Phone preview changes the target window only, and the Performance tab times
  only the target's frames. One recording runs at a time: a second inspector
  asking to record says another inspector is recording.
- The logic (binding, no self-target, depth limit, close propagation, the
  throttle, the title) is `inspector/target.rs`, tested without a window
  system.

The header is the shell's header bar: the inspect button (Ctrl+Shift+C) and
the phone preview toggle at the left, the Elements, Performance and
Accessibility tabs as a segmented rail in the middle (Ctrl+[ and Ctrl+] step
between them, Ctrl+1 to Ctrl+3 jump), and pause, refresh (F5) and the window
controls at the right. Under 760 px the tabs scroll and the tree and
Properties stack. Before the first snapshot the window says it is waiting for
an application window and shows a spinner; the first tree arrives as soon as
the app has laid out.

Failures the runtime caught are visible here (see "Panics are never fatal"). An
element that is an error card shows a red `failed` chip in the Elements tree,
and its label chip carries the message. The Performance tab has an **Errors**
card, absent while nothing has failed, listing the last eight failures newest
first (kind, component, how long ago, the first line of the message, the
component path and the panic location, `×N` when it repeated) with Copy report
and Clear. The same ring buffer is `resilience::recent_errors()`.

An app component can publish its call for the Properties pane:

```rust
rect()
    .component("Row")
    .published(|| {
        Published::call("row")
            .arg(Prop::text("title", title))
            .maybe_method(subtitle.map(|text| Prop::text("subtitle", text)))
    })
```

`Prop::text`, `flag`, `number` and `choice` say what kind of value a prop is,
which picks its editor; `.live()` plus `prop_override(a11y_id, "name")` in the
component lets the inspector change it while the app runs. `property_row` is
the dense row (name, hint, editor, reset) the pane is built from, and
`style::catalogue` lists every token by name for tools like this one.

- **Inspect mode** highlights the element under the pointer in the app with
  content, padding and margin bands and a card naming its tag, size, colour,
  font, padding, margin and accessible name, role and focusability. A click
  selects the element and reveals it in the tree; the click never reaches the
  app. Hold Shift to keep inspecting; Escape cancels.
- **Phone preview** forces the target window to the compact size class and touch
  pointer, so it shows the phone layout on a desktop; the inspector itself
  keeps its desktop density.
- **Elements** shows the live element tree as an outline, one line per element,
  indented by depth. A line is the element's name, then its text in quotes, then
  muted chips: size as `W×H` (the border box the layout measured, margin
  excluded), layout and style values that differ from the
  defaults, `hidden` or `selected`, a `label` when the accessible name differs
  from the text, the accessibility role, and the element kind. The name is the
  bingux-ui component that built the element (`Row`, `Switch`, `PageHead`), else
  its accessibility role, else its layout role. A closed branch shows its child
  count. Click a triangle to open a branch, and Alt-click a triangle to open or
  shut the whole subtree under it. The right-click menu's Expand recursively
  does the same. Up and Down move the selection, Right opens or enters a
  branch, Left closes it or goes to the
  parent, Home and End jump to the ends. Ctrl+F searches names, text, labels and
  chips, with the hit count and Enter for the next hit. A right click offers
  Expand recursively, Collapse children, Copy text and Copy element; Copy element
  copies the outline format. The breadcrumb under the tree shows the selected
  element's ancestors by name; click one to select it. Hovering a row highlights
  its element in the app.
- **Properties**, beside the tree, shows the selected element in bingux-ui's
  own terms, not as CSS. Under it, three tabs: Properties, Layout and Computed.
  - *Component* shows the call that built a component that publishes one, such
    as `row("Wired connection 1")` or `Group::titled("VPN")`, with each
    argument as a field. Text arguments edit live; a prop edits live when the
    component opts in (the `Switch`'s `on` does), and is otherwise shown with
    "set in code".
  - The groups are named as the Rust API names them: *Layout* (`flex`, `items`,
    `justify`, `gap`, `clip`), *Spacing* (`p`, `px`, `py`, `m`, `mx`, `my`),
    *Size* (`w`, `h`, `min_w`, `max_w`, ...), *Paint* (`bg`, `rounded`,
    `shadow`, `border`, `opacity`), *Text style* (`font`, `ink`, `text`) and
    *Accessibility* (`name`, `focusable`, and the role).
  - Each row has the property's name, a hint line with the call it stands for
    and the value it resolves to (`gap(S3) = 12 px`), and an editor. Space,
    size, radius, colour, elevation, border, alignment and type style are
    dropdowns of the tokens, listing each token's value at the density the app
    window renders at (a phone preview shows the compact values); colours show
    a chip. A value on no token is a raw value: pick Raw value, type a number
    or `#rrggbb`, and the hint says "not a token". Numbers step with Up and
    Down. Enter or a pick applies the edit to the running app; an edited row
    shows its name in the accent and a reset button, and the header has Reset
    all.
  - Copy as Rust copies the element as bingux-ui code: the published call, or
    `rect().tw(&tokens, &[...])` with the utilities that are not defaults.
  - Layout draws the box as margin, padding and content bands, each side named
    by its space token, above the same spacing rows. The bands are the margin,
    padding and content size the layout used. Every pane scrolls on its own.
  - Computed lists the values the layout engine *used* for the element right
    now, under a filter, and follows the element live (each new snapshot
    re-reads them; while another tab is shown nothing runs). They come from the
    layout results, not from the style the element declares: width and height
    are the border box the layout measured (`fill`, `auto`, a percentage and a
    flex share are already pixels), `content` is the box inside the padding,
    `padding` and `margin` are what the layout applied, `offset in parent` is
    where the box landed inside its parent's content (margin plus alignment),
    `position` is its place in the window, `children size` and `scroll offset`
    are the engine's, and min/max sizes are resolved (a percentage of the
    parent's content box; `none` for no limit). Text is the style after
    inheritance: family, size, weight, style, stretch, letter spacing, colour,
    alignment, and for a label or paragraph the lines the text laid out in, its
    measured size and the line height that comes to. Paint shows the opacity
    with every ancestor's folded in, and Element the paint layer. When the
    element declared something else, a muted `declared …` chip sits beside the
    used value (`declared fill`, `declared 50%`, `declared inherited`); no chip
    means the declaration was honoured. A value the engine does not expose for
    the element is an em dash with a faint `not exposed` marker, never a
    default. All lengths are logical pixels (the layout's physical pixels
    divided by the window's scale). `inspector/computed.rs` is the pure
    resolution, tested without a window.
- **Performance** records the app for a while: Record, use the app, Stop. The
  header shows the state (a dot and "Recording 3.2 s · 142 frames", then
  "Recorded …") and Clear drops the recording. Four stat tiles summarise the
  frames: frame rate while drawing, average frame, worst frame and the share
  over the 16.7 ms budget. Below, a `timeline_chart` lays every frame along a
  time axis with the budget as a dashed line and slow frames in red; hover a
  bar for its time and length. Drag across the timeline to select a stretch:
  the tiles and the call graph then describe only that stretch, and Clear (or a
  click on the chart, or Escape) returns to the whole recording. Call stacks
  sampled while recording are a `flame_graph` of the selected stretch: bars
  carry the function name, cut with an ellipsis, the detail line under the
  graph gives the full name, its share of all samples and of its own time, the
  bar under the pointer is highlighted, the application's own code is tinted in
  the accent colour, and a click zooms into a call (Escape or All calls zooms
  out). The graph starts past the calls at the top that only call the next one.
  Rust symbols are demangled, legacy and v0. Before the first recording the
  tab says what to do instead. Frames are timed and stacks sampled only while a
  recording runs; the view updates live. The tab scrolls when the window is too
  short for everything, and the tiles stack two by two when it is narrow.
  Keyboard: Tab reaches Record, the timeline and the call graph. On the
  timeline Left and Right move between frames, Shift with an arrow selects, and
  Escape clears. On the graph Down, Up, Left and Right walk callee, caller and
  siblings, Enter zooms in and Escape zooms out.
- **Accessibility** lists what the checks found in the app. The top shows
  stat tiles with the counts of errors, warnings and notes, and a segmented
  filter by severity beside Collapse all and Expand all. Issues sit in one card
  per kind (Naming, Focus, Target size, Contrast, Geometry, Layout shift,
  Motion, Performance). The card's header names the kind with its count, says
  why such problems matter, and folds the issues beneath it. An issue row has a
  severity icon, what is wrong, the element (`Button #485 "Save"`: the component
  that built it, its id and its name or text) and how to fix it. Hovering an
  issue outlines its element in the app. Pressing it, or Enter on it, selects
  the element in Elements, opens the tree down to it and scrolls it into view,
  and leaves it outlined in the app. Up and Down move between rows, and Tab and
  Shift+Tab move between every control. Before the first snapshot the tab says
  it is waiting for an application window, and a window that passes every check
  shows an empty state.

Environment variables for tests and screenshots:

- `BINGUX_INSPECT_OPEN=1` opens an inspector over the first window that
  appears.
- `BINGUX_INSPECT_SHOT=<dir>` writes `app.png`, `inspector.png` and, for an
  inspector of an inspector, `inspector2.png`, `inspector3.png`…, at most once
  every 1.5 seconds each. Without it, nothing is read back from the canvas.


### Form dialogs

`open_form(overlays, title, content, on_closed)` shows a form in a framed dialog (a sheet that fills the window on a phone); `content` is a component that renders `form_dialog(title, body, action, on_action)`, so the action, `.blocked(reason)` and `.busy(message)` follow the form state. The card is one size (`FORM_WIDTH` x `FORM_HEIGHT`) whatever the body holds: a standard `HeaderBar` with Close, a scrolling body, a footer with a one-line reason and Cancel / action (the action carries the Return key cap; on a phone the buttons stack, action on top). Enter runs the action unless a control inside takes it; Escape closes. Options that come and go use `reveal_block`, so nothing jumps. Used by Compress and Extract in Files; any settings dialog with a few fields and one action is the second use. `Dropdown::unavailable(vec![(key, reason)])` lists an option that cannot be chosen, dimmed, with its reason. `files::compress_preview(CompressPreview)` renders the Compress dialog in a fixed world for the taxonomy and tests (`COMPRESS_PNG_DIR=dir cargo test --features testing compress_tests` writes every state at 1100x820 and 390x800).
