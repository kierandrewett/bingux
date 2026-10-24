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
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
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
        let (sender, receiver) = mpsc::channel::<String>();
        if let Some(listener) = listener {
            std::thread::spawn(move || listen(listener, sender));
        }
        FORWARDED.with(|slot| *slot.borrow_mut() = Some(receiver));

        let scheme = self.scheme.clone();
        let (width, height) = self.size;
        let (min_width, min_height) = self.min_size;
        let title = self.title;
        let app_id = self.app_id.clone();
        let window = WindowConfig::new(move || {
            let address = provide_address(initial.clone());
            let scheme = scheme.clone();
            use_hook(move || {
                if let Some(receiver) = FORWARDED.with(|slot| slot.borrow_mut().take()) {
                    start_forwarding(receiver, address, scheme);
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
        let mut config = theme::with_font(LaunchConfig::new());
        if self.borderless {
            config = config.with_plugin(freya::borderless::BorderlessPlugin::new());
        }
        launch(config.with_window(window));
    }
}

thread_local! {
    /// The forwarded addresses, from the listener thread to the window's
    /// root. The root takes it once, when the window opens.
    static FORWARDED: RefCell<Option<Receiver<String>>> = const { RefCell::new(None) };
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

/// Accept connections and pass each line on to the window. A client sends
/// one address per connection; an empty line only raises the window.
fn listen(listener: UnixListener, sender: Sender<String>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // A client that connects and says nothing must not hold the listener.
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_ok()
            && sender.send(line.trim_end().to_string()).is_err()
        {
            // The window is gone: there is nothing left to forward to.
            return;
        }
    }
}

/// Watch for forwarded addresses on the window's thread. Each one opens its
/// address and raises the window. The check is a short poll, like the
/// background helpers in `runtime`, because the listener is a plain thread.
fn start_forwarding(receiver: Receiver<String>, address: State<Address>, scheme: String) {
    spawn(async move {
        let mut address = address;
        loop {
            match receiver.try_recv() {
                Ok(text) => {
                    if !text.is_empty()
                        && let Some(target) = Address::parse_in(&scheme, &text)
                    {
                        address.set(target);
                    }
                    Platform::get().focus_window(Platform::window_id());
                }
                Err(TryRecvError::Empty) => timer(Duration::from_millis(50)).await,
                Err(TryRecvError::Disconnected) => return,
            }
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
