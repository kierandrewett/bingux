# bingux-ui

`crates/ui` is the design system for Bingux's native applications.
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
`vendor/README.md`), `bingux_ui::resilience` decides what the user
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
- A second launch does not open a second window. It sends each address named
  on its command line to the running copy over `$XDG_RUNTIME_DIR/<app-id>.sock`
  and exits with status 0. The running copy opens each (a hierarchical app
  navigates to it; Files opens a tab) and raises its window. A socket left by a
  crash is replaced. An option other than `--help` is refused (see the
  navigation contract), never forwarded.
- **The inspector comes with it.** `Application::run` is the only way to start
  a bingux-ui window: it builds the one launch configuration (interface font,
  the inspector plugin, the borderless plugin) in a private function, so every
  app, example and tool has the inspector and its Ctrl+Alt+Shift+I shortcut
  with no code of its own, in the main window and in any other window the
  process opens. Do not call `freya::launch` or build a `LaunchConfig`: the
  inspector would be missing, and `scripts/ui-lint.py` fails on both anywhere
  outside `crates/ui/src/app.rs`. (Dialogs, popups and sheets are layers of
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
leaving the page. The same pose plays for Back on a compact window. Back on
a desktop window slides the page out. Every one of them takes `SLIDE_MS` (320 ms,
the time a subpage takes to open) on the one slide curve, so a close never
differs from an open in time or in shape. A swipe released past its threshold
carries on at the speed of the finger (see the next paragraph). A cancelled
swipe springs back over 240 ms. Under reduced motion it is a cross-fade with
no spring.

**The slide curve.** Every slide, in and out (a subpage push and pop, predictive
back that is not a swipe, a sheet opening and closing), is `slide(t)` in
`nav/gesture.rs`: the symmetric ease `t^k / (t^k + (1 - t)^k)`, `k` being the
one named constant `SLIDE_SHARPNESS` (2.0). The curve is point-symmetric about
its middle, `slide(1 - t) = 1 - slide(t)`, so a close is the open played
backwards and also the open mirrored in space: the same speed and shape by
either reading, and it has no slow start that reads as lag (an expo-out open
reversed does) and no fast start that reads as a snap (an expo-out close does).
Turn `SLIDE_SHARPNESS` for a snappier (3) or softer (1.5) slide, `SLIDE_MS` for
a faster or slower one; every slide follows. How far along the way a slide is:

| ms | original open, and close | close as the open reversed | now, open and close |
| - | - | - | - |
| 16 | 29.3 % | 0.0 % | 0.3 % |
| 48 | 64.7 % | 0.2 % | 3.0 % |
| 96 | 87.6 % | 0.7 % | 15.5 % |
| 128 | 93.8 % | 1.5 % | 30.8 % |
| 160 | 97.0 % | 3.0 % | 50.0 % |
| 224 | 99.3 % | 12.4 % | 84.5 % |
| 256 | 99.7 % | 24.9 % | 94.1 % |
| 288 | 99.9 % | 50.0 % | 98.8 % |
| 320 | 100 % | 100 % | 100 % |
| 5 % / 50 % / 90 % reached at | 2 / 32 / 106 ms | 183 / 288 / 315 ms | 60 / 160 / 240 ms |
| peak speed, times the average | 6.9 | 6.9 | 2.0 |

(Sharpness 3: 5 % / 50 % / 90 % at 87 / 160 / 216 ms, a peak of 3; sharpness
1.5: 39 / 160 / 260 ms, a peak of 1.5.) A swipe released past its threshold
leaves on a cubic Hermite curve (`release_curve`) that starts at the finger's
speed (`release_slope`, at most 3 times the average) and stops dead, so it
never stands still where the finger let go; a finger at rest starts it from
rest.

### The history menu

`BackButton` and `ForwardButton` open a menu of the places they would reach,
nearest first, on a **right click** or a **450 ms hold** (a finger or the
primary button) of the button. Choosing a row is one `pop_to` or `forward_to`.
The hover tooltip closes when the menu opens. The rows are the route titles (and
the route's icon when it has one). An app with its own button builds the same
menu with `IconButton::context_menu(history_menu(model.back_menu(), on_pick))`;
the file chooser and Files do, with `history_menu_with` to give a row a
**middle click** too (`MenuItem::on_middle`: Files opens that folder in a new
tab). The mouse's own Back and Forward buttons (8 and 9) call `back()` and
`forward()` in a window of Settings and in a file view.

### The navigation contract

There is one model, `NavModel` (`nav/model.rs`), and one router around it.
Settings (`Mode::Hierarchical`), the file views and the chooser
(`Mode::Linear`) differ only in the table's mode; nothing keeps a second
history. The rules, tested in `nav/model.rs` and, for the file views, by a
seeded random walk of 12,000 histories against a browser's (`files/history_fuzz.rs`)
and 10,000 windows of tabs (`apps/files/src/fuzz.rs`):

| Rule | Hierarchical (Settings) | Linear (Files, the chooser) |
| ---- | ----------------------- | --------------------------- |
| What the stack is | The page's ancestors: Back leaves a page, Forward does not exist | A history: back list, current, forward list |
| `push`/`navigate` to the current place | Nothing (a changed fragment only scrolls to the anchor) | Nothing |
| `push`/`navigate` elsewhere | A new entry; a deep link builds the ancestors | A new entry after the current one; the forward list is forgotten |
| Only the query changed | The top entry is replaced in place, unless the key `query_pushes` | Same |
| Back, `pop_to` | Leaves; what was left is dropped | Moves; what was left becomes Forward |
| Cap | The depth of the route tree | The last `HISTORY_LIMIT` (100) entries behind the current one; the oldest is forgotten |
| Transition | `Push` deeper or sideways, `Pop` shallower, `Fade` another root or the index on a wide window, `None` the same path | `None` always |
| Entry state | Retained by entry id (scroll, `use_retained_state`), dropped when the entry leaves the stack | Kept by the host per folder and tab (see `docs/files.md`) |
| A launch (second copy, `open_uri`) | `navigate_to`: a deep link, then the window is raised | `reset`: the window's router holds the one location of the last launch and the host opens it in a tab; no history grows |

A command line is parsed once, by `Application::run`: `--help`/`-h` print the
usage, `--` ends the options, any other argument starting with `-` is an
*unknown option* (a message and the usage on stderr, status 2, nothing
opened), and every other argument is an address, in order, so a flag can never
become a location. An application's `normalise` turns each address into the
text of a location and may refuse it by returning an empty address.

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
is a folder, `recent:///` the recent files, `trash:///` the Trash (and
`trash:///Old%20folder` a folder in it) and `bingux-files://` an alias of
`file:`. A tab's folder, Back and Forward are a `NavModel` on it, so the toolbar's
Back and Forward buttons have the history menu above, and a tab shown again is
where it was left. `files::location_of(&Where)` and `files::place_of(&Location)`
convert between a place and its location. A folder whose name is not valid UTF-8
is recorded byte for byte (percent-encoded in the URI), so going Back to it opens
that exact folder; only its title shows replacement characters.

**Named roots** (`bingux_ui::files::named`, exported as `Root`, `Roots`,
`Typed`). A place is a `Where`: a folder or Recent. The Trash is the
`Where::Folder` of the home bin's `files` folder, so everything a view keys on
(the listing, the history, the watcher, the sidebar's selected row, the Trash
rules of `actions::TrashView`) is the one path and a real path to it is the same
place as `trash:///`. What is *named* is every way of saying a place, and
`Roots` is the only code that does it: `uri` (what Copy location gives),
`edit_text` (the address field), `parse` (the text typed back: `recent:///`,
`trash:` and `trash://` and `trash:///a/b` with `..` held inside the Trash,
a real path, `~`, `file://`), `by_word` (`Trash` or `Recent` alone, when no
folder has the name), `crumbs`, `title` (tabs, the window, history rows),
`icon`, `parent` (Up; none from the Trash) and `completion_text` (a
`trash:///na` being typed is completed from the bin's folder). Inside the Trash
the first name below it is the item's name *before it was deleted*
(`Roots::crumbs`, `title`), read from its `.trashinfo`; the address keeps the
name in the bin. `Roots::new(home, Some(bin_files))` makes up roots for a test;
`Roots::current()` reads `$HOME` and `$XDG_DATA_HOME`. A new named root (a
network place, a drive by its label) is a `Root` variant and a row in those
functions; no caller learns its name.

Every move of a file view's history goes through `bingux_ui::files::history`
(`visit`, `up`; Back, Forward and the menu choices are the model's own): it
admits only places (an absolute folder path, cleaned by name and never resolved
through a link, or the recent files), so text that is not a place never becomes
an entry. The rules a person feels are in `docs/files.md`, "History". The
window's router in such an app carries only the location of the last launch,
as a stack of one (see the contract below).

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
| Rows             | `row` (`subtitle`, `value`, `mono_value` and `path_value` (a read-only value: selectable, never cut: it sits beside the title, wraps, or goes under it; see Info rows below), `badge`, `leading`, `leading_tone` (draws the leading icon in the accent, warning or danger colour, for a row that reports a state), `leading_space` (a blank icon column so text lines up), `leading_app`, `navigates`, `selected`, `destructive`, `context_menu`, `on_press` (makes the row activatable)), `radio_row(title, selected, on_select)` (a radio on the leading edge; the whole row selects; `subtitle`, `value`, `leading_app` as for `row`), `radio_rows(title, rows)` (a group of radio rows: Up and Down move the selection, and only the selected row is in the Tab order), `switch_row` (`nested` for sub-rows shown while on), `slider_row` (a `level_row` for a setting), `level_row` (title on the left and a slider filling the rest of the line; `control` such as a mute button sits between them, `footer` under the track, `mark` and `mark_label` for a tick such as 100%, `danger_from`, `value_text`, `max`, `leading`, `context_menu`), `choice_row`, `text_row`, `entry_row`, `expander_row` (`subtitle`, `leading`, `value`, `expanded`; see Expander rows below), `Row::disclosure` (a row that opens its `nested` rows), `property_row` |
| Icons            | `icon` (bundled symbolic glyphs), `app_icon` (full-colour application icons from the icon theme)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Groups and text  | `Group` (`description`, `content` for a chart or other free content), `section_label`, `group_head(title, trailing)` (a group's title and toolbar with no card, for a page that lists several cards under one name), `caption`, `centered_caption`, `action_row`, `heading`, `status` (loading, empty, error, info blocks)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Path text        | `path_label(path)` (one line that shortens a long path by what matters least, so the full way stays readable: `/home/kieran/…/design/final-report-2026.pdf`, `/h/k/…/design/…`, `final-rep…2026.pdf`; `style`, `ink`, `name` for a file name, `middle` for other text, `selectable`, `copyable`; see Path text below). `elide_path`, `elide_name`, `elide_middle` (module `elide`) are the same cuts as pure functions of a width and a `Measure`; `distinguish` labels paths that share a name |
| Selectable text  | `select_text(text)` (text that can be selected with the pointer and copied: `style`, `align`, `fill`, `tab_stop`, `breaks_anywhere`, `copy_label`; see Selectable text below). `Row::value`, `mono_value` and `path_value`, `property_row`, the licence blocks of About and the Properties values use it |
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

### Tree guides

A `tree_view` row is `lead`, an optional icon, the label run (label, detail, chips, child count) and an optional badge. `lead` is one `Dim::IconControl` column per ancestor level followed by the row's own chevron slot, and every row has the slot, empty on a leaf, so a leaf's label starts where a sibling folder's does and each level is exactly one slot to the right of its parent's.

- **Guide.** Each ancestor level draws a 1 px `Paint::MutedFaint` line (component `TreeGuide`; the separator's 8 % was too faint to follow down a tree) starting `floor(slot / 2)` from its column's left edge, which is the middle of that ancestor's chevron slot. The line starts on a whole pixel, so it covers whole device pixels at 1x and 2x; that puts it half a pixel right of an even slot's exact middle, because a one-pixel line cannot be centred on a whole pixel without blurring at 1x. The slot is 16 px, 18 px at compact density.
- **Continuous.** The guide is the row's full height and the rows sit with no gap, so a level's guides join through every descendant row, whatever each row holds. A row draws one guide per ancestor and nothing else, so a level begins on the row under its parent and **ends at the bottom of the parent's last visible descendant**, never below it; a closed branch has no descendant rows and so no guide. There is no elbow.
- **Fills.** The selected, hovered and pressed fill belongs to the row's box, and the guides are children of that row, drawn over the fill in the same token, so a guide reads the same on a filled row and nothing is cut at the fill.
- **Badge.** The line grows its label run (`grow_children()`), so the badge keeps its own width and ends at the row's padding (`S2`) on the right, with the label shortening to an ellipsis before it. Chips and the child count are inside the label run and are cut, never wrapped.
- **Deep trees.** The view has no horizontal scroll. When `levels x slot` would leave the rest of the row (label run and badge) under six slots, `indent_for` narrows the level to the room left, a whole number of pixels and no less than `S2`; guides and chevrons narrow together, so a guide is still on its ancestor's chevron. Past that the label is what shrinks.
- **Right to left.** The tree mirrors with the window (see Direction): the chevron slot, the guides and the indent are at the start edge, a row's chevron points the way the tree opens (`PanEnd` is mirrored) and Left and Right expand and collapse by the reading direction (`Direction::reading_key`).
- **Motion.** Rows that enter or leave grow and shrink in height inside a clipping slot, with their guides, which are part of the row. No guide is drawn on its own, so none outlives its row.

The taxonomy's Tree view entry shows five levels with a closed branch, a selected deep row, a hovered row (a script captures it), all collapsed, and a ten-level chain. `controls/tree_view_tests.rs` reads the laid-out frame at desktop and phone density and at scales 1 and 2: guide x against the ancestor's chevron, joins between rows, the end of each guide, label alignment, the badge inside the row, row height, whole device pixels, and a twenty-level chain.

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

### Expander rows

`expander_row(title, children)` is a row that opens rows under it: `.subtitle(text)`, `.leading(icon)`, `.value(text)` (a count, read-only, as `Row::value`) and `.expanded(true)` to mount open. It is a `Row` and not a second kind of row: `Row::disclosure(open, on_set)` turns any row into the header of the rows given to `Row::nested`, and `expander_row` is that row with the chevron and the state. A page that needs more than the expander offers builds the row itself.

| Part | Behaviour |
| --- | --- |
| Header | The row's own padding, `Dim::Row` height (60 px at compact density, never under `TOUCH_MIN` there), hover and pressed fill, focus ring and `GUTTER`, on every width. It grows with its text. |
| Title and subtitle | They wrap to every line they need, on a desktop and on a phone, and are never cut with an ellipsis (an ordinary row cuts a title after two lines and a subtitle after three on a phone). The chevron, and a value, keep their place beside the text. |
| Chevron | One `DisclosureChevron` in the row's trailing slot: the size of the chevron of a row that navigates and in the same place (`GUTTER` from the trailing edge, centred on the header however tall it is), so one card of both lines up. It points right while closed and turns to point down as the rows grow, with the same 160 ms and easing; the header does not move while it does. |
| Rows inside | Set in by one `GUTTER`, ending where the header ends, with a hairline between the header and the first row and between every pair (the ones between rows are hidden while either neighbour is lit, as in a card). They are laid out at the width they are shown at, so a long title wraps and a value stacks under its title as it would in the card, and the height of the reveal is that of the laid-out rows. An expander inside the rows is set in by one more gutter. |
| Closed | The rows are not mounted: nothing hidden takes focus or is read out. |
| Motion | The rows below move together by the height the content has at that frame, the rows above and the header never move, and the motion ends at the height a mount of the other state has. A page that scrolls is not scrolled by it. Under reduced motion it is one step. |
| Keyboard and assistive technology | The header is a button and reports `expanded`. Enter and Space press it, Right opens it and Left closes it. |

The geometry is held at 390, 360 and 320 px and on a desktop by `expander_tests` (the header is a row of the card, the chevron's slot, wrapping, the rows' inset, nesting, the frames of an opening and a closing, opening at the bottom of a scrolled page, the keys, the accessibility state); `BINGUX_EXPANDER_DIR=dir cargo test --lib --features testing expander_matrix -- --ignored` writes the cases as PNGs (`BINGUX_TEXT_SCALE=2`, `BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-light` for the other conditions). The taxonomy's Expander row entry shows it open and closed, with an icon, a subtitle and a value, with a long title, nested, and on a phone page 390, 360 and 320 px wide.

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

**Flush buttons.** The trailing buttons are one group (`NoticeButtons`, a row with no gap): the actions and the dismiss cross abut, so their hit areas touch, their hover and pressed fills meet without dead space, and all are `NOTICE_BUTTON` tall on one line. The banner's own gap (`S2`) is only between the message and the group, and the last button ends `NOTICE_PAD` from the edge, the same as above and below, so the cross is optically centred at the end of the pill. Every button takes the container's nested radius, so the end of the group follows the pill's curve (this is how the GNOME banner and the Material snackbar close a notice). It is implemented once in `Banner`; a toast and every specimen get it. Held by `shell/notice_tests.rs` (`the_action_and_the_dismiss_cross_sit_flush`: zero gap, equal heights, equal inset, in every kind, banner and toast, desktop and phone). On a phone the stacked actions row is flush the same way. The dismiss tooltip ("Dismiss") is a bubble as wide as its text, never the width of the cross; `controls/tooltip_layout_tests.rs` hovers it in a banner and in a toast (`Place::Banner`, `Place::Toast`).

**Several actions.** `.action(..)` can be called again. On a desktop the buttons share the row with the message; on a phone (compact density) a banner with more than one action puts them on a row of its own under the message, end-aligned, with the dismiss cross at the top corner, so the message is never squeezed to a word a line. That banner is tall, so it takes the tall radii above.

**Colour.** `BannerKind` decides every colour, from `tokens.notice(kind)` (`NoticePalette`): `fill`, `ink`, `glyph`, `edge`. For a tinted kind the fill is a soft tint of the kind's hue (the accent for Info, `success`, `warning`, `danger`): a hint of it, lightness 0.965 on a light scheme and 0.27 on a dark one with a fifth of the hue's chroma, opaque so it reads over whatever a toast floats on. The text, the dismiss glyph and the label of an action button are that same hue moved in lightness: darker than the fill on a light scheme, lighter on a dark one, until they reach 7:1 (AAA) on the fill. The saturated colour is the leading kind icon alone (`Paint::NoticeGlyph`, the hue at 3:1 on the fill). The edge is not an outline: a 1 px hairline of the ink at 12% over the fill (`Paint::NoticeEdge`), the same for every kind including Neutral, exempt from the contrast matrix as decoration. An action button is **quiet**: no fill at rest, its label in that ink, the hover and pressed layers of every control over the fill, the focus ring, a nested-radius pill the same height and inset as the dismiss cross, so the notice reads as one calm surface. `Neutral` is the dark floating surface of a tooltip (`on_floating` ink). Paints: `Paint::NoticeFill/NoticeEdge(kind)`, ink `Ink::Notice(kind)`; `icon_button(..).ink(Ink::Notice(kind))` draws a glyph in it. Every kind, in both schemes and for every accent, is in the contrast matrix, including the quiet action label at rest, hover and press.

**Icons.** Info, Success, Warning and Error lead with `DialogInformation`, `Check`, `DialogWarning` and `DialogError`, so the kind is not told by colour alone; `.icon(Icon)` picks another, `.without_icon()` none. A neutral banner has none.

**Inline.** A banner opens and closes by height through `reveal` (`.visible(bool)`; no motion under reduced motion), and a closed one is unmounted. `.gap(space)` names the column's spacing so a closed banner takes none and the page does not jump.

**Floating.** The toast shell adds only the part that floats: a layer above the dialog and menus, the `Elevation::Raised` shadow, bottom-centre placement (`S6` above the bottom edge plus `Dim::SafeBottom` on a phone), a 140 ms slide-and-fade, a queue (toasts show one at a time, in the order they were queued) and a timer: four seconds, six with an action, set with `Toast::timeout`. The timer is a pure state machine, `shell::hide_clock::HideClock` (tested with a fake clock), held by three things: the pointer anywhere over the toast's own box (padding, gaps, text and buttons: the shell listens to `pointer_over` and `pointer_out`, which a button inside it does not steal), a finger down on it, and keyboard focus on one of its buttons. While held the clock does not run and the toast never closes; when the last hold lets go the **whole** delay starts again from zero (not from where it paused), and entering again pauses again. A toast that arrives while one is held waits behind it. On a touch screen a sideways drag of 72 px or more dismisses it; a shorter one springs back. Pressing an action also dismisses the toast.

### Progress bars and spinners

`progress_bar(fraction)` takes `Some(0.0..=1.0)`, which fills in the accent and eases to each new value, or `None`, which is indeterminate; `spinner(size)` is the round one. An indeterminate bar and a spinner loop for as long as they are on screen, and a caller does nothing to keep them going: a label or width that changes on the bar, or a page that draws again, never restarts the loop (it starts over only when the bar is mounted again). A determinate bar runs no loop at all.

- **Indeterminate bar.** A segment, 30% of the track, sweeps from just off the left edge to just off the right edge in 1.4 s (sine in and out) and starts over from the left; the track clips it. Until the track has been measured the bar draws no segment, so its first frame is already at the start.
- **Spinner.** A turning half ring over a faint full ring, one turn in 800 ms. `.ink(paint)` and `.text(caption)` change the arc's colour and add a caption.
- **Reduced motion.** Progress is essential motion, the one kind that reduced motion slows and does not stop, because a bar that stood still looks frozen. The bar's segment does not travel: the whole fill breathes between 30% opacity and solid over 2.4 s. The spinner turns once in 2.4 s.

Every loop reads its position from `motion::use_cycle(ms)`: a phase from 0 to 1 that starts over, kept in the scope of the component that calls it (see Motion). Build a new looping indicator on it, not on `use_animation` with `OnFinish::restart()`; `scripts/ui-lint.py` fails an `OnFinish::restart` or `reverse` outside `motion.rs`.

`offset_x` and `offset_y` shift a box's **children**, not the box. The sweep moves its segment by offsetting the strip that holds it; the segment's own `offset_x` moved nothing, which is why every indeterminate bar once stood still while its animation value kept changing. Position what moves from the box above it, or use `Position::new_absolute()`.

`controls/progress_tests.rs` holds the checks. It steps the animation clock and reads where the segment is laid out and what the renderer draws on each step: the segment starts off the left edge, moves on every step, reaches the right edge and starts again from the left, stays inside the track, goes on from where it was after the page draws again, a determinate bar has none, a bar and a spinner under reduced motion still change on every step, and `use_cycle` runs 0 to 1 and starts over. `scripts/loop-check.py` is the same check on the taxonomy's rendered pages, and the taxonomy's Progress bar and Spinner pages hold the live specimens, with a dialog and a popup that carry an indeterminate bar and a reduced-motion one of each.

### Tab strip

`tab_strip(tabs, selected)` is the tab strip of a window that holds several views of one kind: the folders of Files, the documents of an editor. A tab is a rounded rectangle of `Radius::Control`, the corner of a button and of a sidebar row; the selected tab has the fill of a selected sidebar row (`Paint::Selection`), the pointer's fill is `Paint::Hover` and a press is `Paint::Pressed`. The tabs are separate from the content under them, as the segmented control is from the page it switches: nothing joins them. The design thesis, what it replaced and the numbers are in `design/tabs.md`. The host owns the list and the selected id; the strip holds no state. `.on_detach(f)` lets a tab be torn off (dragged a strip height past the strip, released there, or, for a lone tab, dragged at all; `f` gets `Detach { id, pointer, press, grab, released }` and the strip puts the tab back and ends its drag, the host takes it out of its list); `.insertion(Some(i))` draws the accent marker of where a tab brought from another window would land; `.probe(&StripProbe)` publishes the tab bodies' places (`StripLayout::index_at`, `boundary`) so a host can work out that place. The thresholds and hysteresis (`TearGate`) are in `controls/tab_tear.rs`; what the compositors allow is in `design/tab-tearoff.md`. `bingux_ui::app::open_window(seed, location, size)` opens another window of the application and `use_window_seed::<T>()` hands the seed to its root; `Application::transparent()` gives windows an alpha channel so one can be drawn faded. Use `segmented(..).tabs()` instead for a few fixed pages that cannot be closed and have no content of their own to attach to (the inspector's tabs, the Properties dialog).

The strip is the title of a `HeaderBar`: `.tab_slot(strip)` holds it from the start of the room between the bar's controls, and the header bar *is* the strip (`Dim::TabBar` tall, 40 px, so the window has no second row above its tabs; the window controls share the row; its icon buttons are `Dim::TabButton`, the height of a tab; the band above and below the tabs still drags the window). The bar keeps its default fill, `Paint::Surface`, which is the page's colour (`Paint::Bg`), so the bar and the page under it are one colour and nothing is drawn between them: no darker strip, no card, no hairline, no flare. The page runs from the bar's bottom edge to the window's left, right and bottom edges, square, as the Settings page does; the window's rounding is the compositor's. A tab is `FIELD_INSET` from the bar's top and bottom, so there is a gap between a tab and the page. A strip outside a header bar draws the same tabs on whatever is behind it, which should be `Paint::Surface`: the fades at its overflowing edges dissolve into that. Use `segmented(..).tabs()` instead for a few fixed pages that cannot be closed and have no content of their own to attach to (the inspector's tabs, the Properties dialog).

```rust
HeaderBar::new("Files")
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
    )
// then the page, a sibling under the bar: no frame round it
```

Every length is a token. The strip is `Dim::TabBar` tall (40 px, 44 px at compact density; 8 px shorter than the `Dim::Header` bar it replaced at mouse density and 12 px at compact). A tab is the strip less `FIELD_INSET` above and below (`Dim::TabButton`): it stands 4 px from the top edge and is 32 px tall, the same distance from the strip's top and bottom, and as tall as the New tab button; at compact density there is no inset and the tab is the whole 44 px touch target. Its corners are all `Radius::Control`; two tabs are `S1` (4 px) apart, and the first starts at the header bar's `S2` padding. The icon is 16 px and the title is `CAPTION`, the small type step. The close button is the standard `icon_button`, `Dim::DenseControl` (24 px; the whole 44 px `Dim::InlineButton` at compact density) centred in the tab, `FIELD_INSET` from its edges, with the tab's corner nested by that inset (`Radius::Inner`); the New tab and Tab list buttons are `Dim::TabButton` in one cluster at the end of the strip, with a tab's corner. The strip never overflows its container: it is as wide as its tabs (so the New tab button follows the last tab) and no wider than the room it is given, and every part is laid out from the measured width, so nothing can spill out of it.

| Part | Behaviour |
| ---- | --------- |
| Width | Tabs share one width: `Dim::Field` at most, shrinking together as tabs are added down to `Dim::TabMin` (96 px; 128 px on a phone, where the close slot is a whole touch target and a title still needs room beside it). Beyond that the strip scrolls sideways instead of squeezing, the strip fills its container, and the tabs fade into the bar where they continue. **Scrolling**: the wheel over the strip (the vertical wheel too, forwards when turned down), a touchpad's two-finger movement and a finger dragged along the tabs scroll it; a wheel notch glides on `MOVE_MS`, a touchpad and a finger are followed at once; the selected tab is scrolled into view by the shortest distance whenever the selection moves. The offset is kept from the strip's start edge (`controls/tab_scroll.rs`), so the same rules hold right to left. |
| Tab | `[icon] [title] [close]` on one row (mirrored right to left). The title takes the room that is left; one that does not fit fades out over its last 24 px (`fade_text`) before the close slot, which is never faded. The close slot is always laid out, so a tab never changes width when the button appears. The button is the standard `icon_button` (a 24 px hit target, 44 px at compact density, centred in the fill and inset by `FIELD_INSET`, so a press on it is stopped before it can reach the header bar's window drag), named "Close <title>", drawn (fading in with the fill) on the selected tab, on a tab the pointer is over, and on every tab on a touch screen and at compact density; pressing it neither selects nor drags the tab. **Every tab closes, a lone one too**: the strip reports the id and the host decides (Files closes the window). The selected tab is the attached shape; an unselected one is flat, shows `Ink::Muted` text (4.5:1 on the strip, and under the hover and pressed fills), and has a short separator after it that fades out next to the selected tab and the tab the pointer is on. |
| End of the strip | The New tab button (`on_new`) follows the last tab, an `icon_button` of `Dim::TabButton` (32 px square, 44 px at compact density) in a cluster as tall as the strip: its fill is `FIELD_INSET` from the strip's top, its bottom, the last tab and the strip's trailing edge (flush at compact density). It never scrolls. When tabs overflow, a Tab list button (`Icon::PanDown`) before it opens a menu of every tab: each tab's icon, its name (`MenuItem::name()`: cut in the middle, the whole name the tooltip), the selected one ticked at the end of its row, mirrored in a right-to-left interface. |
| Motion | Adding a tab grows it in and the others narrow to make room; closing one shrinks it away and the others widen (`MOVE_MS`, instant with reduced motion). The widths are laid out from one clock, so the strip never exceeds its container during the motion. **Rapid closure**: after a tab is closed with its button or a middle click the others keep their width until the pointer leaves the strip's row, so the next close button comes under the pointer (and is shown as hovered though the pointer has not moved); the widths then grow to the room. `tab_close::RapidClose` is the state machine, over a clock the strip passes in. Closing with the keyboard or the menu, or adding a tab, does not hold. |
| Pointer | A press selects (a mouse as it goes down, a finger when it lifts). A middle click closes. **The tab the pointer is on is worked out from where the pointer and the tabs are**, not from enter and leave events, so exactly one tab is hovered and it stays right when the tabs move under a pointer that does not (a wheel notch, a tab closing, a rapid close); its fill (`Paint::Hover`, the whole tab, corner `Radius::Control`) fades in and out on `MOVE_MS`; pressed (`Paint::Pressed`) is at once; the selected tab has the selection fill, as one opaque colour over the bar so a held tab hides the titles it slides over; a held tab is lifted. With `on_reorder`, a tab dragged more than 6 px follows the pointer, drawn over the others, which slide aside on the springs of `crate::reorder`; letting go settles it into its place and calls `on_reorder(id, to)` once, after it has settled. Dragging past the strip's end scrolls it. Escape, or letting go more than a header's height above or below the strip, puts the tab back. The strip uses `crate::drag::PointerDrag` and `crate::reorder::Reorder`, as `sortable` does. A right click, or a finger held on a tab, opens its menu (`tab_close::menu_rows`): Close tab (Ctrl+W), Close other tabs, Close tabs to the left (greyed out on the first tab) and Close tabs to the right (with `on_close_many`, which gets the ids in strip order), Duplicate tab (`on_duplicate`), Reopen closed tab (Ctrl+Shift+T, `on_reopen(available, f)`; greyed out without a closed tab), and Move left and Move right with `on_reorder`, which is how touch reorders. `on_hover` reports each change of the tab the pointer is on, so a host can accept a drag on it (see Files below). |
| Keyboard | Only the selected tab is in the tab order. The arrows are the screen's: Left and Right move the selection and the focus (Right goes back in a right-to-left strip), Home and End go to the first and the last; Ctrl+Shift+Left and Right move the selected tab (`on_reorder`); Delete or Ctrl+W closes the selected tab (also a lone one) and the focus follows to its neighbour; Ctrl+T calls `on_new`; Ctrl+Shift+T calls `on_reopen`. The tab's focus ring is drawn inside the tab, with its rounded corners, so the scroll region cannot clip it. The dragged tab is lifted by a layer (`HELD_LAYER`) because Freya paints by depth in the tree, so a tab's contents would otherwise draw over a neighbour's fill. A tab wider than the room left in a scrolling region shows its start. |
| Accessibility | The strip is a `TabList` named "Tabs"; each tab is a `Tab` named by its title with its selected state. The buttons are named "Close <title>", "Tab list" and "New tab". |

When a tab closes the host picks the tab that takes the selection: `tab_close::selection_after_close(ids, closed, selected)` is the browser's rule (the neighbour to the right, else the left; the selection stays if another tab closed), and `tab_close::others`, `tab_close::to_the_left` and `tab_close::to_the_right` give the sets for the three menu items. Files keeps its tabs as a plain `TabSet` (new tabs open beside the one on show; `files::ClosedTabs` is the stack behind Reopen closed tab: the last 10 closed tabs with their place and their `TabMemory`, so Back and Forward come back too, and a closed group (others, to the left, to the right) comes back left to right at its places; `TabMemory::duplicate` is Duplicate tab). Closing the last tab closes the window, with a confirmation only if `files::operations_running()` (a copy, move or compression is going).

The taxonomy's Tabs entry shows each state in a bar over a page at both densities: select, close and add with drag; every close route and the menu; many tabs; the first, a middle and the last tab selected; nine tabs; long titles (faded); icons; a lone tab with its close button; no New tab button; keyboard focus; a right-to-left strip; the pointer on one tab. `controls/tab_geometry.rs` holds the geometry tests (the strip is a tab bar tall, a tab is the same distance from the strip's top and bottom and as tall as an end button, the content width and its inverse), `controls/tab_tests.rs` the rendered ones (every width, density and reading direction; a tab's four corners are one radius and its fill is the selection, hover or bar colour in both schemes; the page's corners and top edge are the page's colour whichever tab is selected), `controls/tab_close.rs` the closing rules (selection, `to_the_left`, `others`, `to_the_right`, the menu's rows and their order, rapid closure with a fake clock), `controls/tab_scroll.rs` the scroll rules (logical and screen offsets, wheel and finger, scroll into view, mirrored), `controls/tab_tear.rs` the tear thresholds and the layout a host asks (`index_at`, `boundary`, both directions), `controls/tabs.rs` the layout, the fill and the hit-test, and `controls/tab_tests.rs` renders strips at mouse and compact density: the selected tab's bottom edge is the content's top edge at every width, the card's margins are equal on the left, right and bottom and its corners are checked to the pixel, the New tab button is the same distance from the top, the bottom, the last tab and the trailing edge, everything is inside the strip, tab widths are uniform, the seam and the flares are checked to the pixel in both schemes, the close button's size, its press not reaching the drag region, the selection after a close, the menu (with Close tabs to the left), Reopen, rapid closure, and the keyboard, drag and motion behaviour.

### Fading text and reading direction

`fade_text(text).style(CAPTION).grow()` is a line of text that does not fit: instead of ending in an ellipsis its last `Dim::TextFade` (24 px) fade out. The fade is the glyphs' own alpha (the text's paint is a gradient from the ink to the ink at no alpha), not a coloured overlay, so what is behind the text (a hovered tab's fill, the selected tab's surface, the strip) shows through and the neighbour at the end of the line, a close button, is never touched. It is the end of the *reading direction*: the right edge in a left-to-right interface, the left edge in a right-to-left one. Text that fits is drawn plain. The label inside is a plain `label`, so the text is in the accessibility tree and found by a test like any label. Use it for a title cut beside a control; use `path_label` where the end of a name matters (it cuts the middle).

The layout engine reads a direction (see Direction). In a tab strip the first tab is at the right, a tab's icon is at its right and its close button at its left, the New tab button follows the last tab on its left, the strip scrolls forward leftwards, a tab dragged to a later place goes left, the arrow keys are the screen's and `StripLayout { rtl }` counts the place a torn tab would land from the right. The engine does the mirroring; the code does only the pixel maths.

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

### Info rows: a value is never cut

A row with a read-only value (`value`, `mono_value`, `path_value`,
`property_row`) shows it in full. `choose_value_line` (in `controls::adaptive`,
a pure function of widths, tested without a window) decides from the measured
title and value and the row's width:

| Form | When | Layout |
| --- | --- | --- |
| `Beside` | the value fits on one line beside the title's floor | title in what the value leaves, value on one line at the row's end |
| `Wrapped` | desktop only: the value needs two lines or fewer in what the title's floor leaves | the value wraps, aligned to the row's end; the row grows |
| `Stacked` | anything else, and every value on a phone that does not fit on one line | title above, value under it at the full width, left-aligned, wrapping |

The title keeps its floor in every form (its natural width, or 40% of the line
when less, never less than its longest word) and wraps between words with no
line limit. The gap between the title's cell and the value is the row's `S3`
plus the trailing area's `S1` margin (`ROW_TRAILING`): the decision counts both,
or a title shrunk to leave the value its width leaves it 4 px less and wraps it
(`expander_tests::the_layout_rules_find_nothing_in_any_case`). A value that may
break anywhere (`mono_value`, `path_value`) has no longest word. A very long list (licences) is not a row: it is a `select_text`
block under a heading, as in the About window's Legal page.

The row's width is taken from its own size once laid out, before that from the
width its surface told it (`rows::provide_row_width`; About and Properties know
theirs), else from the last row laid out in this window at this width, else the
window's. A wrong first guess wraps the value (valid at any width) and the next
frame moves it under its title if it needs the room; only a row that mounts
first in a window with no surface that knows its width can move once.

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

**A cascade is a stack of opaque cards.** A submenu opens beside its parent and
goes the way its parent went, so it never lies over the menus it came from; with
no room that way it opens on the other side, which can put a third or fourth
level over the root (a 700 px window, or a menu near the right edge). Every
level is the same surface (`Paint::Popover` fill, hairline, `Radius::Overlay`,
`Elevation::Raised`) and each is painted above everything the level before it
holds: Freya paints a node at its parent's layer plus one per level of depth,
so the rows of one card outrank a sibling card's fill, and two cards on one layer
would let the earlier card's labels, highlight and chevrons show through the later
one. `Panel::nested(level)` puts level *n* on `layers::cascade(n)`
(`n × CASCADE_STEP` relative layers, 256 each, so a level is above the deepest card
before it). `shell/menu_cascade_tests.rs` covers it: at 700 px (compact), 760 px
(regular), on a phone, in both directions and for three and four levels, the last
card has the fill of the root and is the same pixels when the text under it changes
or when the window is wide enough that nothing is under it.

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

Popups share one motion, all with an ease-out curve (a sheet and a subpage move
on the slide curve instead: see below) and all scaled by
`tokens.motion(ms)`, so they are instant under `BINGUX_REDUCED_MOTION=1`.
Each layer stays on screen until its exit finishes.

- Popup: fades in and scales from 94% to 100% in 150 ms. The scrim fades with it. Closing reverses it. (On a narrow window a popup is a sheet: see Sheet below.)
- Dialog: fades in and scales from 96% to 100% in 120 ms. The scrim fades with it.
- Menu: fades in and scales from 96% to 100% in 90 ms, pivoting on its top-left corner (the anchor).
- Toast: slides up 8 px and fades in over 140 ms. Dismissing it slides it back down and fades it out; a swipe moves it with the finger. An inline banner opens and closes by height (`reveal`).
- Dropdown: on a phone a sheet (see Panel); on a desktop it fades and scales in over 90 ms, the same easing as the menu, anchored to its button.
- Subpage: a push slides the new page in from the right over `SLIDE_MS` (320 ms) and the page beneath stays still. It moves on `slide`, the symmetric ease-in-out above: it is moving within three frames and has 90 % of the way done after 240 ms. A pop is the same slide leaving: the page goes over the same 320 ms on the same curve, which is the push played backwards and the push mirrored in space at once, so a close is neither faster nor slower than an open and neither begins by standing still. A swipe released past its threshold is the exception. It carries on from where the finger left it (`release_curve` from the finger's speed), because the finger already supplied the start of the motion. The moving page is a card while it moves: its leading corners take `Radius::Window`, and a soft shadow (`sheet_edge_shadow`: 24 px offset, 32 px blur, black at 0.5 alpha at full amount) falls on the page beneath. Both scale to nothing over the last 15 % of its way, whichever way it goes: the look follows where the page is, not the time. The window's own buttons stay at the window's top right while it moves. A sidebar switch cross-fades over 120 ms. A move that starts mid-slide continues from where the slide is (a page that is turned round at once starts from rest, as every ease-in-out does). `shell/page_motion_tests.rs` steps the animation clock in fixed 16 ms steps and compares the open and the close frame by frame, at desktop and phone width.
- Sheet (a popup on a narrow window, or a dropdown on a phone): is as tall as its content up to a share of the window (70 % for a choice, 92 % for a popup). It is laid out once at its final size and only translated (`offset_y`), so nothing in it reflows while it moves, and a popup's sheet does not start until its body is measured, so no frame draws it anywhere but off the window or in place. It moves on the same tokens as a subpage (`nav/gesture.rs`), by the one clock in `shell/sheet_motion.rs` (`use_sheet_clock`, 0 to 1 over `SLIDE_MS`, 320 ms), and its openness is `slide(clock)`:
  - **Open** is `slide`, the symmetric ease-in-out of a subpage: 3 % of the travel in the first 48 ms, half at 160 ms, 90 % at 240 ms. The scrim is its own box under the sheet and follows the sheet's position (`scrim_amount`), so it fades with exactly the sheet's timing and the sheet itself never fades.
  - **Close** (Cancel, the accept button, Escape, a press on the scrim, Back) runs the same clock backwards over the same 320 ms. `slide` is point-symmetric, so that is the sheet retracing its opening at the same speed and also the opening mirrored: it has gone 3 % of the way in the first 48 ms and half at 160 ms, exactly as the open had. To soften or sharpen every open and close at once, change `SLIDE_SHARPNESS` (or `SLIDE_MS`): the sheet follows.
  - **Interrupted** at any point, the clock turns round where it is and has only the time that is left to run: a sheet closed half way up goes back down the path it came up, and a close that is reopened goes up again. The position never jumps.
  - **A release past the dismiss distance** carries on from the pull at the speed the finger had (`release_offset`: a cubic Hermite curve from the pull and the release velocity to a dead stop off the window, over `SLIDE_MS`; a finger at rest starts it from rest, a flick starts it at the flick's speed, up to 3 times the average). The inset behind it and the scrim follow it.
  - **A release short of it** springs back from where it is, over `SPRING_MS` (240 ms), on the page spring (`spring`, `SPRING_ZETA`, `SPRING_OMEGA`) plus the same spring's response to the release velocity (`spring_back`); it never rises more than `lift_limit` above rest. A finger that catches a sheet that is still springing takes it from where it is.
  - Reduced motion jumps the clock and the spring: the sheet is in place or gone on the next frame, with its scrim.

  The grabber, the title strip and a finger dragging the list down from its top pull it. `shell/panel.rs` puts the motion on a slot *around* the card, because `offset_y` shifts an element's children and not the element, so an offset on the card itself leaves its background behind. Above rest the pull is rubber-banded and stays inside the card's overhang below the window edge. A release dismisses when the pull, projected 120 ms ahead along the release velocity, passes `SHEET_DISMISS_DISTANCE` (96 px, or half a short sheet), so a fast flick dismisses and an upward flick rests. The gesture is a pure state machine in `shell/sheet_gesture.rs` (drag, fling, snap, list hand-over) tested with a fake clock. The curves are pure functions in `shell/sheet_motion.rs`, tested for monotonicity, easing, exact ends and continuity; `shell/sheet_motion_frame_tests.rs` renders a popup sheet on a 390 x 800 window in fixed 16 ms animation steps and checks the frames (open, each way to close, both releases, a turn half way up; `BINGUX_SHEET_TABLES=1` prints them). A list in a sheet scrolls by wheel and by finger; a finger drag at the list's top hands over to the sheet and back when the finger returns. The list ends with `sheet_bottom()` (`SHEET_BOTTOM` + `Dim::SafeBottom`) as scroll content, so its last row scrolls clear of the window edge. Escape and a press on the scrim close it. `controls/dropdown_rows_tests.rs` reads the frame the renderer drew and fails if any option row of an open dropdown (short and 200-row lists, popover and sheet, both schemes, with icons, with a typed query, in every frame of the opening) shows no label: its text, opacity and ink contrast against the row's own fill, and the pixels inside the label's box, are checked for every row, scrolling the list end to end.

A popup with `PopupFlags::INSET_VIEWPORT` shrinks the window behind it into a rounded card on black: the window scales to `INSET_SCALE` (0.96) and its corners take the `Card` radius, both in step with the popup's open and close. On a sheet the inset runs on the sheet's own clock and curve (`openness`), so the card shrinks as the sheet rises and springs back as it leaves. It also follows the pull: full at rest, it eases down in proportion to the drag and reaches zero at the dismiss distance, then springs back with the sheet on release. Desktop popups do not inset.

### Popup windows and window layering

Menus, dropdown lists, popovers and submenus are **windows of their own on Wayland** (`xdg_popup`), so a context menu can leave its window as it does in GTK. `shell/panel.rs` is still the one entry point: `overlays.show_menu`, `menu_button`, `Dropdown`, the colour picker and the Files popovers call `Panel` as before and nothing in a caller changes. `Panel` chooses a route when it renders (`popup_windows::backend()`):

| Surface | Route |
|---|---|
| Menu, context/tab/history/hamburger/path menu, submenu, dropdown list, popover | Popup window (Wayland, window not phone-sized) |
| `PanelKind::Choice` on a phone-sized window | In-window bottom sheet |
| Dialog, About, forms, chooser | In-window modal card (belongs to its window, as in libadwaita) |
| Text edit bar | In-window, pinned to the caret |
| Tooltips, hover cards | In-window: small, on every hover, no grab; a window per hover costs a surface and a graphics context. `tooltip_place` clamps them inside the window. |
| X11, headless snapshot, a compositor that refuses a popup | In-window, unchanged (one refusal switches the session) |

The popup window has its own Freya runner. `Panel` builds the card as before and hands it to the popup (`popup_windows::PopupContent`); the popup's root provides a `ContextSnapshot` of every context visible at the panel (tokens, direction, router, overlays) and renders it in an error boundary of its own. The opener keeps the clear shield (a press on it dismisses and is swallowed), the focus node and the menu's key handler: the renderer delivers a popup's keys to the window it grew from (sway sends them there; mutter sends them to the popup). The compositor's grab dismisses the popup on a press anywhere outside the app (`CloseRequested` then `on_dismiss`). A submenu is a nested `Panel` and opens a child popup anchored to its row (`ParentSlot`); closing a popup closes its children first (destroying a parent first ends the connection, so winit orders it too). `popup_windows::place` turns a `PanelTrigger` into the `xdg_positioner` request (anchor rectangle, anchor, gravity, flip/slide; the gap is part of the anchor rectangle) and has a reference implementation of the compositor's placement for tests. The card sits in a 36 px transparent margin that holds its shadow; the window geometry and input region are the card. `design/popup-windows.md` has the evidence, the sway proof and the vendored winit patch.

`layers.rs` is the one table of paint layers (`TAB_*`, `SLIDE_*`, `BACK_*`, `HEADER`, `POPUP_OUTLINE`, and the overlay levels `LEVEL_MENU < LEVEL_POPUP < LEVEL_DIALOG < LEVEL_TOAST < LEVEL_MENU_OVER_DIALOG`) and `WindowStack`, the model of how windows stack: a window opens above the last window of its parent's family, a parentless window opens on top, menus are above every non-popup of their family, closing returns to the window below, focusing raises the family root with its children, a modal blocks its ancestors. Popup windows register in it; `BINGUX_DEBUG_LAYERS=1` prints the stack on every change (`layers::dump_stack()`).

Which windows are children and which are independent toplevels:

| Window | Kind | Link |
|---|---|---|
| App window, a second Files window, the window a tab is torn off into (`app::open_window`) | Independent toplevel | none: it stacks, raises and closes on its own |
| The inspector (`WindowStack::open_tool`) | Child toplevel, kind `Tool` | `xdg_toplevel.set_parent` to the window it inspects (an inspector of an inspector hangs from that inspector): the compositor keeps it above its target and the stack raises and closes it with it. It never blocks the target. On a tiling compositor such as sway a window with a parent is floated; the harness rule `for_window [title="^Inspector"] floating disable` tiles it beside the app again |
| Menu, submenu, dropdown list, popover on Wayland | Popup window (`xdg_popup`) | `Parent::Window` / `Parent::Popup`: above its parent, closed with it, newest first |
| In-window sub-window (`use_window().open(popup(..))`: chooser, properties, form) | Modal child in `WindowStack` | `WindowManager` registers it under the popup it was opened from, or the window |
| A dialog or tool window that has to be a real window (`app::open_child_window(parent, ..)`) | Child toplevel | `xdg_toplevel.set_parent`: the compositor keeps it above its parent; `WindowStack` records the link |
| In-window dialogs (About, confirmations) | Part of their window | drawn at `LEVEL_DIALOG`; a menu opened from one is `LEVEL_MENU_OVER_DIALOG` |

Floating cards painted with `Layer::Overlay` are named in `layers.rs` (`PANEL`, `TOOLTIP`, `HOVER_CARD`, `INLINE_EDITOR`, `STATUS_PILL`, `VIEW_CARD`, `VIEWER_CONTROLS`, `DRAG_CARRY`), so a call site says what it is.

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
- **The bubble is as wide as its text, never as wide as what it hangs from.** On one line up to `Dim::TooltipMax` (280 px) or the window less its `PANEL_EDGE` margins, whichever is less; longer text wraps inside that onto at most four lines, then is cut with an ellipsis. The size comes from the hidden copy, so the wrap never reflows after the bubble is drawn. A text element wraps at the room its ancestors give it, and an element placed absolutely is given the room of the nearest ancestor that has a size of its own: a 36 px icon button, or the clipped cell of the tab strip's New tab button. So the bubble (and the hidden copy) sits in a *slot*, a `room` wide box of its own (`controls/tooltip.rs`, `slot`), and nothing it hangs from can narrow it. The bug this closed drew "New tab", "Close Home" and "Search" as one-to-three letters a line. Never lay a bubble, or any overlay that holds text, out directly under a sized control: give the overlay a width of its own. The text cache (`freya-core`'s `TextCache`) already keys a paragraph by its width, so it was not at fault.
- **One clock per window.** While a tip is up, and for 300 ms after the last one went, the next control's tip opens at once, in full, instead of waiting 500 ms. The tip it replaces stays as it was until the new bubble has its size and is drawn, and goes in that same frame: moving along a row of controls is one bubble that changes place, with no frame between them that draws nothing and none that draws two. After 300 ms with no tip, the 500 ms rest applies again. `WindowHost` provides the shared state (`provide_tooltips`); a tip with no host keeps its own.
- **Never over a menu.** No tip opens while a menu or dialog is open, and one that is up goes when it opens (`Overlays::menu_open`).
- **Scrolling takes tips down.** Content that scrolls moves under a resting pointer, so no tip is up while anything scrolls. Every `ScrollView` and `VirtualScrollView` tells its window, through one window-level signal (`ScrollActivity`, vendored in `freya-components`, provided by `WindowHost` and read by the tooltip hub), when its content moves: from a wheel notch, and from every frame in which the content has a new position (a glide, a scrollbar drag, the keys, a drag of the content). The window is scrolling from then until the content has stood still for 200 ms (`SCROLL_STILL`). A tip that is up goes in the frame the signal rises, with no fade. A tip that was waiting for the pointer to rest, or whose pointer arrives meanwhile, opens no bubble while it is set; when it clears, a tip whose pointer is still on its control waits the usual 500 ms rest (no 300 ms grace after a scroll) and opens. Tips subscribe to the signal only while they are up or waiting, so an idle tip does not render for it. A host that does not provide the signal (a test that mounts a view bare) never scrolls, as before.

`controls/tooltip_tests.rs` steps the headless app frame by frame with the pointer on real icon buttons: a label that changes while hovered (the bubble is centred on its control, at the same distance, in every frame), neighbouring controls (never two bubbles, the second at once), the wait coming back, a long label wrapping and flipping once at the window edge, and a menu taking the tip down. The taxonomy's Tooltip specimens (Feedback) are the same cases to try by hand.
`controls/tooltip_tests.rs` also scrolls with a tip up: it goes within two frames, none opens for a pointer that arrives while the content moves, and it returns once the content is still.
`controls/tooltip_layout_tests.rs` is the size rule: a 44 px icon button in the open, in a 40 px clipped box, in a scrolled box, in the tab strip (its New tab button), in a window corner and in a window narrower than the text, with five labels (`New tab`, `Refresh (F5)`, `Move to Trash`, a 76-character one, non-ASCII) and a 60-character one. For each it checks that the bubble is as wide as the text on one line (or the room, wrapped on two to four lines with at least three characters a line), one line high for a short label, inside the window and off its control, and that its pixels are painted. Every case runs a second time after the same text was laid out at 20 px in the same window. Wide, tall controls in a free place cannot see this bug; these can.

`scripts/tooltip-check.py` is the same rule on the real windows: with `BINGUX_UI_TOOLTIP_CHECK=FILE`, every capture the headless apps save first hovers each named icon button of its frame (`Snapshot::tooltip_findings`, `controls/tooltip_check.rs`) and records a bubble that wraps while narrower than the cap, a column of letters, a bubble outside the window or over its control. It runs every Settings page and popup at both sizes, every taxonomy page and the Files window (`bingux-files --snapshot`). Run it after changing a tooltip, an icon button or a header.

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

Picking a colour has two forms over one state model: `controls::colour_picker` is described below, under [The colour picker and the colour button](#the-colour-picker-and-the-colour-button).

### The colour picker and the colour button

**Two forms, one picker.** `colour_picker(value, on_change)` is the full view, inline. `colour_button(value, on_change)` is a control-high square holding the colour in a ring, which opens the same picker in a popover anchored to it on a desktop and in a bottom sheet on a phone. Both are in `bingux_ui::controls::colour_picker`, and the button builds a `ColourPicker`, it does not copy one. The colours they take and give are Freya `Color`s.

```rust
// Inline, in a page: everything, with an opacity strip.
colour_picker(colour(), move |next| colour.set(next)).alpha(true)

// In a row: the swatch that opens it. Live: the page shows the colour as it is dragged.
colour_button(colour(), move |next| colour.set(next)).title("Accent colour").live(true)
```

Builder, on `ColourPicker` (the button has the same `alpha`, `accents`, `presets` and `recent`):

| Call                         | What it does                                                                                                                                     |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `alpha(bool)`                | The opacity strip and box. Without them the picker only makes opaque colours, whatever opacity the `value` has.                                    |
| `accents(bool)`              | The system accent colours in the palette, named. On by default.                                                                                  |
| `presets(Vec<Color>)`        | The swatches after the accents, instead of `PRESETS` (white, a grey, black). `vec![]` for none.                                                   |
| `recent(list, on_change)`    | The recent colours the app keeps, newest first, and what to tell it when one is added (at most `RECENT_MAX`, 6). The app saves them where it likes. |
| `recents(bool)`              | The recent row at all. Its room is kept whether or not a colour has been chosen yet.                                                              |
| `original(Color)`            | What the preview's old half shows and a press on it restores. Default: the `value` the picker was first given.                                    |
| `fill()`                     | Fill the width it is given. A phone's picker does without being asked; a desktop's is `Dim::PickerWidth` (264 px).                              |
| `ColourPicker::height()`     | The height it is drawn at, for a popover or sheet that must know before it draws.                                                                 |

On `ColourButton` only: `live(bool)`, `title(text)` (the sheet's title and the button's spoken name), `dense()` (a tool's row: `Dim::DenseControl`, 24 px, not on a phone) and `enabled(bool)`.

**The model.** `Hsva` (hue 0 to 360, saturation and value 0 to 1, alpha 0 to 255) is the truth and the RGB colour is only ever produced from it, because RGB cannot say which hue a grey was on: the hue stays where it was when the saturation reaches 0, and the saturation when the value reaches 0. Every constructor clamps; a NaN is 0. `Hsva::editing(colour)` takes an outside colour in as an edit of the one being edited, keeping what it cannot say, and is the identity for the colour the picker itself just made, so a caller that echoes the colour back moves nothing. `hsl`/`from_hsl`, `with_rgb`, `to_hex(with_alpha)` and `edited_by_hex(text)` are the conversions; `from_float_rgb` brings a colour from outside the sRGB gamut in and says whether it clipped it (`Gamut`). `parse_hex` reads `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa` (the `#` optional, either case, white space ignored) or says why not (`HexError::{Empty, BadDigit, Length}`, each with a one-line `message()`); `style::colour_from_hex` reads the same forms. `Mode::{Rgb, Hsl, Hsv}` is the arithmetic of the numeric boxes: `fields(colour)` are the whole numbers shown, `edited(colour, box, number)` the colour after a number is typed, clamped to the box's range with the other boxes as they are shown. All of it is pure and tested over a grid of the RGB cube (the round trips through HSV, HSL and hex are exact for HSV and hex and within the rounding of whole percentages for the boxes). A `value` that is not what the picker is editing is taken in; while a drag is on it is not (the caller's echo is late).

**What is drawn.** From the top: the saturation and value plane; the hue strip; the opacity strip when asked; the preview (old half, new half, over a checkerboard; a press on the old half restores it); the hex field with the line under it that says what is wrong; the mode switch (`segmented(..).fill()`, which stays a rail on a phone) and its three boxes, with the opacity box as a fourth when asked; the palette; the recent colours. The group order and every length are `colour_picker/geometry.rs`, from the tokens: `Dim::PickerPlane`, `PickerKnob`, `PickerHandle`, `PickerSwatch`, `PickerWidth`, `SliderThumbLarge` and `SliderTrackLarge`, plus `Dim::Control`. Nothing in the picker changes size when a colour is chosen, an entry is wrong or the first recent colour arrives: the hex message, the letters under the boxes and the recent row are all kept free, and a test measures the drawn picker against `height()` at both densities after each of those.

- **The plane is one Skia draw**, not stacked boxes: a horizontal gradient from white to the pure hue multiplied by a vertical one from white to black, with dithering, painted into the plane's own rounded box (`painted`, `Fill::Callback`; the shaders are `shaders::linear_gradient` and `shaders::blend` with `BlendMode::Multiply`). That is exactly hue x saturation x value at every pixel, so a pointer's position and the pixel under it are the same colour (a test reads the frame back), there is no seam, and 8-bit banding is dithered. It is drawn when its hue changes and costs one rectangle. The plane has a 1 px inner edge (`Line::Hairline`) and the radius of a control; the handle's knob is a dark outline, a white ring and the chosen colour in the middle, which shows on white, on black and on every hue between.
- **The strips are the library `slider`** with `.large()` (a 14 px track and a 22 px thumb, 16 and 28 px for a finger), `.gradient_colours(..)` (a Skia gradient with alpha), `.checkerboard()` under the opacity strip, `.thumb_fill(colour)` (the thumb shows the hue it is on) and `.label(..)`. They are the controls every slider is, so their capture, keys and focus ring are the slider's; Shift now makes a slider key press ten steps, everywhere.
- **Targets.** The handle's target (`ColourHandle`) is `Dim::PickerHandle` across, 32 px on a desktop and 44 px on a phone, centred on a smaller knob (20 px and 24 px); a press inside it but outside the knob moves the handle there. (A target that overhangs the plane at an edge is not pressed beyond the plane's own box: the layout engine hits only what is inside the plane.) A swatch is `Dim::PickerSwatch` (32 and 44 px); the strips' rows, the boxes and the buttons are control-high. Pressing anywhere on the plane moves the handle there; the drag is a `controls::capture`, so it follows the pointer wherever it goes and is clamped to the plane.
- **Keys.** The plane: arrows move 1 %, Shift 10 %; Home and End go to no saturation and full saturation; Page Up and Page Down move the value 10 %. A strip: the slider's, in degrees on the hue (Shift ten, Home, End). Up and Down in a box step it, Shift ten. Tab runs plane, hue, opacity, the old colour (when it can restore), the hex field, the mode switch, the boxes, the swatches; Shift+Tab reverses it.
- **Assistive technology.** The plane and the strips are sliders named "Saturation and brightness", "Hue" and "Opacity", the plane with a value ("Saturation 62%, brightness 40%"); each swatch is a button named "Colour Blue, #3584e4" that says whether it is selected (and shows a check mark and a ring, so selection is not colour alone); the preview says both colours; the hex message is a status; the button is "Accent colour, #3584e4" and says whether it is expanded.
- **Direction.** A colour axis is not text: right is more saturation and more hue in every language and the strip's red is at its left end, so its pointer maths is physical, not a reading direction (a test holds the plane and the strips centred in the picker). Run the picker under `BINGUX_DIRECTION=rtl` before changing it.
- **Recent colours.** A colour is added when an edit of it is over: the pointer or a key comes up, Enter in a field, a swatch is pressed. With `recent(list, on_change)` the app owns the list; without it the picker keeps it in memory for the life of the process (`remembered()` in tests). The app has no settings store for them (`runtime::cached_persistent` is a cache of loaded pages, not preferences), so an app that wants them across runs saves the list it is told.
- **No eyedropper.** The one way to read a screen pixel from an app is `org.freedesktop.portal.Screenshot.PickColor`, which answers through a `Response` signal on a request object owned by the caller's own D-Bus connection. The settings app reaches D-Bus through `busctl` and `gdbus` child processes, and neither can hold a connection across a call and a signal, and this crate has no D-Bus client. So there is no button that does nothing; add one when `bingux-ui` has a connection to hold (a `zbus` dependency).

**The button's popover.** A press, Enter or Space on the button opens `Panel` (`PanelKind::Choice`: anchored on a desktop, a sheet on a phone, taller than a list's sheet with `Panel::tall`). On a desktop the content is the picker and a Cancel and Done row; on a phone the same picker under a Cancel, title and Done row (a sheet scrolls, and the buttons stay in reach). It closes on Escape, on a press outside, on Back and on its buttons, and it opens over whatever holds the button, a dialog included. One limit it shares with every popover and dropdown: Escape is heard by each open panel, so over a dialog it closes the dialog as well.

- **Commit on close** (the default): the picker edits a draft; `on_change` hears nothing until Done, which reports it once. Cancel, Escape, a press outside and Back drop the draft; nothing was reported, so there is nothing to put back, and the next opening starts from the colour the button holds. The recent colours are added on Done.
- **Live** (`live(true)`): `on_change` hears every colour as it is dragged (at most once a frame), so the page behind shows it. There is no Cancel, as nothing is held back; Done, Escape and a press outside close and keep what was shown.

The inspector's colour property (`inspector/props.rs`) is a live dense `colour_button` beside its hex field. The Settings accent row is not a colour picker: GNOME's `accent-color` takes one of nine names and nothing else, so it stays the named swatch row (`pages/appearance.rs`), which already has this picker's swatch look (a ring, a check mark, a name on hover).

The taxonomy pages Controls > Selection show both forms: the inline picker in each variation and at both densities, the button at rest, with opacity, disabled, dense, open (a popover; the phone sheet is in the Phone column of the same page when the window is a phone) and live. `controls::colour_picker::live_tests` measures every target at both densities, drags the handle with real pointer and touch events (it follows the pointer and clamps at every edge), checks the keys, the hex field, the boxes, the swatches and the recent row, and opens, closes, cancels and commits the button on a desktop, on a phone and inside a dialog.


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
| Toolbar | Back, Forward, Parent folder (a chevron up), Refresh, the address bar, the file type dropdown, New folder, a list or thumbnails switch and the view menu (sort, hidden files). Every button follows the `Control` radius like any other button, and the view menu opens against its own button, not at the pointer. Refresh (also F5 and Ctrl+R) reads the folder again even when no change was seen, as a network or removable drive sends none; the selection, scroll position and filter stay. While a folder is being read (a first load, the check of a cached listing, a read F5 asked for, a subfolder search) the same button turns into Stop: `icon_button(Icon::Refresh, f).morph(Icon::MediaPlaybackStop, stopping)`, so its box, hover, pressed and focus state carry through, and its tooltip and name are `Refresh (F5)` and `Stop (Esc)`. Stop shows after 150 ms of reading and stays at least 300 ms (`Dwell::LOADING`); pressing it, or Escape while it shows, stops the read (`Reload::stop`) and keeps the listing. See "Reload and Stop" in `docs/files.md`. |
| Address bar (`AddressBar`) | The path as crumbs (search is the bar under the toolbar, see [Search](#search-in-the-file-views)). The field takes the `Control` radius and a 4 px padding, so each crumb inside it takes the `Inner` radius (12 less 4). Each crumb has a chevron that lists that folder's subfolders. A trail that does not fit keeps its first and last crumb and hides the middle behind one `…` menu. Click the empty end, press Ctrl+L, or type `/` or `~` to edit the path as text. Typing shows completions (folders first); Tab completes, Up and Down choose, Enter opens, Escape closes. Errors show inline: not found, not a folder, no permission, not responding. `~`, relative paths and `file://` work. A **named place** shows its icon and name, never a path: Home, **Trash** (field `trash:///`, and `trash:///Old folder/` inside it, completed from the bin) and **Recent** (`recent:///`); typing any spelling of one (`trash://`, `~/.local/share/Trash/files`, `Trash`) opens it. See [File views](#file-views). |
| Sidebar (`PlacesSidebar`) | Recent, Home and the user folders (with their own icons by path), Bookmarks, then Drives: Computer and every mount, each with a usage bar and "X free of Y". A drive that does not answer says "Not responding". There are no buttons on the rows: every row has a context menu (right click, long press, the Menu key or Shift+F10) of text items, built in `files/place_menu.rs` from `PlaceFacts` by a pure `place_plan` that the tests read, in the list menu's grouping: open (Open or, for a drive that is not mounted, Mount; Open in new tab where the window has tabs), what changes the place (Rename… for a bookmark, Eject and Unmount or Unmount for a drive, Empty Trash on the Trash, grey while it is empty), the path and bookmark (Copy path, Add to or Remove from bookmarks), then Properties. Properties opens the list's Properties dialog for the place's folder (`properties::show_folder_properties`; the Trash shows its item count and size, a drive its free space bar); Recent and a drive that is not mounted have none. The menu's shortcut hints are keys the focused row answers to through the same command handler: Enter, Alt+Enter, F2, Delete, Ctrl+Shift+C. The folders have their own icons: a folder with a page for Documents, a down arrow for Downloads, and so on. A phone gets `PlacesMenu`, a sheet with the same rows that closes before Properties opens. |
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

**Context menus of the file view.** A menu is grouped by task and holds only what can apply: a row that can never apply in that place is left out, not greyed. The empty-space menu is, in order: New folder, New file (a submenu of Empty file and the files of the XDG Templates folder when that folder has any, otherwise one row); Paste (greyed until something is copied, left out in Recent) and, only when there is something to take back, a named Undo such as "Undo rename" (there is no Redo); Select all and Invert selection (left out in an empty folder); one View submenu; then Refresh and Properties. View holds List and Grid, Icon size (the sizes of the view on show, from the zoom ladder), Sort by (Name, Size, Type and Modified; Ascending and Descending; Folders first) and Show hidden files. Choices are ticked rows in one leading column, so labels line up. The header menu's Sort by is the same submenu. Shortcuts show on the right.

**Preview (Quick Look).** Space on the selected file shows it in a popup over a dimmed scrim, as macOS Quick Look and GNOME Sushi do, whether or not a pane is docked; the context menu has Preview too, and the chooser dialog has it as well. Alt+P or F3 docks the same preview as a pane at the right edge of the list, where it follows the selection and the list keeps the keyboard; the header button, or the same keys, undock it again, and the pane's edge drags it wider (the width is remembered). A window narrower than `MIN_WINDOW` (600 px) has one frame, a sheet over the window; F goes full screen. One component, `PreviewHost::new(source, mode, nav)`, is every frame (`preview::open(overlays, source)` shows the popup; `PreviewDock::new(open, row_area, source, nav)` is the pane, always mounted so it can slide), and it picks the viewer from the file's type. Space only opens it when the file view itself has the keyboard: never while a rename field, the address bar or a filter has it, and not in the middle of a word typed to jump to a file. Everything loads off the UI thread; the stage is a fixed box with the type's icon as placeholder, so nothing moves when the content arrives, and stepping to another file drops the load of the last one.

**The arrows choose the file.** In every frame and in every viewer, Left, Right, Up and Down (and Alt with an arrow) move the file view's selection and the preview follows: `keys::route(key, modifiers, grid)` has no viewer parameter, so no viewer can take an arrow, and Up and Down step a line in a grid. They never scroll, seek or pan; those are the wheel and the touchpad, Page Up, Page Down, Home and End, J and L, and Shift with an arrow. A popup has the keyboard from the moment it opens; Space and Escape close it and `on_closed` returns the keyboard to the list. A docked pane leaves the keyboard with the list; a press in the pane gives it the pane's own keys (`PreviewNav::pane_keys(state)`, the state lives in the file view's `Ctx`), a press anywhere else or Escape takes them back (`on_release` focuses the list), and while it holds them the pane steps the list's selection itself (`on_step`), so the arrows work after a click in the pane exactly as before it. In a pane that holds the keys Space and Shift+Space page a document and play and pause a film.

**What each type shows**, under one slim title line (icon, name, "5 of 14", then Open, Info, dock or pop out, Close; no footer, the facts are behind Info, I): a **picture** scaled to fit with its proportions kept (zoom buttons appear while the pointer is on it; `+`, `-`, `0`, `1` zoom, R rotates); the first 64 KiB (4 MiB once scrolled) of a **text, code, markup or JSON file** in plain monospace, with a notice only when the preview stops early (W wraps lines); a **folder**'s item count and total size, counted while it is on show; an **archive**'s listing; a **font**'s specimen; a **film or song**, played; a **PDF**, rendered a page at a time; anything else as its icon, its facts and Open. Enter closes the popup and opens the file as a double click does; Alt+Enter or Ctrl+I closes it and shows Properties. In the pane Escape gives the keyboard back to the list.

**The player.** A film or song is decoded by the helper `bingux-preview-host` (GStreamer) and drawn letterboxed on black with the control bar floating over it: play and pause, a seek bar with the buffered range ahead of the thumb, the position and the length, volume with mute, the speed menu, and subtitles when the file has a track or a `.srt` or `.vtt` named like it; full screen is the F key. A song is the file's card with the same bar. The bar fades in on pointer movement or a key and out 2.5 s later while playing; while paused, hovered or failed it stays. **Starting** is decided by who asked: the popup was asked for this file, so it plays at once, with sound; the pane follows the selection, so it never makes a sound by itself: it shows the first frame with a centred play button and plays on a click, and what the person set the volume to is kept for the next file. Keys (while the player has the keyboard): K plays and pauses (Space does in a pane that has been clicked; in a popup it closes); Shift with Left or Right seeks 5 s; J and L seek 10 s; 0 to 9 jump to that tenth; Home to the start; Shift with Up or Down the volume; M mutes; `[` and `]` change the speed; C cycles the subtitles. In a pane the bar is compact (play, position, seek, mute) and a bar given no width measures itself and picks its form. The player is `Playback` in `preview/viewers/media.rs`; its bar, `MediaControls::new(ControlState, on_action).docked(bool).width(w)`, is public and shown in the taxonomy.

**The PDF viewer.** The same helper opens a PDF with poppler and answers four questions: where each page is and how big, a page rendered at a scale into shared memory, the words of a page with their boxes, and a search. The viewer opens **fitted to the page** (the whole first page, centred; a stack shorter than the viewport sits in the middle of it), keeps the pages in one scrolling column, draws only those near the viewport at the scale on screen (times the display scale), shows the nearest scale it has until the right one arrives so a zoom never flashes white, and cancels the renders that scrolled away. The toolbar (`DocumentControls`) is the page field and "of N", zoom out, the zoom menu (fit width, fit page, percentages, in a slot as wide as "Fit width" so nothing moves when the label changes), zoom in, and find, the same in a popup, a pane and a phone sheet; a menu opened from the toolbar is drawn above the popup (the window's menu layer is raised above an open dialog). Keys: Page Up, Page Down, Home and End scroll; `+`, `-`, `0`, `1` and `9` zoom (in, out, fit width, 100 %, fit page); Ctrl and the wheel zoom, Shift and the wheel scrolls sideways; Ctrl+F finds (Enter and Shift+Enter, or F3, step through the hits, Escape closes the bar); F9 the thumbnails; dragging over the text selects words and Ctrl+C copies them.

**The helper.** `bingux-preview-host` is a separate binary so GStreamer and poppler stay out of the application. `HostLink` (`preview/host_link.rs`) starts it on a socket pair with the file as a descriptor, ties its life to the application (`PR_SET_PDEATHSIG`), gives it only the environment it needs, bounds its queue, gives it 15 s to open a file and 600 ms to leave, and takes only the newest frame, so a slow window skips pictures and never queues them. Nothing here waits on it. A helper that is missing, crashes, times out or cannot decode the file is the file's card with the reason, Open, and Try again where a retry can help; the window is untouched. A process runs at most `MAX_LIVE` (2: a pane and a popup) helpers and starting a third ends the oldest ("stopped to make room"); an app that shows many previews at once, as the taxonomy page does, raises it with `preview::allow_helpers(n)`, and a preview in the middle of a page sets `PreviewNav::takes_focus(false)` so that its popup does not take the keyboard and scroll the page to itself. The program is found the way the packages lay it out: `BINGUX_PREVIEW_HOST` (a file that must exist), else beside the application (`/usr/bin`, or the `bin` of an install prefix), else on `PATH`; a debug build run from a cargo target directory also finds the one beside `deps/`. `preview::host_available()` says whether it is there; without it films, songs and PDFs show their card and a note.

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

Search has its own bar under the toolbar, not the address bar. The address bar
holds a place (trail or typed path) and nothing else. User-facing behaviour is
in `docs/files.md` ("Search"); the design, numbers and gaps are in
`design/files-search.md`. This is how an app builds it.

```text
toolbar
[ SearchField ........... ]  [ Search in: dropdown ]  [ Filters (n) ]     SearchBar row 1
[ token x ] [ token x ]  Clear all                      status  [Stop]     SearchBar row 2
list or grid, with a Location column for results
```

**State.** `Find` (`files/search.rs`) is the search a tab is making apart from
its words: `open`, `scope` ([`Scope`](bingux_files::find::Scope): This folder,
Subfolders, Home, Everywhere, a place), `filters`
([`Filters`](bingux_files::find::Filters)), `filters_open` and a focus counter.
The words are the text of the field, kept by the owner (`words`, a
`State<String>`). `Find::query(words)` is the typed
[`Query`](bingux_files::find::Query) of the two together. A `key:value` typed
in the field is lifted into `Find::filters` when a space ends it
(`Find::lift`), and the field is given back the words that are left, so the
text and the Filters panel never disagree; a key still being typed
(`type:imag`) is not part of the query (`find::split_pending`).

**Pieces.**

| Piece | What it is |
| ----- | ---------- |
| `SearchBar` (`files/search_bar.rs`) | The bar. Row 1 is the library `search_field` (with its new `on_arrow`), a `dropdown` for the scope (the user folders and bookmarks of the sidebar after the four core scopes) and the Filters button. Row 2 is the tokens (`FilterToken`: caption text on `AccentSoft`, a `Dim::InlineButton` remove button) and the status line (`status_text`, a polite `Status` region, Stop while it runs). On a phone the field has a row to itself and the scope and Filters share the next. |
| `FiltersButton` (`files/search_filters.rs`) | The button (`Filters (2)`) and the panel it opens: a `Panel` of `PanelKind::Choice`, so a popover on a desktop and a sheet on a phone. Rows of label and `dropdown`, a range shows two `text_field`s, three `switch` rows, Reset and Done. Every row is `Dim::Control` tall, so the panel's height is a sum of tokens (`form_height`). |
| `use_search_run(RunKey, mount) -> Deep` | Runs `bingux_files::find::run` on a thread of its own while a search of folders is on show, publishes every 60 ms, cancels when the key changes, on Stop, or when the component goes. `Deep::latest`, `running`, `stop`. |
| `select_shown` (`files/chooser.rs`) | The rows: the folder narrowed in memory, or the results (arrival order while running, the chosen order once). |
| `no_results_state` (`files/states.rs`) | "Nothing found" with a button per way out. |

**Rules.** The first row never changes size while results stream; the second
row's height is fixed unless tokens wrap, which only a person's own filter
choice causes. Results are appended while a search runs: nothing already on
show moves, whatever the sort; the list is ordered once when the search ends.
The previous results stay until the new search has its first. Column headers
sort results (a second press goes back to relevance); the header marks nothing
while the order is relevance. A navigation, Escape in the list, or Escape in an
empty field ends the search; the scope is kept.

**Keys.** Ctrl+F opens or focuses; Ctrl+Shift+F toggles the panel; Enter opens
the selected result or the first; Down moves to the list; Up steps up in the
field; Escape clears, then closes. With the field empty and focused the recent
searches drop down (`Recents`, eight, in `$XDG_STATE_HOME/bingux/files/recent-searches`).

**The backend** (`bingux_files::find`, UI-free) is described in
`design/files-search.md`: `Query` (parse and print, tested by round trip), `Matcher`
(tiers, filters, `Rank`), `NameIndex` (the names under home in `bingux-search`),
`walk` and `run`. A chooser's file type filter (`Filter`) is a different thing,
it still applies to results, and neither type holds the other.

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

A `FileIcon` is data: the `glyph` (its symbolic stand-in), the `kind`
(`FileKind`), the `colour` icon it is drawn as (`ColourIcon`; `None` for a drive
or Recent), an optional `emblem`, a `tint` and `thumbnail`, where a picture of
the file may come from (the view replaces the glyph with it when it can; a
folder never has one). `FileKind::of_mime` is the one classification of a MIME
type, shared with the file-type rows in Settings. `FileKind` has 23 kinds, the
families below; `FileKind::colour()` is the colour icon of each, one to one.

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
- A **file** is its MIME type's kind: image, audio, video, text, code, data
  (JSON, YAML, TOML, desktop entries), markup (HTML, Markdown, XML, TeX), PDF,
  archive, disk image (ISO, IMG, DMG, QCOW2), spreadsheet, presentation,
  document (word processor), other document (PostScript, calendar, DjVu),
  e-book (EPUB, MOBI, comic books), font, program (and shared libraries,
  AppImages, `.exe`), 3D model, database (SQLite, KeePass), certificate or key
  (PEM, PKCS, OpenPGP, SSH), torrent, or unknown. The families are tried from the
  most specific, so an EPUB (a ZIP) is an e-book. A type's own name decides
  first (`application/pdf`); the extension table's coarse category decides when
  the name says nothing; an extension-less file that is executable and of no
  known type is a program. `bingux_files::mime::known_extensions()` lists every
  extension the table classifies; a test walks them all and fails when one has
  no colour icon.
- A **drive** has the glyph of its kind (disk, removable, network, optical).
- A **symbolic link** keeps the glyph of what it is and gains the link emblem
  only on a glyph of 64 px or more (`EMBLEM_MIN_PX`): a small disc with an arrow
  cut out, a quarter of the glyph, ringed by the page colour, over the lower
  right corner. Smaller glyphs (list rows, sidebar, crumbs, menus) are left
  whole; the link is told by the hover card, Properties and the status pill.
  Only a listed entry carries it: places and crumbs never read the disk.
- A **hidden** file or folder is drawn faint.

**Which colour.** Files and folders are **colour icons** (`files/colour.rs`), a
second set beside the symbolic one, drawn in the language of the Bingux Files
app icon (`apps/files/bingux-files.svg`) and following the theme.

*The design language.* Every shape is rounded by one radius ratio (about one
eighth of the width: 2 px at 16, 2.5 at 20, 3 at 24, 4 at 32, 14 at the 128
grid), the folded corner and the folder's tab included; the light falls from
above (a pale sheen on the top edge fading half way down, a soft shade at the
foot), so an icon reads as a lit surface and not a flat tile; a **back layer**
is darker than the **front layer** (the folder's tab and back behind its panel;
a bin's lid over its can); a thin white highlight runs along the top edge of a
front panel, and a 1 px lip under it; a sheet of paper stands in the folder
(from 32 px) with a line of text on it; glyphs have round caps and joins on the
vector and sit set into the face. A pixel grid keeps its straight edges on whole
pixels and turns only its corners, so a 16 px icon stays crisp and is still
clearly a folder or a page; from 40 px up the vector draws every curve.

- A **folder** (the plain folder, the user folders, the home folder, the Trash)
  is the user's **accent colour** (`Tokens::accent`), always: resting, selected
  or hidden, in the list, the grid, the sidebar and the address bar, and it
  changes with the system accent setting. The front panel shows the accent on
  the dark scheme (whose accent is already pale) and a paler lift of it on the
  light one: the body is lifted by a white layer (`FACE_LIGHT`, 28 %) with the
  sheen on top (`LIT_TOP`, 32 %), pale like the app icon's, and the back and the
  tab are the body under a black layer (`SHADE`, 22 %). Its glyph (Documents, Downloads,
  Music, Pictures, Videos, Desktop, Home, Templates, Public) is a deep tone of
  the accent set into that panel, moved until it reads 4.5:1 on it. The Trash
  is a bin: closed and faint while empty, paper over its rim when it has items.
- A **file** is a rounded page with a rounded folded corner in the colour of its
  family, one hue per family (`ColourIcon::hue`), and a glyph; from 24 px up the
  glyph of text, spreadsheets, presentations, PDFs, documents, archives and disk
  images is a label of two to four letters (`TXT`, `XLS`, `PPT`, `PDF`, `DOC`,
  `ZIP`, `ISO`): a pixel font at 24 and 32, rounded strokes on the vector.
  The page is OKLCH lightness 0.76 on the dark scheme (a deep ink of its own hue)
  and 0.52 on the light one (white ink; the yellows, hue 70 to 128, start 0.28
  lighter and 1.3 times as saturated there and take a deep ink: darkened to
  stand off white at 3:1 they went olive and brown), at the family's hue and
  chroma, with the sheen at the top (`LIT_TOP`) and a shade at the foot
  (`DEEP_BOTTOM`, 10 %). Its silhouette is held by a **rim** (`RIM`, 26 % black
  over the page, 1 px wide inside the outline, 3 px on the vector; the compiler
  draws it as an outline stroke clipped to the page): the rim, not the body,
  must stand off every ground, so a light body can keep its hue and the body
  itself only has to stand apart by `PAGE_BODY` (1.4:1).
  The body is moved along lightness until every edge of the picture shows on
  every ground a row is drawn on (the five layers, and each under hover, press
  and selection; `edges`, `edge_limits`): a page's rim over its body and over its
  foot (under the shade) at 3:1 and over its top (under the sheen) at 2:1; a
  folder's panel at 2.2:1, its top at 1.5:1 and its back at 1.5:1, since the pale
  panel holds a folder's silhouette. The ink reads 4.5:1 under the share of sheen
  and shade a glyph sits in (`glyph_grounds`). `colour_tone(icon, &tokens)` gives
  the two colours (`Tone { body, ink }`); a test checks every family and every
  accent in both schemes, and that the yellow families are light and saturated
  on the light scheme. `colour.py --sheet DIR --tones FILE` draws contact sheets
  in these tones (`BINGUX_COLOUR_TONES=FILE cargo test -p bingux-ui --features
  testing --lib the_tones_can_be_written` writes FILE).
- The colour is the same selected or not; the selection fill is one of the
  grounds the tone is checked against. A hidden file or folder is drawn at half
  strength until it is selected.
- **Drives, Recent and the drag chip** keep the symbolic glyph: muted, the
  accent only when selected, fainter when a drive does not answer (and
  `file_icon_view_floating` uses the ink that reads on the floating surface).
- The SVGs hold roles, not colours: the body is the viewer's `fill`, the ink is
  `currentColor`, and the depth is white and black laid over the body: the
  gradients `lit` and `deep` (white fading out, black fading in), the flat
  `shade` and `light`, and a near-white `paper` for a sheet. The viewer sets
  two colours; the overlays are neutral, so the same file serves every accent
  and both schemes. One viewer, one raster, cached by (icon, size, colours).

**The colour set.** The sources are plain SVGs in `scripts/icons/colour/` (34:
22 file families, 10 folders, the Trash empty and full): the pixel art on a 16
grid, plus a `<g data-grid="128">` with the same glyph drawn smoothly on a 128
grid for large sizes. `scripts/icons/colour.py` compiles each for five grids,
16, 20, 24, 32 and 128, into `crates/ui/icons/colour/<name>-<grid>.svg`.
The chrome (the page, the folder, its paper and lip) is drawn by the compiler
from per-grid tables (`PAGE`, `FOLDER`, `RADIUS`); on the pixel grids
rectangles are on whole pixels, strokes on the pixel centre (odd widths) or the
pixel edge (even), nothing warped or thickened, and a label is in a 3 x 5 (24
grid) or 5 x 7 (32 grid) pixel font; the 128 grid is copied as authored. An icon
of 40 px or less is drawn from the pixel grid whose whole multiple is its size
(`colour_grid(px)`): 16, 20, 24 and 32 px and their 2x are on the lattice, 40 is
2 x the 20 grid; a larger one (48, 64, 96, 128, 192, 256) is the vector, drawn at
any scale. The colour icons do not go through `build.py`'s fit and weight policy
(they are not tinted glyphs): `scripts/icons/colour.py --check` (in `make
icons-check`) is their check: sources valid, output current, `colour_icons!` in
`colour.rs` matching the sources, every pixel shape on the lattice, every
coordinate inside its box, only role colours; `scripts/icons/golden.py` holds
their pixel grids to the approved drawing. `scripts/icons/colour.py --sheet DIR`
writes contact sheets at 1x and 2x. To add one, write
`scripts/icons/colour/<name>.svg` (`data-chrome="page"` for a file, `"folder"`
for a folder, `"none"` for the bin; shapes use `ink`, `body`, `shade`, `light`,
`lit`, `deep` and `paper` fills; a rect may have `rx`), add a line to
`colour_icons!` and a `ColourIcon` variant, map it from a `FileKind` or a
special folder, and run `scripts/icons/colour.py`.

A list row has the same horizontal padding, icon column and gap at every
density (`FILE_ROW_INSET`, `columns::row_padding`); a phone's row differs in its
height (56 px), its two lines and its icon: a folder opens at
`default_zoom_px()`, 16 px on a desktop and 32 px on a phone (the last list
level of the ladder, the one that fills the 56 px row and stays a list), and the
name starts the same distance after the icon as on a desktop.

**Adding a glyph.** If a kind or special folder needs a glyph the set lacks, add
it through `scripts/icons` (see the icon set above): a row in `labels.tsv` with
where it is used, `build.py --check` passing, then the `Icon` variant. Never draw
a surface's own glyph.

The inspector shows each icon as `FileIcon` with its glyph, colour icon and
size; the taxonomy page "File icon" shows every kind in colour at 16, 20, 24,
32, 64 and 128 px, resting and selected, the folders in the accent (resting,
selected, hidden, the Trash empty and full), the folders a path decides, places
and drives, the colour set by name, and the emblem (at 64 and 128 px).

### Layers

| Layer | Where | Depends on |
| ----- | ----- | ---------- |
| Filesystem model: listing, MIME, places, bookmarks, recent files, filters, the file search (query language, matcher, names index, walker), sort, inotify watching, selection, request and answer types, what accepting does | `crates/files` (crate `bingux-files`, `libc`, `regex` and `bingux-search`, unit tests) | nothing |
| Views: `PlacesSidebar`, `PlacesMenu`, `AddressBar`, `FileView`, `use_directory`, `use_places`, `empty_state`, `error_state` | `bingux_ui::files` | the model, bingux-ui controls |
| The dialog: `choose` | `bingux_ui::files` (also in the prelude as `files`, `ChooserRequest`, `Chosen`) | the views |
| The Files window: `browse` | `bingux_ui::files`, hosted by `apps/files` | the dialog's body |

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
.on_pick(..).on_activate(..)`. The Files app (`apps/files`) uses them as they are, through `browse`.

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
Timings: `crates/files/examples/bench.rs`.

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

### Tab switching contract (`browse`)

`browse(Browse::new(tab_memory))` is the chooser's body for a window with tabs.
The host mounts it **once** and passes the `TabMemory` of the tab on show; it
must not key the body by tab. The contract:

- The body keeps the states of every tab it has shown (`TabBank`: history,
  selection, sort, icon size, hidden files, preview pane, typed path, search)
  and reads the ones of the current `TabMemory::id()`. Switching is a change of
  which `State`s are read; nothing is written while the new tab renders, so the
  toolbar, crumb bar, sidebar and status line neither remount nor re-measure.
  The states of a tab are let go of when the host drops its last `TabMemory`
  clone (a closed tab kept for Reopen keeps them).
- The file view is keyed by tab and folder (`FileView::render_key`). It starts
  from `scroll_memory::measured(tab)` (the viewport and pane row the tab's last
  view had) and passes the viewport to `VirtualScrollView::initial_size`
  (vendored patch), so the first frame has the rows, the scrollbar, the column
  plan and the pane width of the settled frame. Selection and scroll come back
  from `scroll_memory::recall`.
- `use_directory` returns a `Directory`: `read(&place)` is the load of that
  place, else the last whole listing the process had of it (a bounded in-memory
  list, 16 folders / 60,000 entries), else `Load::Loading`. The body derives
  the rows from what the render read (no `use_memo`, which lags a pass). A
  folder not read yet is the file view with an empty listing and a placeholder
  in the rows' height, never a different layout.
- Tests: `files::tab_tests` (pass-by-pass) and `scripts/files-shift.py` (real
  window, every frame).

## Search

`bingux_ui::search` is the search every Bingux app shares. An app describes
what can be found, registers it once, and asks for hits as the user types.
It writes no matcher: typo tolerance, ranking, highlights, filters, facets and
Spotlight come with the SDK. The engine is the UI-free crate
`crates/search`, re-exported as `bingux_ui::search`; the design and
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

### Searching files is not this, but uses it

The file view's search (`bingux_files::find`, see [Search in the file
views](#search-in-the-file-views)) does not register a `Searchable`: file
names are not records an app owns, and the crawl and the swap of a fresh index
for the old one are its own. It does use the engine: the names under the home
folder are an `Index` of `bingux-search`, which answers a search there before
the walker has read a folder. The engine is for records an app owns (settings,
applications, contacts, notes, devices), up to about a million short
documents.

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

**Donut.** Unlike the other charts a donut has no fixed height: it is its ring
and its legend rows, so it depends on the width it is given. The ring is
always a square (it never stretches to an ellipse) and
its thickness is 11 % of its size, so the proportions hold at every size.
Where there is room (the regular density, and at least 344 px of card
content: 128 px of ring, a 16 px gap and 200 px of legend) the legend sits
beside the ring, which is 128 px. Anywhere else (a phone, a narrow card, or
text scaled to 200 %, which needs twice the room) the ring is as wide as the
card's content up to 200 px, centred, and the legend is under it with
full-width rows. The legend then has exactly the card's own padding, 16 px at
the regular density and 18 px at the compact one, and a chart never makes a
page wider: it is built to hold at 320 px. A legend row is 44 px tall at the
compact density, a finger's target, and 36 px at the regular one. A long name
wraps onto a second line, at either density, and is cut with an ellipsis after
that. When the numbers of a row (value and share) would leave its name under
about five letters of room, as at 200 % text in a narrow card, every row puts
them on a second line under the name.

The centre text is fitted to the hole, not to the ring: the value and its
caption shrink together (to 55 % at the least) until each is narrower than the
hole is at its height, and the caption is dropped before the value is made
unreadable. The ring grows with the text scale (up to 2), so large text keeps
its size when the card has the room. A segment is chosen by hover, or on touch
by a tap on the ring or on its row (a second tap, or a tap in the hole,
clears it): the others dim, the legend row stays lit, and the centre reads that
segment's value and its share of the whole. A finger that swipes over the ring
to scroll the page chooses nothing. A tap counts anywhere in the band between
the middle of the hole and the outer edge, not only on the stroke, because the
stroke is thinner than a finger. `ring(value)` is a gauge with the same
centre-text fit but a fixed 120 px box.

The taxonomy's Charts page shows the donut in 360 and 320 px columns, with
long names, with seven segments and with no data, in both schemes and at both
densities; `charts/donut_tests.rs` holds the layout tests (square ring, text
inside the hole, legend inside the card, nothing wider than the card, tap and
hover) at 390, 360 and 320 px.

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

Row values are drawn with `select_text` (the shared selectable-text control,
see Selectable text below), so a user can select them with the mouse and copy
them with Ctrl+C or a right-click Copy. This covers the `value`, `mono_value`
and `path_value` of `row` and the value of `property_row`. A value is never cut
with an ellipsis: the row wraps it or stacks it under its title (see Info rows).

### Selectable text

`select_text(text)` (module `controls::select_text`) is the one control for
text worth copying: a value (a path, an address, a version, a hash, a licence),
a block of licence text, error details. It wraps Freya's `SelectableText`:
a drag selects, a double click selects a word, a triple click a line, Shift and
the arrows extend, Ctrl+A selects all, Ctrl+C copies the selection through the
window's clipboard. The control adds:

- The selection is drawn in `Paint::Selection` (the vendored `SelectableText`
  gained `highlight_color`; upstream draws a fixed blue).
- A right click, or a touch that rests for a long press, offers **Copy** (the
  whole text; `copy_label("Copy path")` renames it). The long press watches the
  window's touches, because a selection stops the pointer-down a touch starts
  from.
- Not a Tab stop unless `tab_stop(true)` (the licence list sets it). Role
  `Label`, label the text.
- It wraps between words and never cuts. `breaks_anywhere()` declares a text with
  no space to break at (path, hash, device id, MIME type); the `BreaksAnywhere`
  component is what the Layout checks read, so a word broken in the middle is a
  finding unless declared.
- A selection is a fill behind the glyphs and moves nothing.
- In a row that presses (`on_press`, `navigates`) the text is plain, so the
  selection does not take the press; Copy on a right click still works.

The About window's licence list and licence text are columns of `select_text`
blocks that are Tab stops. A selection does not run from one block to the next
(the list is cut into blocks of 40 lines for the text engine). Tests replace the
clipboard with memory (`select_text::test_clipboard::install()`): the system's
belongs to the person at the desk.

`TextStyle::selectable` is the one path for selectable text that wraps. It
backs the `centered_caption`, the status block, the text field's invalid
message, and the subtitle of a row or level row that does not press. A
selectable text takes the pointer-down that a press starts from, so a row with
`on_press` keeps its subtitle plain: its press keeps working, and the subtitle
cannot be selected there. While a search marks words in a text, it is a
paragraph with the marks instead, so it is not selectable until the search
clears.

## Direction

A window reads left to right or right to left, and the engine does the mirroring, as a
browser does for `dir="rtl"`. `design/rtl.md` has the rules and the reasons.

- **Where it comes from.** `Direction::system()` is `BINGUX_DIRECTION=rtl|ltr` when set, else the
  first of `LC_ALL`, `LC_MESSAGES`, `LANGUAGE` and `LANG` (Arabic, Hebrew, Persian, Urdu and the
  other right-to-left scripts read from the right). Every window is rooted in
  `window_directed(Direction::system(), ..)` (the window boundary does it; a snapshot does it).
  `directed(direction, child)` reads a subtree in another direction: a specimen, a test.
- **What the engine does.** The vendored Torin has `Node::rtl`: a node's stacked and absolute
  children are reflected about its box, so a row runs from the right, a column's children align to
  its right edge, a start padding or margin and an absolute `left` are on the right, and a scrolled
  content starts at its right end. freya-core inherits a `TextDirection` (`.text_direction(..)`):
  paragraphs are shaped right to left (a trailing `.` falls on the left, `TextAlign::Start`, the
  default, is the right edge, and a paragraph with a minimum width, as a text field's text has, is
  aligned inside that width), borders, corner radii, shadows and linear gradients are drawn
  reflected, and a caret in an empty line is placed by the resolved alignment. Write code for left
  to right and read it as start and end: `pl` is the start padding, `Align::Start` the start. The
  flag follows the inherited direction through a replaced element and a changed direction.
- **What the code does.** What the engine cannot know is physical: a pointer, a canvas, a key.
  `use_direction()` is the direction the subtree reads; `Direction::left_edge`, `sign`,
  `reading_key` (Left and Right swap, so a control's `ArrowRight` arm is "towards the end") and
  `along` are the helpers. Slider, split pane, stepper, segmented control, tree view, menus,
  disclosure rows, the back swipe (from the right edge), popup alignment and the Files key handling
  read them.
- **Icons.** `Icon::mirrors_in_rtl()` (`#mirror_rtl` in `scripts/icons/labels.tsv`): go-next,
  go-previous, pan-end, sidebar-show, send-to, view-list, help-browser, emblem-symbolic-link. A
  test keeps the Rust set and the directive equal and `build.py --check` reads it.
- **Not mirrored.** Charts and graphs (the plot is drawn in pixels, left to right, as Material's
  bidirectionality guidance has it), the media bar, clocks and numbers (digits stay in Latin
  order), a colour axis, checks, logos and circular arrows.
- **Pseudo-localisation.** `BINGUX_PSEUDO_LOCALE=ar|he` turns every letter of a visible string into
  Arabic or Hebrew and keeps digits and punctuation.
- **Checks.** `layout-lint.py --rtl` (in `make layout-check`) captures each page twice and reports a
  `direction-asymmetric` finding for a box that is not the reflection of where it is left to right;
  allow one in `ALLOWED` with the reason. `controls/direction_tests.rs` renders a box, border,
  radius, shadow, gradient and row in both directions and compares the pixels; it also guards that
  a node whose element is replaced keeps mirroring its children and that a subtree that changes
  direction is laid out again.

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
| `Dim::SliderThumbLarge`             | 22 px           | 28 px           |
| `Dim::SliderTrackLarge`             | 14 px           | 16 px           |
| `Dim::PickerPlane` (height)         | 168 px          | 200 px          |
| `Dim::PickerKnob`                   | 20 px           | 24 px           |
| `Dim::PickerHandle` (target)        | 32 px           | 44 px           |
| `Dim::PickerSwatch`                 | 32 px           | 44 px           |
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
- **The search field** (`search_field(value, on_change)`, `SearchField`) is the
  one control for a filter, a find bar and a "search for" well; no screen draws
  its own. It is a `text_field` with the search glyph in a **square slot as
  wide as the field is tall** (`Dim::Control`: 36 px at mouse density, 44 px at
  touch density; `dense()` 24 px; `flat()` `Dim::FlatField`), the glyph at
  `IconSize::Control` centred in it on both axes, so the glyph's centre is the
  slot's centre by construction, not by a margin. The text starts `S1` after the
  slot. While the field has text, the clear button sits at the trailing edge in
  the same kind of square slot (the text field's inline button, centred on the
  field's height and as far from the end as the glyph is from the start). The
  accessibility role is `SearchInput`. Escape clears the text, reports the empty
  text through `on_change` and keeps focus; with the field empty it leaves the
  field, and with `on_escape` set it is the owner's (a find bar closes with
  it). `on_submit` runs on Enter. The placeholder is `Search` unless
  `placeholder(text)` says what is searched (`Search pictures`). `well()` is the
  elevated fill for a page or sidebar, `fill_width()` fills the container,
  `enabled(false)` greys it out and takes it out of focus order. A field under
  the pointer takes the control hover layer; focus is the ring. The clear button
  acts on left pointer-down: a press on the Freya input's own handler does not
  reach a child's `on_press`. Any `text_field` with `leading(icon)` (the Files
  address field's folder glyph) gets the same slot; the layout lint's
  `icon-not-centred` rule (see Layout checks) holds every such slot and every
  icon button to it. Scrollbars in the dropdown list and the popup auto-hide, so
  they are not pinned open.
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
  subpage slides in from the right over 320 ms (`SLIDE_MS`) on the symmetric
  ease-in-out `slide` (`SLIDE_SHARPNESS`), over the page it was opened from.
  That page stays in place and dims slightly, and the new page is a card with
  a soft shadow on its leading edge, while the window's buttons stay at its
  top right. A pop is the same slide over the same 320 ms on the same curve,
  which is the open played backwards and the open mirrored in space at once,
  so the top page leaves as it came in, revealing the page below, which
  undims. A close on a compact window is predictive back over the same
  320 ms. Never give a close its own duration or curve, and never use an
  asymmetric ease (an expo-out, a Material decelerate) for a slide: its close
  either starts like lag or ends in a snap. A second navigation during a slide
  starts from where the page is, with no snap. A sidebar switch cross-fades
  over 120 ms. Motion is off under reduced motion (`tokens.motion(ms)`
  returns 0).
- Icons are the Bingux set under `crates/ui/icons/`; see
  [Icons](#icons) for how to use one and how to add another.
- A dropdown draws a symbolic icon before each option with `.icons(Vec<Icon>)`,
  in the same order as the options. Sound uses it for each device's kind.

## Icons

The icon set is the chat app's icon fork (`@chat/icons`, a rounded fork of
Phosphor: continuous "squircle" corners, its fill and weight choices and its own
custom icons). Icons are the fork's drawings, not redrawn: `scripts/icons/build.py`
takes the fork's outline, puts it on the 16 px grid (one uniform scale at most, and a
shift of at most half a pixel that leaves the fewest soft edges, see
[Fitting](#fitting-the-pixel-rules)) and writes one SVG per
icon, tinted with the text colour at draw time. The Rust names (`Icon::NetworkWired`)
and the SVG file names (`network-wired-symbolic.svg`) follow Adwaita's symbolic
vocabulary, which the QML shell also uses.

**Colour icons are a separate set.** The file and folder icons of the Files app
(34: the 22 file families, ten folders, the Trash empty and full) are lit,
rounded pictures in the style of the Files app icon, not tinted glyphs, so they do not go through `build.py`: their
sources are `scripts/icons/colour/*.svg`, their compiler and check is
`scripts/icons/colour.py` and their output is `crates/ui/icons/colour/`
(see [File icons](#file-icons)). The taxonomy's Icons page lists them with the
symbolic set (`ColourIcon::ALL`, `name()`, `family()`, drawn by
`colour_icon_view(&tokens, icon, px)`).

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
`crates/ui/icons/chat/geometry.json` and `src/chat_icon_list.rs`, both written
by `scripts/icons/import-chat.py` from `~/dev/chat`; they hold every fork icon, not
only those in use. A catalogue icon is the fork's outline scaled by 1/16, not fitted,
so it can look softer than a named icon. `scripts/icons/review.py catalogue` renders
the whole fork, fitted, to PNG pages; `review.py candidates House:fill Mountains:regular`
shows chosen glyphs side by side; to adopt one, give it a row in `labels.tsv`.

### Fill and outline: the policy

Each icon in `scripts/icons/labels.tsv` has a category (`#policy <category>: ...`
lines) and the category decides its fork weight; `build.py --check` fails an icon
whose weight does not match. The look is the one released in commit `5d49113`: outlines
are the fork's `bold` weight (strokes 1.5 px at 16 px, exactly as the fork draws them) and
objects are `fill`. The fork's `regular` weight (1 px wire) is not used by a named icon:
at 16 px it reads thin, small and rounder than the neighbouring fills, because the
corner radius of the fork's drawing is large next to a 1 px line. A named icon is never
thickened or thinned to a whole pixel (an offset with round joins also rounds the
corners): a 1.5 px stroke has one half-covered pixel row per edge, which is
how it looks, and is accepted.

| Category    | Weight            | What belongs there                                                                                  |
| ----------- | ----------------- | --------------------------------------------------------------------------------------------------- |
| `object`    | `fill`            | Things: devices (laptop, printer, webcam, headphones), places (folders, home, trash), tools, lock, bell, sun/moon |
| `status`    | `fill`            | Badges whose meaning is the mass: error, info, warning, help, secure/insecure, VPN, play, pause, eject, starred |
| `detail`    | `bold` or `fill`  | Things recognised by their inner structure: globe (`bold`), envelope, calendar with dates, text and document files, eye (`fill`), touchpad (`bold`); the phone and the tablet are drawn |
| `section`   | `bold` or `fill`  | The settings sidebar: Network, Bluetooth, Search, Power and Applications (`DotsNine`: nine round dots) are outlines; Displays, Sound, Appearance, Accessibility, Date and time, Users, Mouse and About are fills; the Keyboard and the Desktop (a monitor with a dock) are drawn |
| `action`    | `bold` or `fill`  | Verbs: share, refresh, search, zoom, rotate (`bold`); delete, filter, sidebar toggle (`fill`); copy, edit, full screen, list and grid views are drawn |
| `indicator` | `bold` or `fill`  | Levels and states: wired network (`fill`); volume (`fill`); battery, Bluetooth and the hotspot are drawn by hand (`custom`) |
| `control`   | `bold`            | Tick, plus, minus, close, menu, more, drag handle, window controls, outline star; the chevrons, power and window-new are drawn |
| `drawn`     | strokes           | Drawn by hand: the Wi-Fi family (two 2 px arcs and a dot, symmetric about x = 8), the symbolic-link badge, never a fork glyph |

Default applications: Web, Mail, Calendar, Text and Documents are fills, like Files,
Photos, Music and Video; Text and Documents have their own icons (`default-app-text`,
`default-app-documents`) so changing them never changes `document-edit` or
`document-open`. `document-edit` is only for editing a document (a text editor, a
note); every other edit (a path, a connection, an access point, a rule) is `pencil`.

The controls family is drawn by hand where a fork glyph cannot be placed on whole
pixels or is the wrong picture: the chevrons (`go-next`, `go-previous`, `pan-*`) are
1.5 px polylines with a mitred point and a sharp inner angle; `view-list` and
`view-grid` sit on whole pixels (2 px bars and dots on rows 2-4, 7-9, 12-14; four 5 px
squares); `edit-copy` draws both sheets with one radius; `view-fullscreen` and
`view-restore` are four corner brackets, out and in, as lucide's maximize and
minimize; `preferences-system` and `emblem-system` are one cog (a ring, eight tapered
teeth, a round hole); `display-brightness` is a true circle with eight rays;
`system-shutdown` is a true circle (r 5, centre (8, 9)) open at the top, with the bar
above it; `power-profile-balanced` keeps its needle inside the dial.

### Fitting: the pixel rules

`build.py` places a fork glyph with three per-icon attributes, which `map-fork.py` writes
into the source from `labels.tsv` (`Bluetooth bold scale=1.1`):

1. `mirror=left|right`: keep that half, replace the other with its mirror (the fork's
   `User` fill is lopsided: its right shoulder reaches about 0.8 px further from the middle than the left, so the
   avatar uses `mirror=left`, and is never moved sideways, so it stays symmetric about x = 8);
2. `scale`: one factor on both axes about the middle of the box. Only Bluetooth uses it
   (1.1): the other glyphs keep the size the fork draws, 10 px (the nine dots) to 16 px (a battery);
3. `dx`, `dy` (rare) and the automatic shift: of the translations of at most half a pixel
   (in quarter pixels) that keep the ink in the box, the one that leaves the fewest soft
   edge pixels at 16 px (ties: the smallest move). `align=none` turns the automatic
   shift off.

A glyph is only ever moved and scaled uniformly. Nothing warps one axis, thickens a stroke,
snaps an edge or flattens a curve, so circles stay circles (the Applications dots, the
battery cells' ends, Bluetooth's loops) and the fork's corners are the fork's own: when
an edge cannot meet a pixel line it renders soft. `build.py` raises an error for a
non-uniform scale (`affine`). When there is no `mirror`, the SVG is the fork's own path data
under `translate(..) scale(..)`, byte for byte.

Rules, checked on the rasterised icon
(`scripts/icons/raster.py`: exact-area coverage, no hinting; the ink box is read from a
64 px raster):

| Rule | Limit | Applies to |
| ---- | ----- | ---------- |
| Ink long side | 10 to 16 px | every icon not in `#mark`: the fork's own sizes (nine dots 10.5 px, battery 16 px); catches a glyph that is lost or leaves the box |
| Ink long side, `mark` | 8 to 13 px | `#mark`: chevrons, tick, plus, minus, close, more, window controls |
| Ink in the box | no ink outside the 16 x 16 box | every icon (checked when it is compiled) |
| Centre of the ink from the centre of the box | at most 1.5 px each axis | every icon: the fork centres glyphs by their mass (a lock's shackle, a play triangle, headphones), which puts the ink box up to 1.4 px off |
| Soft edge pixels at 16 px | at most 1.35 x the fewest any placement gives, plus 6 | every icon: pixels with 15 to 85 % coverage in a straight run of 3 or more on a horizontal or vertical edge. A bold stroke always leaves such a row, so the limit is relative to the icon's own best translation (quarter pixels, at most half a pixel) |
| Soft edge pixels at 14, 18, 20 px | at most 100, 120, 140 | every icon: scaled copies cannot be crisp, the bound catches regressions |
| Symmetry | centre within 0.05 px; mirrored coverage differs by at most 0.15 at any pixel, 0.03 on average | `#symmetric`: the avatar and the Wi-Fi family. The other glyphs are symmetric in the fork but may be shifted half a pixel for crispness |
| Counters | a fork glyph keeps every hole and cut-out | every fork icon (`#exempt counters:` to waive) |
| Sidebar set | ink covers 10 to 55 % of the box | the icons of `groups.txt` `[sidebar]`: fills 40 to 50 %, outlines 10 to 40 % |
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
scripts/icons/build.py --suggest [icon ...]  # the scale that brings a glyph into the size rule
make icons-check                    # the first two, the colour icons and golden.py
scripts/icons/golden.py             # every icon at 16-32 px, 1x and 2x, against golden.json (see "Visual integrity checks")
scripts/icons/golden.py --update    # approve a change you have looked at (--sheet FILE draws the set)
```

A failing check names the icon, the number and the fix: `probe-small: ink is 4.00 x
4.00 px (icon): the long side must be 10 to 16 px; change scale`, `probe-soft: 48
soft edge pixels at 16 px, 20 at its best placement (allowed 33)`, `avatar-default: not
left-right symmetric`.
After editing `scale`, `dx` or `dy` in `labels.tsv`, run `map-fork.py`, `build.py` and
`build.py --check`; never edit `crates/ui/icons/*.svg` (they are written).

Icons that show a state (Wi-Fi signal, battery level, volume) are one drawing per
state with the same footprint, so the glyph does not jump when the state changes:
the battery levels share one body (the same rectangle), as do the four volume icons.

**Dimmed ink.** A level icon keeps the parts that are "off" in view instead of dropping them: a Wi-Fi
arc without signal, the empty end of a battery. A source marks such a shape `data-dim="yes"`; `build.py`
writes it as a second path with `fill-opacity="0.4"` (`DIM_ALPHA`, one value for every icon), never
overlapping the solid path. The icon is still tinted with one colour at draw time (Freya's `SvgViewer`
sets `fill` on the root, which a path's own `fill-opacity` keeps; the QML shell's `MultiEffect`
colorization keeps the alpha too). The pixel rules and `golden.py` judge the whole silhouette, dim part
included. A cut removes ink from both layers: the charging bolt on a battery is cut out of the filled and the dim part alike. `review.py` draws both layers.

Battery: `battery-level-N` (0 to 100 in tens) is a rounded body filled from the left to N % with the
rest and the nub dimmed; `-charging` is the same with the bolt, so it shows the level and the charge;
`-100-charged` is full with a tick; `battery-missing` is a solid body with an exclamation mark. Pick
one with `Icon::battery_level(percent)` (Rust, nearest ten) or `SystemIndicators.batteryIconName()`
(the shell, the same rounding). Wi-Fi has two arcs at most (IC-03), so `signal-good` and
`signal-excellent` are one drawing; none, weak and ok light none, the dot and the dot and inner arc.

### Choose the icon for where it is used

Pick the conventional metaphor for the label the user reads next to the icon, not
the glyph whose name matches. A sidebar "Network" page needs a network glyph, not a
node diagram; "Share" is a box with an up arrow, not a paper plane.
`scripts/icons/labels.tsv` records, for every icon, where it appears, the metaphor
people expect there, the fork glyph or drawing, and its category. Mappings that were
weak, and what was decided:

| Icon | Was | Problem | Now |
| ---- | --- | ------- | --- |
| `input-touchpad` | `HandPointing` regular | 1 px wire, thinner than its neighbours | `HandTap` bold, as in `5d49113` |
| `network-vpn` | `Shield` fill | a plain shield | `ShieldCheckered` fill: the chequer of Material's VPN icon |
| `preferences-desktop-wallpaper` (now `preferences-desktop`) | `ImageSquare` bold | a picture is the wallpaper, not the Desktop page | drawn: a monitor on a stand with a dock pill inside |
| `view-app-grid` | solid nine-dot square | a solid blob | `DotsNine` bold (nine round dots, no mass) |
| `input-tablet`, `phone` | `DeviceTablet`, `DeviceMobile` fill | two plain rounded rectangles, neither read as a tablet or a phone | drawn: a landscape 14 x 10 body and a tall 6 x 14 body, each with a screen and a home button cut out |
| `web-browser`, `mail-unread`, `x-office-calendar` | `Globe`, `EnvelopeSimple`, `CalendarBlank` fill | the globe's filled disc hid its meridians; the calendar had no dates | `Globe` bold, `EnvelopeSimple` fill, `CalendarDots` fill |
| Default applications: Text | `document-edit` (`NotePencil` fill) | an editor, not a text file | `default-app-text`: `FileText` fill |
| Default applications: Documents | `document-open` (`FolderOpen`) | a folder, not a document | `default-app-documents`: `FileDoc` fill |
| `help-about` | `Info` regular | 1 px wire in a set of fills and bold outlines | `Info` fill, as in `5d49113` |
| `audio-volume-low` | `SpeakerNone` | a bare cone can read as mute | kept: 0, 1, 2 waves are monotone and mute has the X |
| `battery-level-*` | five fork glyphs for eleven levels, outlined cells | a line per cell, not filled; the level read badly | drawn: a body filled to the level, the rest dimmed |
| `battery-level-*-charging` | `BatteryCharging` | the bolt is small at 16 px, level is lost | drawn: the same level fill with the bolt turned inside out on it |
| `window-restore` | `CopySimple` | close to `edit-copy` | kept: two squares are the convention |
| `find-location` | `CursorClick` | no "inspect" glyph | kept for the inspector toolbar |
| `network-wired` | `Network` bold | the squares were outlines | `Network` fill |
| `go-up` | a folder with an arrow out of it | a second, heavier "up" beside the chevrons | removed: the parent-folder button is `pan-up` |
| `security-high`, `security-low`, `system-lock-screen` | shields and a padlock | three more padlock-like icons | removed: the shell uses the padlock (`network-wireless-encrypted`) |
| `preferences-desktop-keyboard-shortcuts` | `Keyboard` bold | the same drawing as `input-keyboard` | removed: the shortcuts row uses `Icon::Keyboard` |
| `x-office-document`, `x-office-spreadsheet`, `x-office-presentation` | a page marked DOC, XLS, PPT | a file extension is not a picture, and DOC is `default-app-documents` | removed: documents are `text-x-generic`, spreadsheets `table`, presentations `presentation` |
| `audio-card` | `Waveform` bold | the Sound page's fallback device is not a sound card | the Sound page uses `Icon::Sound`; the file stays for the shell's Bluetooth devices (BlueZ names it) |
| `document-edit` | `NotePencil` fill | used for every edit, and a pencil that looked odd | drawn pencil; `pencil` is the general edit icon |

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
4. `scripts/icons/build.py --suggest <file>` prints the `scale` that brings the glyph
   into the size rule (nothing prints when 1 fits; there is nothing else to fit).
   `scripts/icons/map-fork.py` writes the source, `scripts/icons/build.py`
   compiles it into `crates/ui/icons/` and the shell's copy,
   `scripts/icons/build.py --check` judges it.
5. Add the variant to `Icon`, to `Icon::ALL` and to `Icon::bytes()` in
   `crates/ui/src/icons.rs`.
6. Look at it. `scripts/icons/review.py sheets --out /tmp/icons` writes one labelled PNG
   per group (Icon variant, file, category, glyph and fit, where it is used, metaphor;
   16 px dark and light, 18 px, 48 px) and `icons.txt`, the same table as plain text.
   `review.py compare BEFORE_DIR AFTER_DIR --group sidebar` shows a set before and
   after. `scripts/icons/sheets.py --out /tmp/icons` renders the same at 12 to 48 px
   with Freya's own rasteriser, beside the Adwaita originals, plus a top-bar mock.
   The taxonomy's Icons / Icon set page is one grid of every file in `icons/`, sorted by
   file name: the glyph (16 to 48 px, 24 by default) with its name under it, a name
   filter, the name in a tooltip, a press copies it. It carries no per-icon detail; that
   is `review.py`'s job.

The fork's geometry is MIT-licensed Phosphor work; `crates/ui/icons/COPYING`
carries the licence and attribution.

### In the QML shell

`scripts/icons/build.py` also writes `shell/bingux/icons/` (the same icons with a dark fill,
which the shell's `SymbolicIcon` recolours) and `shell/bingux/BinguxIconIndex.js` listing
their names. That set is the shell's whole symbolic icon vocabulary: `BinguxIcons.qml`
resolves every name the shell asks for to a file in it, and nothing reads the Adwaita
theme at runtime. `scripts/ui-lint.py` fails when a QML or JavaScript file of the shell
names an icon the set does not have, so a new icon starts as a row in `labels.tsv`.
Names other programs send (gnome-settings-daemon's on-screen display, the icon names
BlueZ gives a device) are mapped to Bingux icons in `BinguxIconAliases.js`; the shell's
own code uses the Bingux name. Only what no Bingux file covers (the icons of other
programs' applications, notifications and tray items) is left to the system icon theme.

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
cargo test --lib --features testing contrast   # in crates/ui
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
- Right to left: `BINGUX_DIRECTION=rtl`, with `BINGUX_PSEUDO_LOCALE=ar` or `he` to swap every
  letter for Arabic or Hebrew. `layout-lint.py --rtl` compares it with the left-to-right
  capture (see Direction).

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
- `use_tween(target, ms)` changes a value, not the size of the box it sits in.
  Give text a fixed width if neighbours must not move. `use_tween` starts at its
  target, retargets from the value on screen when the target changes (an
  interrupted motion never snaps), and jumps at 0 ms.
- `icon_morph(from, to, size, colour, progress)` draws two icons stacked in one
  `size` square and sets both from one `progress` (0 shows `from`, 1 shows `to`):
  `from` turns a quarter turn clockwise while it shrinks to `MORPH_SMALLEST`
  (0.5) and fades out, `to` turns in from a quarter turn the other way while it
  grows and fades up, all along one smoothstep, with the opacities adding up to
  1. The square never changes size, and both glyphs stay mounted (the hidden one
  at opacity 0) so neither is rasterised for the first time mid-motion. The
  layers are the pure function `morph_layers(progress)`, so the motion is
  continuous exactly when the progress is. `IconButton::morph(to, on)` drives it
  with `use_tween(on, tokens.motion(MOVE_MS))`: 160 ms, instant under reduced
  motion, a flip half way continues from the icon on screen, and a button that
  mounts with `on` set does not animate. The caller changes the tooltip (which is
  the accessible name) and the press handler with the state. The taxonomy shows
  it on the Buttons page ("Morph: Reload and Stop") and on the Motion page
  (frozen at five points); `motion/icon_morph_tests.rs` steps the animation clock
  16 ms at a time and reads the button's pixels, box and name.
- `use_dwell(active, Dwell { show_after, hold })` is a state that shows late and
  goes late, so one that is true for a moment does not flicker: it is shown once
  `active` has held for `show_after` without a break, and once shown it stays at
  least `hold` from the moment it was shown, and while `active` stays true.
  `Dwell::LOADING` (150 ms, 300 ms) is the Stop state of a loading folder. The
  state machine is `DwellGate::update(active, now)` on a clock the caller
  passes in (tests use a fake one); the hook reads the real clock and draws the
  component again when the answer changes by time alone.
- `animated_number(value)` rolls its digits as the shell's clock does
  (`RollingNumber.qml`). Every cell of the number is a clipped column one line
  tall; a digit that changes slides out of its place while the new one slides
  in, 140 ms (`ROLL_MS`), ease-out cubic, the two cross-fading (the old digit
  at `1 - p`, the new at `p`). A value that went up rolls the digits up; one
  that went down rolls them down, and a wrap (9 to 0, 59 to 00) still rolls up.
  Digits that do not change do not move, and a digit that changes passes no
  digits between (3 to 8 is one slide, as on the clock). The position of a
  column is a tween over numbered slots, so a new value mid-roll adds a slot
  ahead and the digit carries on from the point it had reached; a value that
  comes back turns the roll around where it is. Nothing queues and nothing
  snaps. Reduced motion changes the digits at once.
  - The builder: `.decimals(n)`, `.min_digits(n)` (zero padding, for a clock
    with `:` labels between three numbers), `.style(TextStyle)`, `.grouped()` /
    `.locale("de_DE")` / `.format(NumberFormat)` for the thousands and decimal
    marks (English, no grouping by default; Latin digits always), `.prefix(..)`
    and `.suffix(..)` for a unit that stays put (`%`, ` MB/s`), `.silent()`.
    The value is any `impl Into<f64>`. A negative number has a real minus (U+2212)
    that cross-fades in place; a value that rounds to zero has no sign.
  - Width: digits are tabular and every digit column is as wide as the widest
    digit of the style (measured with the same engine as the labels), so counting
    never changes the width and the text beside the number never moves. A number
    that gains or loses a digit changes its width smoothly: the new column grows
    from nothing while its digit rises in, and a lost column shrinks while its digit
    leaves. Separators and signs cross-fade in place; only digits travel. Put the
    number in a box of its own width, aligned right, when even that must not move
    a neighbour.
  - Height: a column is one line of the style, so the number sits on the same
    baseline as a label of that style beside it, from `NUMERIC` to `HERO`. There
    is no alpha mask in the renderer; the clip edge fades because each glyph's own
    opacity falls to 0 as it leaves, so it works on any surface.
  - Assistive technology: the number is one polite live region
    (`Role::Status`) whose label and value are the final text with its prefix and
    suffix, set from the first frame of a change; the columns and the units are
    hidden from the tree, so a screen reader hears `130%` once, not digits and not
    frames. `.silent()` makes it a plain label.
  - Right to left: the digits are always laid out left to right in Latin order. The
    number is a block; a mirrored layout may move the block, but never reverses
    the digits. The layout engine reads the window's direction (see Direction).
  - Not selectable: the digits are drawn one column each, so the final text cannot
    be selected or copied from the number. An overlay of invisible `SelectableText`
    was tried and did not take a drag in the headless harness, so it was left out;
    give a copyable value its own `TextStyle::selectable` text.
- `use_cycle(ms)` is the clock of every loop: a phase from 0 to 1 over `ms` that
  starts over as it gets there, for as long as the calling component is
  mounted. Its state lives in the caller's scope, so the caller drawing again
  never restarts it. It is the one motion that ignores reduced motion (a
  progress indicator that stood still looks frozen); the caller picks a
  longer, gentler cycle instead. The spinner and the indeterminate bar are
  built on it (see Progress bars and spinners).

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

`bingux-taxonomy` (`apps/taxonomy`) shows every component, element,
widget, icon and token of bingux-ui running, in a real Bingux window. Open it
with `bingux-taxonomy`, or at a page with `bingux-taxonomy controls/buttons`:

```sh
CARGO_TARGET_DIR=/var/tmp/bingux-settings-target \
    cargo run --manifest-path apps/taxonomy/Cargo.toml
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
   `apps/taxonomy/src/demos/<area>.rs`: `Spec::live(label, || ..)`
   for the real component, `Spec::forced(label, || ..)` for a state its props
   hold. The closure builds an `Element` and may use hooks. `.wide()` or
   `.page(height)` give it room, and `.script("click-text Open; wait 300; save
   {out}")` adds a snapshot of a state a still image cannot reach (`click-name
   NAME` clicks an icon-only control by its accessible name).
2. Add an `Entry` to `ENTRIES` in `src/registry.rs`: its name, category and
   sidebar page, the API in one line, the `doc` anchor (a heading of this
   file), `covers` (the Rust names it shows) and its `body`.
3. Name every public item you added in some entry's `covers`. An internal with
   nothing to look at goes in `coverage-allowlist.txt` with its reason. A new
   icon needs nothing: the Icons page lists the `icons/` directory.
4. Run `python3 scripts/taxonomy-check.py` and `cargo test --manifest-path
   apps/taxonomy/Cargo.toml`. They fail on an exported item with no
   entry, a `covers` name bingux-ui no longer exports, an `Icon` variant not in
   `Icon::ALL`, and a doc anchor that is not a heading. `make check` runs both.
5. Look at it: `bingux-taxonomy --snapshot /tmp/taxonomy <category>/<page>`, or
   run the app and use the switcher. Report the desktop and the 390x800
   screenshot paths, or what you saw, in the change description.

### Layout checks

Which check holds which design rule, and how to run them all and read the result (`make rules-check`, the report in `design/rules-report.md`): `design/rules-checks.md`. A new check or rule cites its rule IDs there; the layout rules below do it in every finding or in `RULE_IDS` of `scripts/layout-lint.py`.

The inspector's Layout tab (beside Elements, Performance and Accessibility) lists visual defects that are not accessibility problems. **Uneven padding** is a box with the same padding on both sides of an axis whose content sits nearer one side. The content is the boxes of the box's children (a child's own padding is not counted against its container), and only an axis the content sizes is judged: a card as wide as its page, a flex cell or a percentage starts its content at the leading edge on purpose. The other rules are an item off the centre line of a row that centres its items; siblings of one component with different heights in a row; a box with a fixed height and more than 32 px empty under its content; ink clipped by a box that clips; partly overlapping controls; one-line text cut off while its container has room for all of it; unaligned baselines of same-size texts in a row; text broken inside a word or into more lines than it is given; text that overflows its box without an ellipsis; a box outside its parent; content wider than its scroll view; a control whose label is hidden; texts that lie on each other (only the part a clip leaves visible counts); and a focus ring that is not around the box its control paints. Text inside a text field is left out of the text rules, because a long value scrolls there by design. Each finding shows the rule, the measured numbers and how to fix it; pressing it selects the node in Elements and outlines it in the app. The rules are pure functions over the inspector's snapshot (`inspector/layout_checks.rs`, tested on hand-made trees). The touch-target minimum on a phone stays an Accessibility check.

**Icon off centre** (`icon-not-centred`) is an icon inside a field's leading slot (the `IconSlot` component) or an icon button whose centre lies more than half a pixel (`ICON_CENTRE`) from the centre of that slot on either axis. It exists because a glyph set by hand with a margin (the search field's, once 8 px from the edge in a 36 px field, so 2 px left of the middle) looks wrong at once and is not found by the padding rules. The measure is the centre of the icon's box; the unit tests of `search_field` measure the ink on the rendered pixels for the same slot.

**Spacing and alignment checks.** Eight more rules hold the padding model of `design/rules.md` (the table "The padding model") to the measured tree. They are in `inspector/checks/` (one registration point, `checks/mod.rs`: `run`, `meta`, `EXEMPT`), every finding starts with its rule IDs from `design/rules.json` (`[SP-01]`), and each has a known-bad and a clean fixture in `checks/tests.rs`.

- **Inset asymmetry** (`inset-asymmetry`, SP-01): a card (the painted box a `Group` draws round its rows), row, header, banner, dialog or popup has the same room above and below its content and, where the content spans the box, at its start and its end, whatever padding it declares (`uneven-padding` only sees boxes that declare equal padding). A row, property row or header is measured on its ink (a leading slot taller than its icon is not content); a card on its children's boxes (its rows are flush). The start and end are compared on boxes, so a slot kept for an icon or a trailing control still counts; an end that is a control is `trailing-inset`'s. A box taller than the window is a virtual list and is not measured.
- **Trailing inset** (`trailing-inset`, SP-02, AL-09, SP-12, SZ-12): the last control (button, switch, dropdown, icon button) of a row, field, toast, banner or strip is the same distance from the top, the bottom and the end of its box. The chevron zone and a tab strip's end cell are flush, so all three are 0. Not held: a field or slider in a row (`NOT_BUTTONS` in `checks/mod.rs`, each with its reason), a tab's own close button, a header (its 36 px buttons sit in 48: 6 above, 8 at the side; `EXEMPT`), a control stacked under a banner's text, and in a row or banner taller than one line only the vertical centring (a 40 px button in a 63 px row is 11.5 from the top and cannot also be 8 from the end on the scale). A button in a row that is not passed straight to `Row::trailing` (a tooltip wraps it, or a switch sits beside it) needs `Row::trailing_fit()`, or the row keeps its gutter at the end.
- **Column aligned** (`column-aligned`, AL-01, SZ-05): rows of one kind with the same left edge, width and icon-slot width put their icon slots at one x and their labels at one x; trailing dropdowns of one card are one width. The slot is the square box round the glyph (an avatar and a glyph in a 24 px slot share a column).
- **List row inset** (`list-row-inset`, SP-09): a sidebar or place row's icon is as far from the start of the row as from its top (`NAV_INSET` = the space above and below the icon), at both densities. A second line (a drive's usage) makes the row taller than a nav item and exempts it; file rows (SP-10) keep `FILE_ROW_INSET` and are held by the parity pass instead.
- **Field text start** (`field-text-start`, AL-03, SF-01): text in a field with an `IconSlot` starts S1 (4 px) after the slot.
- **Header measure** (`header-measure`, AL-04, SZ-02): a centred header title (the one on its own out-of-flow layer, not a `title_start` one) is centred on the whole bar unless a control leaves it no room, and every icon button, segmented control, split button and action button in a header is `Dim::Control` tall (36, or 44 in a window narrower than 740 px; a wider window may hold a compact specimen, so either passes there). A bar that holds tabs is a tab strip and is skipped.
- **Radius not concentric** (`radius-concentric`, BR-01, BR-02): a painted box that shares a corner with its painted ancestor, an even inset d in from both sides and at most half the ancestor's radius, has `Radius::nested` of that radius and d (flush, it is square or the same radius). A capsule (a pill wider than it is tall) is only in a component of `PILLS` (switch, notice, dock preview, media bar, count badge, each with its reason), and a circle is round unless it is the face of a button component (`IconButton`, `ActionButton`, ...): icon buttons follow the radii.
- **Off-scale spacing** (`off-scale-spacing`, SP-15, SP-21): every padding, margin and gap of the tree is a `Space` step or a held inset token (10, 16 and 18 are `FIELD_ROW_INSET`, `NAV_INSET`, `SWITCH_INSET`). A window narrower than 740 px must use the compact values; a wider one may hold a compact specimen, so either set passes. A thin box (a scrollbar thumb) is placed by its margin and is not measured.

**Parity** (`python3 scripts/layout-lint.py --parity`, which `make layout-check` passes). One window shows one density, so parity compares two captures of the same page. Each capture's line in `BINGUX_UI_LAYOUT_LINT` carries `metrics`: the named insets of each component class (padding, gap, radius, a row's icon-left, label-start and trailing-end, a card's content-top, -bottom and -start) with how often each value was measured. `--parity` takes the most common value of each between the desktop and the phone capture of every page (Settings pages and popups, Files, and each taxonomy page with its `.phone` twin) and fails where an inset that scales (`PH-01`) is smaller on the phone or more than 1.4 times the desktop value, or where an inset that `PARITY_HELD` lists as held equal (radii, `FILE_ROW_INSET`, `DIALOG_INSET`, the header's end inset; `PH-03`) differs. A difference that is intended is an `ALLOWED` entry for the rule `parity`.

**Value cut off** (`value-ellipsised`) is any text that ends in an ellipsis because it does not fit, whose line limit leaves part of it out, that a path label shortened, or whose ink a clipping box cuts off. A value is read in full or it is useless (a licence, a path, an address, a version), so plain text is never allowed to be cut, in a row, a property, a table cell or a dialog. It is not loosened for spare room or for a few visible glyphs. Not findings: text in a text input (it scrolls), text a scroll view or a box whose content is larger than itself moves out of view (scrolled, not cut), and a path or name that a `path_label` shortens by design while its full text is the hover tooltip (it lies under an `Elided` box; a closed dropdown whose label does not fit does the same). Fix: let the value wrap or stack under its title (an info row does this itself, see Info rows), put a long list in a `select_text` block, or use `path_label`. `python3 scripts/layout-lint.py --rule value-ellipsised` reports only this rule. The lint also runs Bingux Files: the folder list and the Properties dialog (Basic, Permissions, Details) of a file, a link and a folder with names, paths and a link target far too long, at 1100x760 and 390x800.

`python3 scripts/layout-lint.py` runs the same rules headlessly (no compositor, fixed fakes for the machine's data) over every Settings page and subpage, the Settings popups that open from a click, and every taxonomy page, at desktop size and 390x800, and exits 1 on a finding. `make layout-check` is the same run with `--build` (it builds the snapshot binaries first, a no-op when they are current) and `make check` calls it together with `make overflow-check`, so a change that adds a finding does not pass. `--only settings|taxonomy`, `--pages a,b`, `--sizes 390x800` and `--jobs N` narrow or widen it; `--json FILE` writes every finding and `--keep DIR` keeps the captures. With `BINGUX_UI_LAYOUT_LINT=FILE` any `bingux_ui::snapshot` run appends the findings of each saved capture to FILE, and with `BINGUX_UI_LAYOUT_TREE=1` as well it writes the measured tree (`<capture>.tree.txt`, one node a line with its id, component, box, padding and sizing) beside each capture: read that to see why a box measured as it did. A full run takes about five minutes.

Fix the component or token. Where a finding is right by design, add an entry to the script's `ALLOWED` list (`scripts/overflow-check.py` and `scripts/tooltip-check.py` have their own, of the same shape): the rule's key (`dead-space`, `outside-parent`, ... see `Rule::key`), a regex that must match the capture's name (`popup-wifi-join-`), a regex for the component path or node description it is scoped to (`Dialog "Join network"`), and one line saying why the box is meant to look like this. Never a blanket entry: a rule alone, or a page alone, allows nothing. An entry that matches no finding is reported as unused, so the list cannot rot. Where the rule is wrong rather than the box (a fixed-height specimen stage, text inside a text field, content a clip hides on purpose), change the rule in `inspector/layout_checks.rs` and add to its tests a tree that is now clean and one that stays a finding.

### State checks

The layout rules read one captured tree. `python3 scripts/state-check.py` (`make state-check`, called by `make check`) reads what a window *does*: it drives every Settings page, popup, taxonomy page and the Files window headlessly, moves the pointer onto each interactive node, presses it, focuses it, tabs through the page, opens every menu and renders what a person would see. The code is `crates/ui/src/inspector/state_checks/` (`Snapshot::state_report`), the same `bingux_ui::snapshot` machinery the other checks use; with `BINGUX_UI_STATE_CHECK=FILE` every capture an app saves appends its report to FILE. The animation clock is switched off while it runs, so each state is finished one frame after it starts; what is measured is what is on screen after the transition, and the transitions themselves are the motion checks' work. An interactive node is a node with `a11y_focusable(true)` and an id; a *control* (a role a person presses, toggles, picks or types into) is held to every rule, a focusable region (a page of selectable text, a window) only to the Tab and ring rules. Each finding names its rule IDs from `design/rules.json` (`[ST-06]`).

| Rule key | IDs | What it holds |
|---|---|---|
| `state-hover` | ST-06 | Pixels in the node's box differ between rest and hover (a changed pixel is a channel off by at least 4; at least 0.5 % of the box and 8 pixels). |
| `state-press` | ST-06 | Pressed differs from hover. The press is let go off the control, so no click lands. |
| `state-focus`, `ring-clipped` | FC-01, FC-04 | Keyboard focus draws a ring (the face's `FocusRing` node, at 2 px, 2 px off the face) on all four sides, in the ring's own box; a side that is not drawn names what cuts it: the clipping ancestor, the window edge (`focusable_inside`), or the node painted over it. The ring is read where the control is drawn after the focus (focus scrolls a control at its scroller's edge into view) against a rest frame at that scroll position with focus elsewhere (a popup keeps focus on its first field, so that field is judged against a frame where another control has it). A ring over a fill of its own colour (the accent ring over a slider's accent track) counts as drawn there once another side shows it. A control under an open panel (a completion list, a menu) is not measured: the pointer and the ring land on the panel. |
| `state-selected`, `hover-collateral` | ST-05, ST-09 | A selected sibling looks different from an unselected one at rest; hovering another item leaves the selected item's pixels alone. |
| `row-hover` | ST-01 | A row that is not activatable paints no hover fill (read at the ends of its middle line, where no tooltip or caret is). |
| `state-open` | ST-06 | A dropdown or menu button opened from the keyboard has the fill it has under the pointer (not on a phone, where the sheet's scrim dims the page). |
| `state-disabled` | ST-04 | A control with `a11y_enabled(false)` is dimmed to `DISABLED_OPACITY` (+ 0.15) and its declared fill does not change under the pointer (read from the tree, so a tooltip that opens on it is not a fill). |
| `touch-target` | SZ-01, SZ-11, PH-04 | At compact density (a window narrower than 740 px) every control's hit area, the face plus any transparent absolute box inside it (`hit_area`), is 44 x 44 px or more. |
| `resize-clearance`, `scrollbar-clearance` | SZ-06, SC-01, SC-02 | No control's box reaches into a popup's eight resize handles (an edge band of 6 px, 2 inside the panel, and 12 px corner squares), the cursor over a control is never a resize cursor, and a scrollbar's gutter is clear of the handles and of the header bar. |
| `tab-reach`, `tab-order`, `tab-ring`, `tab-trap`, `popup-escape` | FC-04, FC-05, FC-07 | Tab through the page from no focus: every focusable node is one stop and none is a stop twice, the lap comes round and covers the page (a lap that ends part way is one finding at its last stop; a control that swallows Tab, or a lap that never closes, is a trap), no step between controls goes back up its column (read from the layout at the moment of the step, because a Tab scrolls), every stop shows its ring (a control cut by the viewport or half under the floating header is not held to it), Shift+Tab retraces the lap, in a window with a modal scope no stop is outside it, and Escape closes it (twice when a text field is focused: the first leaves the field, controls/text_field.rs). |
| `menu-keys` | FC-11, MN-02, MN-03 | Every dropdown and menu button in full view is opened with Enter and walked (on a phone, where a menu is a sheet with no keyboard highlight, only the open and the close): Home and End highlight the first and last pickable row, Down and Up move one row, typing the first two letters of a row highlights a row that contains them and keeps the query for 2.5 s, the first Escape takes the query back and the second closes the menu and returns focus to the button, and in a dropdown outside a header Enter picks the highlighted row and closes the list. |
| `cursor-map` | ST-03 | The cursor with the pointer on the middle of each control is the arrow; the I-beam only over a text field, a pointing hand nowhere, a resize cursor only on a handle. |
| `text-under-glyph`, `chevron-size` | AL-12, PH-07, SZ-04 | A dropdown's, menu button's or field's text never overlaps a glyph beside it, and the trailing chevron of a dropdown or split button is `IconSize::Mark` at the control's own size class (12 px, 14 px at a 44 px control). |

Fix the component. Where a control is meant not to do what a rule asks, add an entry to `ALLOW` in `state_checks/mod.rs` (a component's doing, with its reason: a selected segment has no hover, a stepper's middle is a value) or to the script's `ALLOWED` (one capture's: a rule key, a regex for the capture's name, a regex for the control's label, and why). An `ALLOWED` or `KNOWN` entry that matches nothing is reported as unused, so those lists cannot rot. `KNOWN` is for a defect found and not fixed: it names what is wrong and who owns the code, is printed on every run and does not fail it, and goes the day the defect does. `python3 scripts/state-check.py --fixtures` runs the check on its own known-bad controls (no hover, no ring, a ring cut off by its clip, a 30 px control at 390x800, a pointing hand, a Tab order that goes back up the page, a label under its chevron) and on the shared controls, which must pass every rule (`cargo test --lib --features testing state_checks`).

Narrow it with `--only settings|taxonomy|files`, `--job REGEX` (the captures whose name matches, such as `popup-wifi-join-desktop|taxonomy-controls`), `--pages a,b`, `--sizes 390x800`, `--scope static,cursor,tab,sweep,menu` (`BINGUX_UI_STATE_SCOPE`), `--limit N` (the most controls swept per capture, `BINGUX_UI_STATE_LIMIT`), `--rules rule,rule`, `--kinds` (the nodes by role and component) and `--notes` (what a run could not do, such as a press that changed the window, which ends that capture's sweep). The sweep tries three controls of each kind (`BINGUX_UI_STATE_PER_KIND`) in a time budget of 40 s a capture (`BINGUX_UI_STATE_BUDGET`): a taxonomy page is 12000 px tall and a frame of it takes a second, so a capture says "stopped at the time budget" rather than run on. `BINGUX_UI_STATE_KEEP=DIR` writes the frame a ring or hover finding is about to DIR, and `BINGUX_UI_STATE_TRACE=1` prints what a menu walk read.

### Running an app from a script or a test

The Settings pages read and write desktop settings through the `gsettings` command (mouse speed, double-click time, key repeat, the interface and power keys). A sweep that presses sliders and switches would change the real dconf of the person who ran it, so every check that starts an app builds its child's environment with `isolated()` of `scripts/app_env.py`: `GSETTINGS_BACKEND=memory`, and no `WAYLAND_DISPLAY`, `DISPLAY` or `DCONF_PROFILE`. `probe_lib.run_job` and `layout-lint.run_job` (and so every check that goes through them), `frame-gate`, `frame-walk`, `frame-time`, `loop-check`, `nav-slide`, `number-roll-check`, `mobile-walk`, `files-shift`, `focus-ring-sheet` and `state-check` use it, and the harness sessions (`session.sh`, `files-session.sh`) export `GSETTINGS_BACKEND=memory`. `tests/app-env.test.py` runs both runners with `env` as the app and holds that no script that starts an app skips the helper. The in-process harness does the same: `Snapshot::new` calls `runtime::isolate_system_settings()`, after which every command `runtime::run` starts gets the memory backend, so `cargo test` and the `--snapshot` captures of the three apps cannot write dconf either. A new script that runs an app belongs in `LAUNCHERS` of that test.

### Tests that run side by side

Every `cargo test` test runs on a thread of its own, even with `--test-threads=1`, and the threads share one process: its environment, its files, its statics. A thread-local is therefore a test's own, and anything else is every test's. Two rules keep the order and the number of threads from mattering. State a window keeps on disk is the test thread's own in a test build: the recent searches (`files::search_bar::recents_file`, which asks `files::test_state` for a file that goes with the thread) and the remembered folder views (`files::view_state`, held in memory per thread), so no test opens with what another one searched for, and none writes the person's `~/.local/state`. A new thing a window keeps in a file takes its path from one function like these, never from `std::env::var` at the point of use, and never from a test that sets an environment variable for the whole process.

A test waits for what it expects, not for a time. `Snapshot::settle(d)` always runs for the whole of `d`, so a duration that suits an idle machine fails on a loaded one and one that suits a loaded one is slow on every other. `Snapshot::settle_until(cap, |shot| ..)` steps the window until the condition holds and returns at once, with `cap` generous (30 s in the search tests): the cap costs nothing when the machine keeps up. Keep `settle(d)` for what must not happen. A test seam that paces a background thread by the clock has the same flaw: the `-slow-walk` folders of the search tests hold the walk after its first batch until it is stopped (`holds_the_walk` in `files/search.rs`), not for some milliseconds a batch.

A test that opens a film or a song does not play it through the machine's sound server. `spawn_host` (`preview/host_link.rs`) starts the helper of a unit test with `--audio-sink fakesink`, as the helper's own tests do. Two helpers that opened a film at once on the default output (here a PipeWire sink) stood at their first picture until the helper's own 20 s open timer stopped them ("The preview took too long and was stopped"), whatever budget the test allowed; each opened in under a second alone. A budget is not the fix for a helper waiting on a device: take the device away.

### Visual integrity checks

These hold the rules of `design/rules.md` (the IDs are in `design/rules.json`) that a token test cannot see, by what a window looks like. Each check prints findings tagged with the IDs (`[BR-03]`), has `--fixtures` (the check on its own known-bad and clean cases, with no binary needed, so a check that stops finding things fails first), an `ALLOWED` list of the same shape as the layout lint's (a rule key, a regex for the capture, a regex for the node, a reason; an entry that matches nothing is reported), and is run by `make check`. The apps run through `bingux_ui::snapshot`; with `BINGUX_UI_PROBE=FILE` every capture also appends one JSON line (`inspector/probe_checks`): the node tree with flags, the text nodes with their declared colour and size, each modal scope with the painted card it draws as, and the scroll containers. `scripts/probe_lib.py` runs the captures (the page, popup and Files lists of `layout-lint.py`, so a new page is covered at once) at a scheme and a scale (`--variants dark,light,darkx2,lightx2`) and reads PNG and probe together.

- **Icons** (`scripts/icons/golden.py`, CK-01, IC-01, IC-03, IC-05, IC-06, AL-15; `make icons-check`). Every icon of the set, symbolic and colour, is drawn with the exact-area rasteriser of `raster.py` at 16, 18, 20, 24 and 32 px at 1x and 2x and measured: the ink box, coverage (falls when strokes thin, rises when corners lose their roundness), soft edge pixels, the weight of the line (twice area over outline length), every dot (width, height, share of its box) and a 16 px picture. `scripts/icons/golden.json` is the approved record, one line an icon; a difference is a finding when an edge moves 0.15 px, coverage 2%, the stroke 0.06 grid units, a dot changes shape or the picture differs by 2.5% of its ink. A new or removed icon is one too, so a change to an icon commits `golden.json` with it: look at `golden.py --sheet /tmp/icons.png`, then `golden.py --update` (which refuses while an absolute rule fails). The absolute rules: a dot of a drawn icon is round (fills 0.70 to 0.88 of its box), a Wi-Fi icon has at most two arcs above its dot, a `#symmetric` icon is its own mirror image, nothing is empty or leaves its box. The look the golden holds is the set as it was on 2026-10-29.
- **Stability** (`scripts/structure-stability.py` and `scripts/scroll-continuity.py`, CK-02 and CK-22, LS-01, LS-02, LS-03, LS-05, LS-06, TX-16). The first frame the harness can give and the settled one of every Settings page and subpage (both sizes, with every background result held back 1.5 s), every click-opened popup and sheet, every taxonomy page, the Files folder list, a new tab, switching back, Properties and the preview pane, and Displays with a display held mid-drag. Nodes are paired by their path from the root; one whose position in its parent or size changed by more than a pixel is a `layout-shift` (the outermost only; a text is judged on its height and vertical place, not on the width a changed value gives it), one only in the settled tree is `appears` (or `title-late` for a `PageHead`), one only in the first is `disappears`. There is no list of exempt pages; the three entries in `ALLOWED` are a tab opening by width (design/tabs.md), the preview pane taking its width and a phone folder's rows arriving with the listing. `scripts/layout-stability.py` is still the frame by frame version for Settings. `scroll-continuity.py` scrolls a page, does the thing and requires the same offset to 1 px (a virtual Files list has no offset in the tree, so it requires the same first row): Network scrolled then a subpage pushed and popped, a page scrolled while its data lands, Files through a new tab and Alt+1, and into the parent folder by its crumb and Back. A walk that never scrolled fails rather than passes.
- **Pixels** (`scripts/pixel-check.py`, CK-20 and CK-07). `text-contrast` (CL-04): the colour that covers most of a text's box in the PNG is its ground, the text colour the layout resolved (or, for a text that inherits one the headless capture does not resolve, the pixel furthest from the ground) is its ink; under 4.5:1, or 3:1 for text of 24 px or 18.66 px bold, is a finding. Dimmed text (opacity under 1), text on a busy ground, text under an overlay and a box with nothing drawn in it (a frame with its own origin, a fade) are counted, not judged. `notice-edge` (BR-07): a banner's or toast's edge against its fill at most 1.3:1; `floating-dark` (CL-03): a toast darker than the window. `popup-perimeter` (BR-03): each side of every popup, dialog, menu and sheet card is cut into eight runs and each corner sampled on its arc, and the ring (the card's own edge pixels), the inside and the outside (where the shadow falls) are compared; a run is visible from 1.10:1 (a corner from 1.05:1), in both schemes at 1x and 2x, and `popup-perimeter-structure` requires every header bar on the card's top edge to be inside the node that carries the border. A side on the window edge is not measured. Two pitfalls it works around: numpy 2.2.6 here returns wrong values from `**` on arrays over about 30000 elements (`probe_lib.to_linear` works in pieces), and the harness panics at 2x on the taxonomy's address bar page (`TAXONOMY_SKIP_AT_SCALE`).
- **Copy** (`scripts/copy-lint.py`, CK-06, TX-01, TX-05 to TX-11). A lexer reads the string literals of the apps and bingux-ui outside comments, attributes, `#[cfg(test)]` blocks, log and panic messages; the same rules run on the text of every label in every capture, where a subtitle is also held against its title (token overlap, and a row's, not a banner's, subtitle may not instruct, end in a full stop or run past 90 characters). Rules: `percent-space`, `show-more`, `click-here`, `exclamation`, `three-dots` (the ellipsis is `…`), `filler`, `title-case` (source only: names in a capture are data), `quote-mix`, `password-mask`, `computer-label`, `value-ellipsis`, `subtitle-restates`, `instruction-subtitle`, `subtitle-period`, `subtitle-long`. No rule in `design/rules.md` fixes straight against curly quotes (Settings uses straight, Files curly), so only a string that mixes the two is a finding; the run prints the counts.
- **Motion** (`scripts/loop-check.py --all`, `number-roll-check.py`, `nav-slide.py`, `nav/slide_users_tests.rs`, `frame-gate.py`; CK-19, CK-09, CK-08). `loop-check --all` renders every taxonomy page and state twice at two settle times in both motion modes: a bar or a spinner that changed is a loop that runs; anything else that changed is motion on a page that should be still (`ALLOWED_MOTION`). `number-roll-check` walks the Animated number specimens at a tenth of the animation clock and reads the frames: the share of the way to the last picture never goes back by more than 15%, at least 3 looks are between the first and the last, nothing outside the digits' columns changes and what changes stays inside one line. `nav-slide.py` reads p(t) of each push and pop from the pixels and requires the pop to be the push backwards and mirrored within 0.12 and as long within 40%; the Rust test holds what a loaded machine cannot: the shared curve is its own mirror to 1e-5, every file that draws a slide is in its table (a new one fails until added) and no motion has an `_IN_MS` that differs from its `_OUT_MS` (the focus ring's, 160 and 120, is listed). `frame-gate.py` mounts each page N times from the Power page through `BINGUX_SETTINGS_SNAPSHOT_TIMING` and gates the best of the rounds against `scripts/frame-budget.json`: over `warn_ms` (20, about a frame) it warns, over `fail_ms` (50, the blocked-thread limit of `frame-time.py`) or when a page cannot be timed it fails; budgets scale by how slow this machine runs a fixed piece of Python (at most 4x), a median over three times the best is flagged `noisy`, and `BINGUX_FRAME_GATE=report` never fails. It needs no sway session; `frame-time.py` still measures real input and presenting.

First runs (2026-10-29, before fixes), what they found and what became of it. Pixels: 22 texts in the light scheme at under 4.5:1, namely the taxonomy's TOOLTIP recipe specimen (dark ink on the floating surface) and the Files audio and video player's card (dark ink on its black backdrop); fixed in `demos/foundations.rs` and `preview/viewers/card.rs`. A banner's edge was 1.35 to 1.43:1 in the dark scheme at 12% ink; `NOTICE_EDGE_ALPHA` is 0.07. A phone sheet had no hairline (its top edge was the fill against the dimmed page, 1.07:1); `window/popup.rs` draws the outline on the sheet too. Rendering at 2x panicked in two unwraps of the vendored freya-core (an accessibility child and an effect-cascade node that were removed in the same frame); both are guarded. Copy: 17 three-dot ellipses in Settings and 10 trailing full stops in subtitles were written out, the Files undo labels are sentence case, and two taxonomy subtitles that instructed or ran long were cut. Open: the Files preview video specimen draws its loading spinner at the top, over the header's "2 of 5" (seen in the light scheme: the text is under it, so the pixel check cannot judge it), the history menu and tab title still call the file system root "Computer" (TX-09), and `frame-gate.py` has no 1000-item Applications or Fonts fixture (BH-04).

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
  key toggles once. `BINGUX_INSPECT_KEYLOG=1` prints every
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
