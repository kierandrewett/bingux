//! The writer of an open profile: one thread that owns its files, so nothing
//! the window does waits for a disk. The window sends what changed; the thread
//! writes it and answers with how keeping went ([`Event`]), waking the window
//! the way the engine's events do.
//!
//! A file this build could not read (damaged, or from a newer browser) is never
//! written over: its part stays [`NotSaved`] until the person chooses Start
//! over, which moves the file aside first.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Waker;
use std::thread::JoinHandle;

use super::files::{self, Problem};
use super::places::Folders;
use super::settings::{Settings, SettingsProblem};
use crate::model::keeping::{Keeping, NotSaved, damaged_name};

/// How keeping a part went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Settings(Keeping),
}

enum Command {
    Settings(Vec<u8>),
    StartOverSettings(Vec<u8>),
    Flush(Sender<()>),
}

/// The state a settings problem at open gives.
pub fn keeping_of(problem: Option<&SettingsProblem>) -> Keeping {
    match problem {
        None => Keeping::Saved,
        Some(SettingsProblem::Damaged(detail)) => Keeping::NotSaved(NotSaved::Damaged {
            detail: detail.clone(),
        }),
        Some(SettingsProblem::TooNew { found, supported }) => Keeping::NotSaved(NotSaved::TooNew {
            found: *found,
            supported: *supported,
        }),
        Some(SettingsProblem::Io(problem)) => not_saved(problem),
    }
}

/// The state a failed write gives.
pub fn not_saved(problem: &Problem) -> Keeping {
    Keeping::NotSaved(match problem {
        Problem::DiskFull => NotSaved::DiskFull,
        Problem::ReadOnly => NotSaved::ReadOnly,
        Problem::Other(text) => NotSaved::Failed(text.clone()),
    })
}

/// The events the thread has sent and the waker that tells the window.
#[derive(Default)]
struct Outbox {
    events: Vec<Event>,
    waker: Option<Waker>,
}

impl Outbox {
    fn send(outbox: &Mutex<Self>, event: Event) {
        let mut outbox = outbox.lock().unwrap_or_else(PoisonError::into_inner);
        outbox.events.push(event);
        if let Some(waker) = &outbox.waker {
            waker.wake_by_ref();
        }
    }
}

/// The writer of one open profile. Dropping it writes what is queued and
/// stops the thread.
pub struct Store {
    commands: Option<Sender<Command>>,
    outbox: Arc<Mutex<Outbox>>,
    thread: Option<JoinHandle<()>>,
}

/// What the thread owns.
struct Writer {
    settings_path: PathBuf,
    settings: Keeping,
    outbox: Arc<Mutex<Outbox>>,
}

impl Writer {
    fn report_settings(&mut self, keeping: Keeping) {
        if keeping != self.settings {
            self.settings = keeping.clone();
            Outbox::send(&self.outbox, Event::Settings(keeping));
        }
    }

    fn write_settings(&mut self, bytes: &[u8]) {
        // What could not be read is never written over.
        if matches!(
            self.settings,
            Keeping::NotSaved(NotSaved::Damaged { .. } | NotSaved::TooNew { .. })
        ) {
            return;
        }
        self.replace_settings(bytes);
    }

    fn replace_settings(&mut self, bytes: &[u8]) {
        let keeping = match files::replace(&self.settings_path, bytes) {
            Ok(()) => Keeping::Saved,
            Err(error) => not_saved(&Problem::of(&error)),
        };
        self.report_settings(keeping);
    }

    fn start_over_settings(&mut self, bytes: &[u8]) {
        let Keeping::NotSaved(NotSaved::Damaged { detail }) = &self.settings else {
            return;
        };
        let aside = damaged_name(&self.settings_path);
        if let Err(error) = std::fs::rename(&self.settings_path, &aside)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            // Still damaged, so still never written over; the page says why
            // Start over did not work.
            let detail = format!("{detail}; it could not be moved aside: {error}");
            self.report_settings(Keeping::NotSaved(NotSaved::Damaged { detail }));
            return;
        }
        self.replace_settings(bytes);
    }

    fn run(mut self, commands: Receiver<Command>) {
        while let Ok(command) = commands.recv() {
            match command {
                Command::Settings(bytes) => self.write_settings(&bytes),
                Command::StartOverSettings(bytes) => self.start_over_settings(&bytes),
                Command::Flush(done) => {
                    let _ = done.send(());
                }
            }
        }
    }
}

impl Store {
    /// Starts the writer of the profile in `folders`. `settings` is how the
    /// settings read at open went.
    pub fn open(folders: &Folders, settings: Keeping) -> std::io::Result<Self> {
        let outbox = Arc::new(Mutex::new(Outbox::default()));
        let writer = Writer {
            settings_path: folders.settings(),
            settings,
            outbox: Arc::clone(&outbox),
        };
        let (commands, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("browser-store".into())
            .spawn(move || writer.run(receiver))?;
        Ok(Self {
            commands: Some(commands),
            outbox,
            thread: Some(thread),
        })
    }

    fn send(&self, command: Command) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(command);
        }
    }

    /// Keep `settings`.
    pub fn save_settings(&self, settings: &Settings) {
        self.send(Command::Settings(settings.encode()));
    }

    /// Move the damaged settings file aside and keep `settings` from now on.
    pub fn start_over_settings(&self, settings: &Settings) {
        self.send(Command::StartOverSettings(settings.encode()));
    }

    /// Wake `waker` when an event is waiting.
    pub fn set_waker(&self, waker: Waker) {
        let mut outbox = self.outbox.lock().unwrap_or_else(PoisonError::into_inner);
        if !outbox.events.is_empty() {
            waker.wake_by_ref();
        }
        outbox.waker = Some(waker);
    }

    /// The events not taken yet, oldest first.
    pub fn take_events(&self) -> Vec<Event> {
        std::mem::take(
            &mut self
                .outbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .events,
        )
    }

    /// Waits until everything sent before has been written. For tests and
    /// for the window's last moment, never for a frame.
    pub fn flush(&self) {
        let (done, wait) = mpsc::channel();
        self.send(Command::Flush(done));
        let _ = wait.recv();
    }
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // Closing the channel ends the thread once the queue is written.
        self.commands = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
