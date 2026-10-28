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

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Poll, Waker};
use std::time::Duration;

use freya::prelude::*;

use crate::nav::{Location, RouteTable, Router, provide_router};
use crate::theme;

/// The settings of one application: its identity, its routes, its window and
/// the usage text `--help` prints.
pub struct Application {
    app_id: String,
    routes: Option<RouteTable>,
    title: &'static str,
    size: (f32, f32),
    min_size: (f32, f32),
    borderless: bool,
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
        tune_allocator();
        // Panics are never fatal: the panic hook, the failure log and the
        // relaunch plan exist before the first window and the first thread.
        crate::resilience::install_for_app();
        let normalise = self.normalise;
        // `args()` panics on an argument that is not valid UTF-8, which a file
        // name may be; `args_os()` never does.
        let argument = std::env::args_os().nth(1).map(|text| match normalise {
            Some(normalise) if !text.is_empty() && !text.as_encoded_bytes().starts_with(b"-") => {
                normalise(&text)
            }
            // No address form to build: the text is a location as typed, and
            // one that is not UTF-8 is not a location (it is reported below).
            _ => text.to_string_lossy().into_owned(),
        });
        if matches!(argument.as_deref(), Some("--help") | Some("-h")) {
            println!("{}", self.usage);
            return;
        }
        // No argument opens the app root. An argument that names nothing (an
        // empty string) only raises a running copy. Text that is not a URI is
        // not an error: the app opens its root and says so.
        let initial = match argument.as_deref() {
            None | Some("") => table.home(),
            Some(text) => Location::parse_in(&scheme, text).unwrap_or_else(|| {
                eprintln!("{}: not a location: {text}", self.app_id);
                table.home()
            }),
        };
        let initial = table.canonical(&initial);
        let forwarded = match argument.as_deref() {
            None | Some("") => String::new(),
            Some(_) => initial.to_string(),
        };

        let socket = socket_path(&self.app_id);
        let listener = match UnixStream::connect(&socket) {
            Ok(mut running) => {
                if let Err(error) = writeln!(running, "{forwarded}") {
                    eprintln!("{}: could not reach the running copy: {error}", self.app_id);
                    std::process::exit(1);
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
        let window = WindowConfig::new(move || {
            // From here on the thread is the UI thread: blocking calls are reported.
            crate::runtime::mark_ui_thread();
            let router = provide_router(table.clone(), &initial);
            use_hook(move || {
                if let Some(inbox) = FORWARDED.with(|slot| slot.borrow_mut().take()) {
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
        .with_title(title)
        .with_app_id(app_id)
        .with_size(width as f64, height as f64)
        .with_min_size(min_width as f64, min_height as f64);
        let window = if self.borderless {
            window.with_decorations(false)
        } else {
            window
        };
        launch_application(window, self.borderless);
    }
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
    waker: Option<Waker>,
}

impl Inbox {
    fn push(&self, line: String) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.queue.push_back(line);
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }

    async fn next(&self) -> String {
        std::future::poll_fn(|cx| {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            match state.queue.pop_front() {
                Some(line) => Poll::Ready(line),
                None => {
                    state.waker = Some(cx.waker().clone());
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
fn start_forwarding(inbox: Arc<Inbox>, router: Router) {
    spawn(async move {
        loop {
            let text = inbox.next().await;
            if !text.is_empty() {
                router.navigate(&text);
            }
            Platform::get().focus_window(Platform::window_id());
        }
    });
}
