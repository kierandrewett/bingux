//! The application loop: one per process, like GtkApplication.
//!
//! [`Application::run`] reads the address named on the command line, makes
//! sure only one copy of the application runs, and opens the window.
//!
//! * The current address lives in a root context. Components read it with
//!   [`use_address`] and move to another with [`navigate`]. Both are
//!   reactive, so a page that shows the address redraws when it changes.
//! * A second launch does not open a second window. It sends its address to
//!   the running copy over a unix socket in `$XDG_RUNTIME_DIR` and exits 0.
//!   The running copy opens that address and raises its window. A socket left
//!   behind by a crash is removed and replaced.

use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Poll, Waker};
use std::time::Duration;

use freya::prelude::*;

use crate::address::Address;
use crate::theme;

/// The settings of one application: its identity, its window and the usage
/// text `--help` prints.
pub struct Application {
    app_id: String,
    scheme: String,
    title: &'static str,
    size: (f32, f32),
    min_size: (f32, f32),
    borderless: bool,
    usage: String,
}

impl Application {
    /// An application with id `app_id`. Its addresses use the same scheme
    /// unless [`Application::scheme`] says otherwise.
    pub fn new(app_id: impl Into<String>) -> Self {
        let app_id = app_id.into();
        let (width, height) = theme::WINDOW_SIZE;
        let (min_width, min_height) = theme::WINDOW_MIN_SIZE;
        Self {
            scheme: app_id.clone(),
            usage: format!("Usage: {app_id} [ADDRESS]"),
            app_id,
            title: "",
            size: (width as f32, height as f32),
            min_size: (min_width as f32, min_height as f32),
            borderless: false,
        }
    }

    /// The scheme of this application's addresses, for example `bingux-settings`.
    /// A bare path on the command line takes this scheme.
    pub fn scheme(mut self, scheme: impl Into<String>) -> Self {
        self.scheme = scheme.into();
        self
    }

    pub fn title(mut self, title: &'static str) -> Self {
        self.title = title;
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
        tune_allocator();
        let argument = std::env::args().nth(1);
        if matches!(argument.as_deref(), Some("--help") | Some("-h")) {
            println!("{}", self.usage);
            return;
        }
        // No argument opens the root address. An argument that names nothing
        // (an empty string) only raises a running copy.
        let initial = match argument.as_deref() {
            None => Address::new(self.scheme.clone()),
            Some("") => Address::new(self.scheme.clone()),
            Some(text) => match Address::parse_in(&self.scheme, text) {
                Some(address) => address,
                None => {
                    eprintln!("{}: not an address: {text}", self.app_id);
                    std::process::exit(2);
                }
            },
        };
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
        FORWARDED.with(|slot| *slot.borrow_mut() = Some(inbox));

        let scheme = self.scheme.clone();
        let (width, height) = self.size;
        let (min_width, min_height) = self.min_size;
        let title = self.title;
        let app_id = self.app_id.clone();
        let window = WindowConfig::new(move || {
            let address = provide_address(initial.clone());
            let scheme = scheme.clone();
            use_hook(move || {
                if let Some(inbox) = FORWARDED.with(|slot| slot.borrow_mut().take()) {
                    start_forwarding(inbox, address, scheme);
                }
            });
            root()
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
        let mut config =
            theme::with_font(LaunchConfig::new()).with_plugin(crate::inspector::plugin());
        if self.borderless {
            config = config.with_plugin(freya::borderless::BorderlessPlugin::new());
        }
        launch(config.with_window(window));
    }
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

thread_local! {
    /// The forwarded addresses, from the listener thread to the window's
    /// root. The root takes it once, when the window opens.
    static FORWARDED: RefCell<Option<Arc<Inbox>>> = const { RefCell::new(None) };
    /// The window's address state, read by [`navigate`].
    static WINDOW_ADDRESS: Cell<Option<State<Address>>> = const { Cell::new(None) };
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

/// Open each forwarded address on the window's thread and raise the window.
fn start_forwarding(inbox: Arc<Inbox>, address: State<Address>, scheme: String) {
    spawn(async move {
        let mut address = address;
        loop {
            let text = inbox.next().await;
            if !text.is_empty()
                && let Some(target) = Address::parse_in(&scheme, &text)
            {
                address.set(target);
            }
            Platform::get().focus_window(Platform::window_id());
        }
    });
}

/// Provide the window's address, starting at `initial`, to everything below
/// the window root. [`Application::run`] calls this; a snapshot calls it to
/// render a page at an address without a running application.
pub fn provide_address(initial: Address) -> State<Address> {
    let address = use_provide_root_context(move || State::create(initial));
    // Event handlers have no component scope, so `navigate` finds the window's
    // address through this slot rather than through context.
    WINDOW_ADDRESS.with(|slot| slot.set(Some(address)));
    address
}

/// The current address. Reactive: a component that reads it redraws when the
/// address changes.
pub fn use_address() -> Address {
    let address = use_consume::<State<Address>>();
    let current = (*address.read()).clone();
    current
}

/// Moves the window to `address`, from any component or event handler. The
/// page or popup that address names replaces the current one.
pub fn navigate(address: Address) {
    let mut current = WINDOW_ADDRESS
        .with(Cell::get)
        .expect("navigate called before the window opened");
    current.set(address);
}

/// The address the window shows right now, read without subscribing. Event
/// handlers use it to work out a parent, such as Back.
pub fn current_address() -> Option<Address> {
    WINDOW_ADDRESS
        .with(Cell::get)
        .map(|current| (*current.peek()).clone())
}
