//! The application loop: one per process, like GtkApplication.
//!
//! [`Application::run`] reads the location named on the command line, makes
//! sure only one copy of the application runs, and opens the window.
//!
//! * The app's routes ([`Application::routes`]) are its navigation tree. The
//!   window's router holds the stack of locations; components reach it with
//!   [`crate::nav::use_router`] and anywhere else with [`crate::nav::navigate`].
//! * A second launch does not open a second window. It sends its location (a
//!   URI such as `bingux-settings://network/proxy`, or a bare path of the
//!   app's scheme) to the running copy over a unix socket in
//!   `$XDG_RUNTIME_DIR` and exits 0. The running copy opens it and raises its
//!   window. A socket left behind by a crash is removed and replaced.
//!   `xdg-open bingux-settings://network/proxy` reaches the same path through
//!   the desktop file's `x-scheme-handler`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Poll, Waker};
use std::time::Duration;

use freya::prelude::*;

use crate::nav::{Location, Mode, RouteTable, Router, provide_router};
use crate::theme;

/// Who the running application is: its id and the name its windows carry.
static IDENTITY: std::sync::OnceLock<(String, &'static str)> = std::sync::OnceLock::new();

/// The id the running application was started with ([`Application::new`]):
/// what the compositor reports as the `app_id` of each of its windows. Empty
/// before [`Application::run`] (in a headless snapshot, say).
pub fn app_id() -> &'static str {
    IDENTITY.get().map_or("", |(id, _)| id.as_str())
}

/// The name of the running application, as [`Application::title`] gave it.
/// Empty before [`Application::run`].
pub fn app_title() -> &'static str {
    IDENTITY.get().map_or("", |(_, title)| title)
}

/// The settings of one application: its identity, its routes, its window and
/// the usage text `--help` prints.
pub struct Application {
    app_id: String,
    routes: Option<RouteTable>,
    title: &'static str,
    size: (f32, f32),
    min_size: (f32, f32),
    borderless: bool,
    transparent: bool,
    usage: String,
    normalise: Option<fn(&std::ffi::OsStr) -> String>,
    about: Option<crate::about::AppInfo>,
}

impl Application {
    /// An application with id `app_id`. Without [`Application::routes`] its
    /// scheme is the id and it has no pages.
    pub fn new(app_id: impl Into<String>) -> Self {
        let app_id = app_id.into();
        let (width, height) = theme::WINDOW_SIZE;
        let (min_width, min_height) = theme::WINDOW_MIN_SIZE;
        Self {
            routes: None,
            usage: format!("Usage: {app_id} [URI]"),
            app_id,
            title: "",
            size: (width as f32, height as f32),
            min_size: (min_width as f32, min_height as f32),
            borderless: false,
            transparent: false,
            normalise: None,
            about: None,
        }
    }

    /// The app's routes: its navigation tree as data. Every page, subpage and
    /// folder is a location of the table's scheme (`bingux-settings://network`),
    /// and a bare path on the command line takes that scheme. See
    /// [`crate::nav`].
    pub fn routes(mut self, routes: RouteTable) -> Self {
        self.routes = Some(routes);
        self
    }

    pub fn title(mut self, title: &'static str) -> Self {
        self.title = title;
        self
    }

    /// Who the application is, for its About window: name, id, version, icon,
    /// description, website, issue tracker, licence and authors. Build it with
    /// [`app_info!`](crate::app_info), which reads `Cargo.toml`:
    ///
    /// ```ignore
    /// Application::new("bingux-files").about(bingux_ui::app_info!(name = "Files"))
    /// ```
    ///
    /// Without it the About window still exists, with the id and title the
    /// application launched with, and `scripts/ui-lint.py` fails the crate.
    /// What the info leaves empty (the id, the name, the icon) is filled from
    /// [`Application::new`] and [`Application::title`].
    pub fn about(mut self, info: crate::about::AppInfo) -> Self {
        self.about = Some(info);
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = (width, height);
        self
    }

    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = (width, height);
        self
    }

    /// No system title bar: the compositor draws the window's corners, border
    /// and shadow, so the window is opaque and square.
    pub fn borderless(mut self) -> Self {
        self.borderless = true;
        self
    }

    /// Windows are made with an alpha channel, so a window can be drawn
    /// faded and the windows under it show through (Files fades the window it
    /// is carrying over another window's tab strip, so the strip's marker
    /// can be seen). Everything in the window is still painted opaque; a
    /// window that never fades looks and costs the same.
    pub fn transparent(mut self) -> Self {
        self.transparent = true;
        self
    }

    /// Turn the command line's argument into the text of an address before
    /// anything reads it, in this copy and in the copy it is handed to. An
    /// application whose addresses name files makes a relative path absolute
    /// here, because the running copy does not share this process's folder.
    /// An empty argument, which only raises the running window, is not passed.
    /// The argument is the process's own bytes (`OsStr`): a file name that is
    /// not valid UTF-8 is a valid argument, and the address built from it
    /// must name that exact file. An option (`-h`) is not passed either.
    pub fn normalise(mut self, normalise: fn(&std::ffi::OsStr) -> String) -> Self {
        self.normalise = Some(normalise);
        self
    }

    /// The text `--help` prints, normally the address forms the application
    /// understands.
    pub fn usage(mut self, usage: impl Into<String>) -> Self {
        self.usage = usage.into();
        self
    }

    /// Run the application with `root` as its window content. Returns when
    /// the window closes, or straight away when another copy took the
    /// address.
    pub fn run<E: IntoElement + 'static>(self, root: impl Fn() -> E + 'static) {
        let table = Arc::new(
            self.routes
                .clone()
                .unwrap_or_else(|| RouteTable::new(&self.app_id)),
        );
        let scheme = table.scheme().to_string();
        let _ = IDENTITY.set((self.app_id.clone(), self.title));
        tune_allocator();
        // Panics are never fatal: the panic hook, the failure log and the
        // relaunch plan exist before the first window and the first thread.
        crate::resilience::install_for_app();
        let normalise = self.normalise;
        // `args()` panics on an argument that is not valid UTF-8, which a file
        // name may be; `args_os()` never does.
        let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        let launch = match Launch::parse(&arguments) {
            Ok(launch) => launch,
            Err(error) => {
                eprintln!("{}: {error}\n{}", self.app_id, self.usage);
                std::process::exit(2);
            }
        };
        if launch.help {
            println!("{}", self.usage);
            return;
        }
        // One address per argument, in order. An empty argument names nothing
        // and only raises a running copy. Text that is not a URI is not an
        // error: the app opens its root and says so.
        let addresses: Vec<Option<Location>> = launch
            .addresses
            .iter()
            .map(|text| {
                let text = match normalise {
                    Some(normalise) if !text.is_empty() => normalise(text),
                    // No address form to build: the text is a location as typed,
                    // and one that is not UTF-8 is not a location (it is
                    // reported below).
                    _ => text.to_string_lossy().into_owned(),
                };
                if text.is_empty() {
                    return None;
                }
                let parsed = Location::parse_in(&scheme, &text).unwrap_or_else(|| {
                    eprintln!("{}: not a location: {text}", self.app_id);
                    table.home()
                });
                Some(table.canonical(&parsed))
            })
            .collect();
        let initial = addresses
            .first()
            .cloned()
            .flatten()
            .unwrap_or_else(|| table.canonical(&table.home()));
        // What a running copy is told, one line each: no argument only raises it.
        let forwarded: Vec<String> = if addresses.is_empty() {
            vec![String::new()]
        } else {
            addresses
                .iter()
                .map(|location| {
                    location
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default()
                })
                .collect()
        };

        let socket = socket_path(&self.app_id);
        let listener = match UnixStream::connect(&socket) {
            Ok(running) => {
                // One connection per address: the listener reads one line each.
                let mut running = Some(running);
                for line in &forwarded {
                    let sent = match running.take() {
                        Some(mut stream) => writeln!(stream, "{line}"),
                        None => UnixStream::connect(&socket)
                            .and_then(|mut stream| writeln!(stream, "{line}")),
                    };
                    if let Err(error) = sent {
                        eprintln!("{}: could not reach the running copy: {error}", self.app_id);
                        std::process::exit(1);
                    }
                }
                return;
            }
            Err(_) => {
                // Nothing answered: any socket file is left over from a copy
                // that is gone.
                let _ = std::fs::remove_file(&socket);
                match UnixListener::bind(&socket) {
                    Ok(listener) => Some(listener),
                    Err(error) => {
                        eprintln!(
                            "{}: running without a single-instance socket: {error}",
                            self.app_id
                        );
                        None
                    }
                }
            }
        };
        let inbox = Arc::new(Inbox::default());
        // The first address starts the window; the others are what a second
        // launch would have handed it.
        for line in forwarded.iter().skip(1).filter(|line| !line.is_empty()) {
            inbox.push(line.clone());
        }
        if let Some(listener) = listener {
            let inbox = Arc::clone(&inbox);
            std::thread::spawn(move || listen(listener, &inbox));
        }
        let _ = INBOX.set(Arc::clone(&inbox));
        FORWARDED.with(|slot| *slot.borrow_mut() = Some(inbox));

        // Anything that needs a process or a font scan to answer is asked now,
        // before the window exists, never on the first frame that wants it.
        let _ = crate::typography::mono_family();
        let _ = theme::font_family();
        // The About window's facts: the app's own, with what it left out taken
        // from the id and title it launched with. The system's side of them
        // (OS, session, graphics) is read now, not when the window is asked for.
        crate::about::register(
            self.about
                .clone()
                .unwrap_or_default()
                .completed(&self.app_id, self.title),
        );
        crate::about::read_facts();

        let (width, height) = self.size;
        let (min_width, min_height) = self.min_size;
        let title = self.title;
        let app_id = self.app_id.clone();
        // The window's content is built once, so the boundary below it sees the
        // same closure on every render of this root and does not rebuild the
        // whole window when the location changes.
        let root: Rc<dyn Fn() -> Element> = Rc::new(move || root().into_element());
        let boundary_name = if title.is_empty() { "The app" } else { title };
        let blueprint = Blueprint {
            table,
            root,
            title,
            app_id,
            min_size: (min_width, min_height),
            borderless: self.borderless,
            transparent: self.transparent,
            boundary_name,
        };
        let window = blueprint.window(initial, (width, height), None);
        SPAWNER.with(|spawner| *spawner.borrow_mut() = Some(blueprint));
        launch_application(window, self.borderless);
    }
}

/// What it takes to open one more window of the application: the same root,
/// routes, title and window attributes as the first. [`Application::run`]
/// keeps one for [`open_window`].
#[derive(Clone)]
struct Blueprint {
    table: Arc<RouteTable>,
    root: Rc<dyn Fn() -> Element>,
    title: &'static str,
    app_id: String,
    min_size: (f32, f32),
    borderless: bool,
    transparent: bool,
    boundary_name: &'static str,
}

impl Blueprint {
    /// The window opening on `initial`, `size` big. A window with a `seed`
    /// hands it to its root, which takes it with [`use_window_seed`].
    fn window(&self, initial: Location, size: (f32, f32), seed: Option<SeedCell>) -> WindowConfig {
        let (table, root, boundary_name) =
            (self.table.clone(), self.root.clone(), self.boundary_name);
        let window = WindowConfig::new(move || {
            // From here on the thread is the UI thread: blocking calls are reported.
            crate::runtime::mark_ui_thread();
            let router = provide_router(table.clone(), &initial);
            if let Some(seed) = &seed {
                let seed = seed.clone();
                use_provide_context(move || seed.clone());
            }
            use_hook(move || {
                // The first window takes the inbox it was started with; a
                // window opened later reads the same one, so a second launch
                // is answered by whichever window is there.
                let inbox = FORWARDED
                    .with(|slot| slot.borrow_mut().take())
                    .or_else(|| INBOX.get().cloned());
                if let Some(inbox) = inbox {
                    start_forwarding(inbox, router);
                }
            });
            // A relaunch after an unrecoverable failure opens this location.
            crate::resilience::note_location(&crate::nav::use_location().to_string());
            crate::resilience::use_window_notices();
            // The last resort: whatever fails below and has no boundary of its
            // own fails into this one, as a full-window error card.
            crate::resilience::window_boundary(boundary_name, root.clone()).into_element()
        })
        .with_title(self.title)
        .with_app_id(self.app_id.clone())
        .with_size(f64::from(size.0), f64::from(size.1))
        .with_min_size(f64::from(self.min_size.0), f64::from(self.min_size.1));
        // An alpha channel is not enough: freya clears the canvas to the window's background
        // (white by default) before it draws, so a root drawn at 0.4 opacity would be mixed with
        // white, not with the windows under it. The clear must be transparent too.
        // ui-lint: allow(the window's own clear colour, not a box)
        let window = if self.transparent {
            window
                .with_transparency(true)
                .with_background(Color::TRANSPARENT)
        } else {
            window
        };
        if self.borderless {
            window.with_decorations(false)
        } else {
            window
        }
    }
}

thread_local! {
    /// The blueprint of the running application, for [`open_window`].
    static SPAWNER: RefCell<Option<Blueprint>> = const { RefCell::new(None) };
}

/// What a new window is handed at birth, once.
#[derive(Clone)]
struct SeedCell(Rc<RefCell<Option<Box<dyn std::any::Any>>>>);

/// Open another window of this application, on `location` (the app's home
/// for `None`), `size` big, and hand it `seed`, which its root takes with
/// [`use_window_seed`]. Resolves to the new window once it exists, or `None`
/// when the application has not started its first window. Call it from a
/// task (`spawn`) of a window of the application.
///
/// The window is an **independent toplevel**: it stacks, raises and closes on
/// its own (a second Files window, a window a tab was torn off into). A window
/// that belongs to another (a dialog or tool window that must stay above its
/// parent and go with it) is opened with [`open_child_window`].
pub async fn open_window<T: 'static>(
    seed: T,
    location: Option<&str>,
    size: (f32, f32),
) -> Option<freya::winit::window::WindowId> {
    open_window_under(None, seed, location, size).await
}

/// Like [`open_window`], but the new window is a child of `parent`: the compositor keeps it above
/// `parent` and moves it with it (`xdg_toplevel.set_parent`), and the window stack
/// (`layers::WindowStack`) records the link.
pub async fn open_child_window<T: 'static>(
    parent: freya::winit::window::WindowId,
    seed: T,
    location: Option<&str>,
    size: (f32, f32),
) -> Option<freya::winit::window::WindowId> {
    open_window_under(Some(parent), seed, location, size).await
}

async fn open_window_under<T: 'static>(
    parent: Option<freya::winit::window::WindowId>,
    seed: T,
    location: Option<&str>,
    size: (f32, f32),
) -> Option<freya::winit::window::WindowId> {
    let blueprint = SPAWNER.with(|spawner| spawner.borrow().clone())?;
    let scheme = blueprint.table.scheme().to_string();
    let initial = match location {
        Some(text) => Location::parse_in(&scheme, text).unwrap_or_else(|| blueprint.table.home()),
        None => blueprint.table.home(),
    };
    let initial = blueprint.table.canonical(&initial);
    let cell = SeedCell(Rc::new(RefCell::new(Some(
        Box::new(seed) as Box<dyn std::any::Any>
    ))));
    let mut window = blueprint
        .window(initial, size, Some(cell))
        .with_on_close(|_, id| {
            crate::layers::with_stack(|stack| stack.close(u64::from(id)));
            freya_winit::config::CloseDecision::Close
        });
    if let Some(parent) = parent {
        window = window.with_parent(parent);
    }
    let id = Platform::get().launch_window(window).await;
    crate::layers::with_stack(|stack| {
        let parent_id = parent.map(u64::from);
        if let Some(parent) = parent_id
            && !stack.contains(parent)
        {
            stack.open(
                parent,
                crate::layers::WindowKind::Toplevel,
                None,
                false,
                "window",
            );
        }
        let kind = if parent.is_some() {
            crate::layers::WindowKind::Dialog
        } else {
            crate::layers::WindowKind::Toplevel
        };
        stack.open(u64::from(id), kind, parent_id, false, "window");
    });
    Some(id)
}

/// The seed this window was opened with by [`open_window`], taken the first
/// time the root asks and kept for its life. `None` for a window the person
/// opened by launching the application, or of another type.
pub fn use_window_seed<T: Clone + 'static>() -> Option<T> {
    use_hook(|| {
        try_consume_context::<SeedCell>()
            .and_then(|cell| cell.0.borrow_mut().take())
            .and_then(|seed| seed.downcast::<T>().ok())
            .map(|seed| *seed)
    })
}

/// The one place a bingux-ui process starts its event loop. It builds the
/// launch configuration, with the interface font and the inspector plugin, so
/// every window of every app has the inspector (Ctrl+Alt+Shift+I) without any
/// code of its own. [`Application::run`] is the only caller, and
/// `scripts/ui-lint.py` fails on `LaunchConfig::new()` or `freya::launch`
/// anywhere else.
fn launch_application(window: WindowConfig, borderless: bool) {
    crate::warm::enable();
    let mut config = theme::with_font(LaunchConfig::new()).with_plugin(crate::inspector::plugin());
    if borderless {
        config = config.with_plugin(freya::borderless::BorderlessPlugin::new());
    }
    launch(config.with_window(window));
}

/// Keep glibc's heap from holding on to freed memory. The default lets every
/// worker thread open its own arena and raises the threshold for mapping big
/// buffers up to 32 MiB, so decoded pictures and page models freed by one
/// thread stay in the process. Two arenas, and buffers over 256 KiB mapped
/// and unmapped on their own, hand that memory back.
fn tune_allocator() {
    // SAFETY: `mallopt` only sets allocator parameters. It runs first in
    // `run`, before the worker threads exist.
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
        libc::mallopt(libc::M_MMAP_THRESHOLD, 256 * 1024);
        libc::mallopt(libc::M_TRIM_THRESHOLD, 1024 * 1024);
    }
}

/// The inbox of the running application, for [`open_uri`] from any thread.
static INBOX: std::sync::OnceLock<Arc<Inbox>> = std::sync::OnceLock::new();

/// Open `uri` in this application's window from any thread, as a second launch
/// would: the window's task wakes, navigates (a deep link: the stack is the
/// location's ancestors) and raises the window. A search action, a D-Bus
/// `Activate` or a socket thread calls this. Returns false when the
/// application has not started yet, so nothing was queued.
pub fn open_uri(uri: &str) -> bool {
    match INBOX.get() {
        Some(inbox) => {
            inbox.push(uri.to_string());
            true
        }
        None => false,
    }
}

thread_local! {
    /// The forwarded addresses, from the listener thread to the window's
    /// root. The root takes it once, when the window opens.
    static FORWARDED: RefCell<Option<Arc<Inbox>>> = const { RefCell::new(None) };
}

/// `$XDG_RUNTIME_DIR/<app-id>.sock`, or the temporary directory when the
/// runtime directory is not set.
fn socket_path(app_id: &str) -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join(format!("{app_id}.sock"))
}

/// The addresses forwarded by other copies, waiting for the window. The
/// listener thread pushes and wakes the window's task, so the window sleeps
/// until a copy actually hands off: no timer runs while the app is idle.
#[derive(Default)]
struct Inbox {
    state: Mutex<InboxState>,
}

#[derive(Default)]
struct InboxState {
    queue: VecDeque<String>,
    /// The windows waiting for an address: more than one when the application
    /// has opened more windows than its first.
    wakers: Vec<Waker>,
}

impl Inbox {
    fn push(&self, line: String) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.queue.push_back(line);
        for waker in state.wakers.drain(..) {
            waker.wake();
        }
    }

    async fn next(&self) -> String {
        std::future::poll_fn(|cx| {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            match state.queue.pop_front() {
                Some(line) => Poll::Ready(line),
                None => {
                    if !state.wakers.iter().any(|other| other.will_wake(cx.waker())) {
                        state.wakers.push(cx.waker().clone());
                    }
                    Poll::Pending
                }
            }
        })
        .await
    }
}

/// Accept connections and pass each line on to the window. A client sends
/// one address per connection; an empty line only raises the window.
fn listen(listener: UnixListener, inbox: &Inbox) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // A client that connects and says nothing must not hold the listener.
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_ok() {
            inbox.push(line.trim_end().to_string());
        }
    }
}

/// Open each forwarded location on the window's thread and raise the window.
///
/// A hierarchical app (Settings) navigates: the location is a deep link and
/// the stack becomes its ancestors. A linear app (Files) keeps its history
/// in its own tabs, so the window's router only carries the location the
/// window was last asked for, as a stack of one: a launch never leaves an
/// entry in any history.
fn start_forwarding(inbox: Arc<Inbox>, router: Router) {
    spawn(async move {
        loop {
            let text = inbox.next().await;
            if !text.is_empty() {
                let table = router.table();
                match (
                    table.stack_mode(),
                    Location::parse_in(table.scheme(), &text),
                ) {
                    (Mode::Linear, Some(location)) => router.reset(&location),
                    (Mode::Hierarchical, Some(location)) => router.navigate_to(&location),
                    (_, None) => {}
                }
            }
            Platform::get().focus_window(Platform::window_id());
        }
    });
}

/// What a command line asks for: the options it holds and the addresses it
/// names. An option is never an address.
#[derive(Debug, PartialEq, Eq)]
struct Launch {
    help: bool,
    addresses: Vec<std::ffi::OsString>,
}

/// Why a command line was refused.
#[derive(Debug, PartialEq, Eq)]
struct UnknownOption(String);

impl std::fmt::Display for UnknownOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown option '{}'", self.0)
    }
}

impl Launch {
    /// Split `arguments` (without the program name) into options and
    /// addresses. `--help` and `-h` ask for the usage; every other argument
    /// that starts with `-` is an unknown option, not a location, so a
    /// mistyped flag can never reach the app as a place to open. `--` ends the
    /// options, so a folder called `--odd` is opened as `-- --odd`. A lone `-`
    /// and every argument that does not start with `-` is an address.
    fn parse(arguments: &[std::ffi::OsString]) -> Result<Self, UnknownOption> {
        let mut launch = Launch {
            help: false,
            addresses: Vec::new(),
        };
        let mut options_end = false;
        for argument in arguments {
            let bytes = argument.as_encoded_bytes();
            if options_end || bytes == b"-" || !bytes.starts_with(b"-") {
                launch.addresses.push(argument.clone());
            } else if bytes == b"--" {
                options_end = true;
            } else if bytes == b"--help" || bytes == b"-h" {
                launch.help = true;
            } else {
                return Err(UnknownOption(argument.to_string_lossy().into_owned()));
            }
        }
        Ok(launch)
    }
}

#[cfg(test)]
mod launch_tests {
    use std::ffi::OsString;

    use super::*;

    fn parse(arguments: &[&str]) -> Result<Launch, UnknownOption> {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        Launch::parse(&arguments)
    }

    fn addresses(arguments: &[&str]) -> Vec<String> {
        parse(arguments)
            .expect("a command line")
            .addresses
            .iter()
            .map(|text| text.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn no_argument_names_nothing() {
        assert_eq!(
            parse(&[]),
            Ok(Launch {
                help: false,
                addresses: vec![]
            })
        );
    }

    #[test]
    fn a_path_is_an_address_and_an_option_is_not() {
        assert_eq!(
            addresses(&["/usr", "docs", "~", "recent:///", "file:///tmp", "", "-"]),
            ["/usr", "docs", "~", "recent:///", "file:///tmp", "", "-"]
        );
    }

    #[test]
    fn help_is_asked_for_by_name() {
        assert!(parse(&["--help"]).expect("help").help);
        assert!(parse(&["-h"]).expect("help").help);
        assert!(parse(&["/usr", "-h"]).expect("help").help);
    }

    #[test]
    fn an_unknown_option_is_refused_whatever_it_looks_like() {
        for option in [
            "--snapshot",
            "--snapshot=/tmp/x",
            "-x",
            "---",
            "-1",
            "--HELP",
            "-hh",
        ] {
            let result = parse(&[option]);
            assert!(result.is_err(), "{option}: {result:?}");
        }
        let error = parse(&["--snapshot", "/tmp"]).expect_err("refused");
        assert_eq!(error.to_string(), "unknown option '--snapshot'");
        // Nothing before it is opened: the whole command line is refused.
        assert!(parse(&["/usr", "--snapshot"]).is_err());
    }

    #[test]
    fn two_dashes_end_the_options() {
        assert_eq!(addresses(&["--", "--odd", "-x"]), ["--odd", "-x"]);
        assert_eq!(addresses(&["/a", "--", "-b"]), ["/a", "-b"]);
        assert!(parse(&["--", "--help"]).is_ok_and(|launch| !launch.help));
    }

    #[test]
    fn a_name_that_is_not_utf8_is_an_address_or_an_unknown_option_by_its_first_byte() {
        use std::os::unix::ffi::OsStringExt;
        let odd = OsString::from_vec(b"/tmp/caf\xe9".to_vec());
        assert_eq!(
            Launch::parse(&[odd.clone()]).expect("an address").addresses,
            [odd]
        );
        let option = OsString::from_vec(b"-caf\xe9".to_vec());
        assert!(Launch::parse(&[option]).is_err());
    }
}
