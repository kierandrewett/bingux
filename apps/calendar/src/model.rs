//! What the window knows: the events, the preferences, which day and view
//! are on show, and the operations that change them. Every view reads this
//! and calls it; nothing else holds calendar state.

use std::collections::HashSet;
use std::sync::Arc;

use bingux_calendar_core::chrono::NaiveDateTime;
use bingux_calendar_core::recur::{self, Change, Scope};
use bingux_calendar_core::store::Loaded;
use bingux_calendar_core::watch::{Event as WatchEvent, Watcher, Writer};
use bingux_calendar_core::{Item, Key, Occurrence, Prefs, Snapshot, Store, ViewKind, Zone};
use bingux_ui::calendar::Date;
use bingux_ui::task::{Feed, spawn_blocking};
use freya::prelude::*;

use crate::convert;

/// The things that live as long as the window and are not plain state: the
/// store, the thread that writes to it, and the watcher.
pub struct Backend {
    pub store: Store,
    pub writer: Writer,
    _watcher: Watcher,
}

/// Today, the time now, and the clock this computer is set to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    pub today: Date,
    pub minutes: u32,
    pub zone: Zone,
}

impl Clock {
    pub fn now() -> Clock {
        let zone = Zone::detect_local();
        let now = zone.wall(bingux_calendar_core::chrono::Utc::now());
        Clock {
            today: convert::date_of(now.date()),
            minutes: convert::minutes_of(now),
            zone,
        }
    }
}

/// An event, or one occurrence of a repeating one, as a view names it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EventRef {
    pub uid: String,
    pub key: Option<Key>,
}

impl EventRef {
    pub fn of(occurrence: &Occurrence) -> EventRef {
        EventRef {
            uid: occurrence.uid.clone(),
            key: occurrence.key,
        }
    }

    /// The id a grid carries for it.
    pub fn id(&self) -> String {
        match self.key {
            Some(key) => format!("{}\u{1f}{}", self.uid, key.format("%Y%m%dT%H%M%S")),
            None => self.uid.clone(),
        }
    }

    pub fn parse(id: &str) -> EventRef {
        match id.split_once('\u{1f}') {
            Some((uid, key)) => EventRef {
                uid: uid.to_string(),
                key: NaiveDateTime::parse_from_str(key, "%Y%m%dT%H%M%S").ok(),
            },
            None => EventRef {
                uid: id.to_string(),
                key: None,
            },
        }
    }
}

/// Everything the views share. Cheap to copy: it is handles.
#[derive(Clone, Copy, PartialEq)]
pub struct Model {
    pub snapshot: State<Option<Arc<Snapshot>>>,
    pub backend: State<Option<Arc<Backend>>>,
    pub prefs: State<Prefs>,
    pub view: State<ViewKind>,
    pub anchor: State<Date>,
    pub clock: State<Clock>,
    pub selected: State<Option<String>>,
    pub query: State<String>,
    pub hint_dismissed: State<bool>,
    /// On a phone the search field has a row of its own, shown on request.
    pub search_open: State<bool>,
    pub scroll_nonce: State<u64>,
    pub warnings: State<Vec<String>>,
    /// The text of the dialog that asks for a calendar's name, its colour
    /// and the calendar an import goes in.
    pub draft_name: State<String>,
    pub draft_colour: State<String>,
    pub draft_target: State<String>,
}

/// What a thread tells the window.
pub enum Message {
    /// The clock, the files, or a reminder with nowhere to go.
    Watch(WatchEvent),
    /// A write to the disk failed.
    SaveFailed(String),
}

/// Where the threads leave what they tell the window.
pub type Feeder = Arc<Feed<Message>>;

impl Model {
    /// The model of the calling component. Call from the window's root only.
    pub fn create() -> Model {
        let prefs = Prefs::load();
        let clock = Clock::now();
        Model {
            snapshot: use_state(|| None),
            backend: use_state(|| None),
            view: use_state({
                let view = prefs.view;
                move || view
            }),
            prefs: use_state(move || prefs),
            anchor: use_state(move || clock.today),
            clock: use_state(move || clock),
            selected: use_state(|| None),
            query: use_state(String::new),
            hint_dismissed: use_state(|| false),
            search_open: use_state(|| false),
            scroll_nonce: use_state(|| 1),
            warnings: use_state(Vec::new),
            draft_name: use_state(String::new),
            draft_colour: use_state(String::new),
            draft_target: use_state(String::new),
        }
    }

    /// Reads the store on a worker and starts the writer and the watcher.
    /// The window draws an empty calendar meanwhile.
    pub fn start(self, feed: Feeder) {
        let mut snapshot = self.snapshot;
        let mut backend = self.backend;
        let mut warnings = self.warnings;
        spawn(async move {
            let (loaded, shared) = spawn_blocking(move || {
                let store = Store::open();
                let loaded = store.load();
                let error_feed = feed.clone();
                let writer = Writer::new(store.clone(), move |error| {
                    error_feed.push(Message::SaveFailed(error));
                });
                let watcher_feed = feed.clone();
                let watcher = bingux_calendar_core::watch::watch(
                    store.clone(),
                    writer.clone(),
                    || Prefs::load().twelve_hour(),
                    move |event| watcher_feed.push(Message::Watch(event)),
                );
                (
                    loaded,
                    Arc::new(Backend {
                        store,
                        writer,
                        _watcher: watcher,
                    }),
                )
            })
            .await;
            warnings.set(loaded.warnings);
            snapshot.set(Some(Arc::new(loaded.snapshot)));
            backend.set(Some(shared));
        });
    }

    /// Reads the store again, after another program changed it.
    pub fn reload(self) {
        let Some(backend) = self.backend.read().clone() else {
            return;
        };
        let mut snapshot = self.snapshot;
        let mut warnings = self.warnings;
        spawn(async move {
            let loaded: Loaded = spawn_blocking(move || backend.store.load()).await;
            warnings.set_if_modified(loaded.warnings);
            snapshot.set(Some(Arc::new(loaded.snapshot)));
        });
    }

    pub fn data(&self) -> Arc<Snapshot> {
        self.snapshot
            .read()
            .clone()
            .unwrap_or_else(|| Arc::new(Snapshot::default()))
    }

    pub fn loaded(&self) -> bool {
        self.snapshot.read().is_some()
    }

    pub fn zone(&self) -> Zone {
        self.clock.read().zone
    }

    pub fn first_weekday(&self) -> bingux_ui::calendar::Weekday {
        convert::weekday_of(self.prefs.read().first_day())
    }

    pub fn twelve_hour(&self) -> bool {
        self.prefs.read().twelve_hour()
    }

    pub fn hidden(&self) -> HashSet<String> {
        self.prefs.read().hidden_calendars.iter().cloned().collect()
    }

    /// What touches `from..to` (days, `to` not included), in the calendars on show.
    pub fn occurrences(&self, from: Date, to: Date) -> Vec<Occurrence> {
        self.data().occurrences(
            convert::midnight(from),
            convert::midnight(to),
            self.zone(),
            &self.hidden(),
        )
    }

    /// Whether the store has no events at all, in any calendar.
    pub fn is_empty(&self) -> bool {
        self.loaded() && self.data().items.is_empty()
    }

    // ---- looking ------------------------------------------------------------

    pub fn go(self, day: Date) {
        let mut anchor = self.anchor;
        anchor.set_if_modified(day);
    }

    pub fn today(self) {
        let clock = *self.clock.read();
        self.go(clock.today);
        let mut nonce = self.scroll_nonce;
        let next = *nonce.peek() + 1;
        nonce.set(next);
    }

    /// Switches view, and remembers it.
    pub fn show(self, kind: ViewKind) {
        let mut view = self.view;
        let mut prefs = self.prefs;
        if *view.peek() == kind {
            return;
        }
        view.set(kind);
        let mut next = prefs.peek().clone();
        next.view = kind;
        prefs.set(next.clone());
        spawn(async move {
            spawn_blocking(move || next.save()).await;
        });
        let mut nonce = self.scroll_nonce;
        let next = *nonce.peek() + 1;
        nonce.set(next);
    }

    /// The day `steps` periods from the one on show: a day, a week, a month
    /// or a year, as the view counts.
    pub fn stepped(&self, steps: i64) -> Date {
        let anchor = *self.anchor.peek();
        match *self.view.peek() {
            ViewKind::Day => anchor.add_days(steps),
            ViewKind::Week => anchor.add_days(steps * 7),
            ViewKind::Month => anchor.add_months(steps as i32),
            ViewKind::Year => anchor.add_months(steps as i32 * 12),
            ViewKind::Agenda => anchor.add_days(steps * 7),
        }
    }

    pub fn step(self, steps: i64) {
        self.go(self.stepped(steps));
    }

    pub fn set_prefs(self, change: impl FnOnce(&mut Prefs)) {
        let mut prefs = self.prefs;
        let mut next = prefs.peek().clone();
        change(&mut next);
        prefs.set(next.clone());
        spawn(async move {
            spawn_blocking(move || next.save()).await;
        });
    }

    pub fn toggle_calendar(self, id: &str) {
        let id = id.to_string();
        self.set_prefs(move |prefs| {
            if let Some(at) = prefs
                .hidden_calendars
                .iter()
                .position(|hidden| *hidden == id)
            {
                prefs.hidden_calendars.remove(at);
            } else {
                prefs.hidden_calendars.push(id);
            }
        });
    }

    // ---- changing -----------------------------------------------------------

    /// Applies changes at once and queues them to be written.
    pub fn apply(self, changes: Vec<Change>) {
        let (Some(current), Some(backend)) =
            (self.snapshot.read().clone(), self.backend.read().clone())
        else {
            return;
        };
        let mut snapshot = self.snapshot;
        snapshot.set(Some(Arc::new(current.apply(&changes))));
        backend.writer.write(changes);
    }

    /// The item an event reference names.
    pub fn item(&self, uid: &str) -> Option<Arc<Item>> {
        self.data().item(uid).cloned()
    }

    /// The first calendar that can be written to, or the preferred one.
    pub fn default_calendar(&self) -> Option<String> {
        let data = self.data();
        let preferred = self.prefs.read().default_calendar.clone();
        preferred
            .filter(|id| {
                data.calendar(id)
                    .is_some_and(|calendar| !calendar.read_only)
            })
            .or_else(|| {
                data.calendars
                    .iter()
                    .find(|calendar| !calendar.read_only)
                    .map(|calendar| calendar.id.clone())
            })
    }

    /// An edit of `item` at `key`, applied now.
    pub fn edit(
        self,
        item: &Item,
        key: Option<Key>,
        scope: Scope,
        event: bingux_calendar_core::Event,
        rule: Option<Option<String>>,
    ) {
        let changes = recur::edit(
            item,
            key,
            scope,
            event,
            rule,
            &bingux_calendar_core::ics::new_uid(),
        );
        self.apply(changes);
    }

    /// A removal of `item` at `key`, applied now. Returns what undoes it.
    pub fn remove(self, item: &Item, key: Option<Key>, scope: Scope) -> Vec<Change> {
        let before = self.data().item(&item.uid).map(|item| (**item).clone());
        let changes = recur::remove(item, key, scope);
        // What puts it back: the item as it was, and any item the change made.
        let mut undo: Vec<Change> = Vec::new();
        if let Some(before) = before {
            undo.push(Change::Save(before));
        }
        for change in &changes {
            if let Change::Save(saved) = change {
                if saved.uid != item.uid {
                    undo.push(Change::Delete(saved.uid.clone()));
                }
            }
        }
        self.apply(changes);
        undo
    }

    /// The day an occurrence starts on, in the view's dates.
    pub fn start_day(occurrence: &Occurrence) -> Date {
        convert::date_of(occurrence.start.date())
    }
}

/// How an event's time is said: "9:00 – 10:00", "All day", "Until 18:00".
pub fn time_text(occurrence: &Occurrence, day: Date, twelve_hour: bool) -> String {
    use bingux_ui::calendar::format_clock;
    if occurrence.all_day {
        return "All day".to_string();
    }
    let start_day = convert::date_of(occurrence.start.date());
    let end_day = convert::date_of(
        (occurrence.end - bingux_calendar_core::chrono::Duration::seconds(1)).date(),
    );
    let start = format_clock(convert::minutes_of(occurrence.start), twelve_hour);
    let end = format_clock(convert::minutes_of(occurrence.end), twelve_hour);
    if start_day == end_day {
        if occurrence.end == occurrence.start {
            start
        } else {
            format!("{start} – {end}")
        }
    } else if day == start_day {
        format!("From {start}")
    } else if day == end_day {
        format!("Until {end}")
    } else {
        "All day".to_string()
    }
}
