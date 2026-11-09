//! What a running window needs from the machine besides the files: writes
//! in the order they were asked for, a minute clock, news that another
//! program changed the store, and reminders.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{NaiveDateTime, Utc};

use crate::alarm::{self, Delivery};
use crate::recur::Change;
use crate::store::Store;
use crate::zone::Zone;

/// Saves changes on a thread of its own, one after another, so two quick
/// edits are written in the order they were made and the window never waits
/// for the disk.
#[derive(Clone)]
pub struct Writer {
    sender: Sender<Vec<Change>>,
    known: Arc<AtomicU64>,
}

impl Writer {
    /// `on_error` is told, in words, when a write fails.
    pub fn new(store: Store, on_error: impl Fn(String) + Send + 'static) -> Writer {
        let (sender, receiver) = mpsc::channel::<Vec<Change>>();
        let known = Arc::new(AtomicU64::new(store.fingerprint()));
        let seen = known.clone();
        thread::Builder::new()
            .name("calendar-writer".into())
            .spawn(move || {
                for changes in receiver {
                    if let Err(error) = store.write(&changes) {
                        on_error(format!("{error}"));
                    }
                    seen.store(store.fingerprint(), Ordering::Relaxed);
                }
            })
            .ok();
        Writer { sender, known }
    }

    /// Queues `changes` to be written.
    pub fn write(&self, changes: Vec<Change>) {
        let _ = self.sender.send(changes);
    }

    /// The fingerprint the store had after the last write made here: a store
    /// that has another one was changed by someone else.
    pub fn known(&self) -> u64 {
        self.known.load(Ordering::Relaxed)
    }

    /// Tells the writer the store is as it is now, after the window read it.
    pub fn accept(&self, fingerprint: u64) {
        self.known.store(fingerprint, Ordering::Relaxed);
    }
}

/// What a watcher reports.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The minute changed; the time now on this computer's clock.
    Minute(NaiveDateTime),
    /// Another program changed the store.
    Changed,
    /// A reminder went off and no notification service showed it.
    Reminder { title: String, body: String },
}

/// A running watcher. Dropping it stops its threads.
pub struct Watcher {
    stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Starts watching. `on_event` is called on the watcher's threads, so it
/// must hand the event on (a queue the window reads) and return.
pub fn watch(
    store: Store,
    writer: Writer,
    twelve_hour: impl Fn() -> bool + Send + Sync + 'static,
    on_event: impl Fn(Event) + Send + Sync + 'static,
) -> Watcher {
    let stop = Arc::new(AtomicBool::new(false));
    let on_event = Arc::new(on_event);

    // The clock and the files.
    {
        let (stop, on_event, store, writer) =
            (stop.clone(), on_event.clone(), store.clone(), writer);
        thread::Builder::new()
            .name("calendar-watch".into())
            .spawn(move || {
                let mut last_minute = None;
                let mut ticks = 0u32;
                while !stop.load(Ordering::Relaxed) {
                    let zone = Zone::detect_local();
                    let now = zone.wall(Utc::now());
                    let minute = now.format("%Y%m%d%H%M").to_string();
                    if last_minute.as_deref() != Some(minute.as_str()) {
                        last_minute = Some(minute);
                        on_event(Event::Minute(now));
                    }
                    // The files, every two seconds.
                    if ticks % 4 == 0 {
                        let print = store.fingerprint();
                        if print != writer.known() {
                            writer.accept(print);
                            on_event(Event::Changed);
                        }
                    }
                    ticks = ticks.wrapping_add(1);
                    thread::sleep(Duration::from_millis(500));
                }
            })
            .ok();
    }

    // Reminders. The window shows one itself when nothing else could.
    {
        let (stop, on_event) = (stop.clone(), on_event);
        let slot: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        thread::Builder::new()
            .name("calendar-reminders".into())
            .spawn(move || {
                let _guard = slot;
                let stop_inner = stop.clone();
                alarm::run(&store, &stop_inner, twelve_hour, |alarm, delivery| {
                    if delivery == Delivery::None {
                        let (title, body) =
                            alarm::message(alarm, Utc::now(), Zone::detect_local(), false);
                        on_event(Event::Reminder { title, body });
                    }
                });
            })
            .ok();
    }

    Watcher { stop }
}
