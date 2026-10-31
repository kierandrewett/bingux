//! Sound > Effects: the stack of effects between the applications and the
//! speakers (output) or between the microphone and the applications (input).
//!
//! It opens as a large popup (a bottom sheet on a phone) from the Sound page:
//! a popup is a task with its own dismissal, it has the room an equalizer
//! needs where a subpage column does not, and the stack beside the effect's
//! page does not have to fight the Settings sidebar for width. On a phone the
//! stack is a list and an effect's page is pushed over it (`master_detail`).
//!
//! The page edits the active preset, a plain JSON file, and tells the engine to
//! read it (`bingux-effects reload`); the engine works out what changed and
//! touches only that (see `docs/audio-effects.md`). Every edit is shown at
//! once, then written and signalled 40 ms after the last one, so a slider
//! drag is heard while it moves and costs one file write per pause.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use freya::prelude::*;

use bingux_audio_effects::service::{self, StageState};
use bingux_audio_effects::store::{State as Saved, Store};
use bingux_audio_effects::{Catalog, Direction, Preset, graph};
use bingux_ui::prelude::{Window, popup, use_window};
use bingux_ui::window::{PopupFlags, WindowLike, WindowParent};

use crate::system::background_then;
use crate::ui::{Overlays, use_overlays};

mod body;
mod eq_page;
mod page;
mod picker;
mod presets;
mod stack;

/// How long after the last edit the preset is written and the engine told.
const COMMIT_DELAY: Duration = Duration::from_millis(40);

static COMMIT_VERSION: AtomicU64 = AtomicU64::new(0);

/// What the page shows of the store: the settings and the active preset.
#[derive(Clone, PartialEq)]
pub(super) struct Loaded {
    pub direction: Direction,
    pub saved: Saved,
    pub presets: Vec<String>,
    pub preset: Preset,
    /// False for the placeholder shown until the store has been read.
    pub ready: bool,
}

thread_local! {
    /// The last store contents read for each direction, so the popup opens with
    /// them at once. UI thread only.
    static LAST: std::cell::RefCell<HashMap<Direction, Loaded>> = std::cell::RefCell::new(HashMap::new());
    /// The last catalog, for the same reason.
    static CATALOG: std::cell::RefCell<Option<Catalog>> = const { std::cell::RefCell::new(None) };
}

/// What the popup shows before the store has been read: the last contents when
/// it has been open before, else a flat, empty stack. A lookup, never a file read.
fn cached(direction: Direction) -> Loaded {
    LAST.with(|last| last.borrow().get(&direction).cloned())
        .unwrap_or_else(|| Loaded {
            direction,
            saved: Saved::default(),
            presets: Vec::new(),
            preset: Preset::new("Flat", direction),
            ready: false,
        })
}

/// Read the store for `direction`: a few small files, and the factory presets
/// (and the migration of old ones) are done when they are missing. Runs on a worker.
fn load(direction: Direction) -> Loaded {
    let store = Store::default();
    let _ = store.ensure_factory();
    let saved = store.state();
    let preset = store
        .load(direction, &saved.of(direction).preset)
        .unwrap_or_else(|_| Preset::new("Flat", direction));
    Loaded {
        direction,
        presets: store.list(direction),
        saved,
        preset,
        ready: true,
    }
}

/// What the running system has: whether the engine runs, the nodes the meters
/// need and what each effect's stage reports. Read in the background.
#[derive(Clone, Default, PartialEq)]
pub(super) struct Runtime {
    pub running: bool,
    pub virtual_node: Option<u32>,
    pub device_node: Option<u32>,
    /// The real device, as the engine uses it.
    pub device: Option<String>,
    /// Its name for a person.
    pub device_label: Option<String>,
    /// Why an effect is not in the sound, by its id.
    pub failures: HashMap<String, String>,
}

fn probe(direction: Direction, saved_device: Option<String>) -> Runtime {
    let nodes = service::list_nodes().unwrap_or_default();
    let device = saved_device.or_else(|| service::default_device(direction));
    let found = |name: &str| nodes.iter().find(|node| node.name == name);
    let failures = service::read_status(direction)
        .map(|status| {
            status
                .stages
                .into_iter()
                .filter(|stage| stage.state == StageState::Failed)
                .map(|stage| {
                    (
                        stage.id,
                        stage
                            .reason
                            .unwrap_or_else(|| "It is not running.".to_string()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Runtime {
        running: service::is_running(direction),
        virtual_node: found(graph::node_name(direction)).map(|node| node.id),
        device_node: device.as_deref().and_then(found).map(|node| node.id),
        device_label: device
            .as_deref()
            .and_then(found)
            .map(|node| node.description.clone()),
        device,
        failures,
    }
}

/// Everything the parts of the popup share. Copy it into closures.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Fx {
    pub data: State<Loaded>,
    pub runtime: State<Runtime>,
    pub catalog: State<Option<Catalog>>,
    /// The effect whose page is shown, by its id.
    pub selected: State<Option<String>>,
    /// On a phone: whether the effect's page is pushed over the stack.
    pub page_open: State<bool>,
    pub overlays: Overlays,
    pub window: Window,
}

impl Fx {
    pub fn active_direction(self) -> Direction {
        self.data.read().direction
    }

    /// The effect that is selected: the chosen one, else the first.
    pub fn selected_id(self, data: &Loaded) -> Option<String> {
        let chosen = self.selected.read().clone();
        chosen
            .filter(|id| data.preset.slot(id).is_some())
            .or_else(|| data.preset.plugins.first().map(|slot| slot.id.clone()))
    }

    pub fn select(mut self, id: &str) {
        self.selected.set(Some(id.to_string()));
    }

    /// Read the running system again.
    pub fn refresh(self) {
        let direction = self.active_direction();
        let device = self.data.peek().saved.of(direction).device.clone();
        let mut runtime = self.runtime;
        background_then(
            move || probe(direction, device),
            move |found| {
                if found.running || found != *runtime.peek() {
                    runtime.set(found);
                }
            },
        );
    }

    /// Show the settings and preset the store holds now, read in the background.
    pub fn reload(self) {
        let direction = self.active_direction();
        let mut data = self.data;
        background_then(
            move || load(direction),
            move |loaded| {
                LAST.with(|last| last.borrow_mut().insert(direction, loaded.clone()));
                // A direction chosen since this read began has its own read.
                if data.peek().direction == direction {
                    data.set(loaded);
                }
            },
        );
    }

    /// Change the active preset. The change is shown at once, then written and
    /// sent to the engine a moment after the last one.
    pub fn edit(mut self, change: impl FnOnce(&mut Preset)) {
        let mut preset = self.data.peek().preset.clone();
        change(&mut preset);
        if preset == self.data.peek().preset {
            return;
        }
        self.data.write().preset = preset;
        self.commit_soon();
    }

    fn commit_soon(self) {
        let version = COMMIT_VERSION.fetch_add(1, Ordering::SeqCst) + 1;
        let direction = self.active_direction();
        spawn(async move {
            timer(COMMIT_DELAY).await;
            if COMMIT_VERSION.load(Ordering::SeqCst) == version {
                self.commit_now(direction);
            }
        });
    }

    /// Write the preset and tell the engine, if the effects are on.
    pub fn commit_now(self, direction: Direction) {
        let overlays = self.overlays;
        let preset = self.data.peek().preset.clone();
        let enabled = self.data.peek().saved.of(direction).enabled;
        let shown = preset.clone();
        background_then(
            move || {
                Store::default().save(&preset).map_err(|e| e.0)?;
                if enabled {
                    service::reload(direction)
                } else {
                    Ok(())
                }
            },
            move |result: Result<(), String>| {
                match result {
                    Ok(()) => LAST.with(|last| {
                        if let Some(entry) = last.borrow_mut().get_mut(&shown.direction) {
                            entry.preset = shown;
                        }
                    }),
                    Err(message) => overlays.toast(message),
                }
                // A new effect needs a moment to start before its state is known.
                spawn(async move {
                    timer(Duration::from_millis(900)).await;
                    self.refresh();
                });
            },
        );
    }

    /// Turn the effects of the current direction on or off.
    pub fn set_enabled(mut self, enabled: bool) {
        let direction = self.active_direction();
        self.data.write().saved.of_mut(direction).enabled = enabled;
        let overlays = self.overlays;
        background_then(
            move || {
                Store::default()
                    .update_state(|state| state.of_mut(direction).enabled = enabled)
                    .map_err(|e| e.0)?;
                service::apply(&Store::default(), direction)
            },
            move |result: Result<(), String>| {
                if let Err(message) = result {
                    overlays.toast(message);
                }
                spawn(async move {
                    timer(Duration::from_millis(1200)).await;
                    self.refresh();
                });
            },
        );
    }
}

/// The popup's state, loaded once.
fn use_fx(direction: Direction) -> Fx {
    let overlays = use_overlays();
    let window = use_window();
    let data = use_state(move || cached(direction));
    let runtime = use_state(Runtime::default);
    let catalog = use_state(|| CATALOG.with(|cached| cached.borrow().clone()));
    let selected = use_state(|| None::<String>);
    let page_open = use_state(|| false);
    let fx = Fx {
        data,
        runtime,
        catalog,
        selected,
        page_open,
        overlays,
        window,
    };
    use_hook(move || {
        let mut catalog = catalog;
        // Scanning the plugins can take a while, so it is done in the background
        // and the result is cached by the date of the plugin bundles.
        background_then(Catalog::load, move |found| {
            CATALOG.with(|cached| *cached.borrow_mut() = Some(found.clone()));
            catalog.set(Some(found));
        });
        fx.reload();
        fx.refresh();
    });
    fx
}

/// Open the effects popup. A bottom sheet on a phone.
pub fn open(window: Window) {
    window.open(
        popup("Effects", body::EffectsBody)
            .size(1040.0, 760.0)
            .flags(PopupFlags::CANCEL | PopupFlags::GRABBER | PopupFlags::FILL),
    );
}

/// What [`summary`] last found: `None` until it has run.
static SUMMARY: Mutex<Option<Option<String>>> = Mutex::new(None);

/// Used by the page that lists the sound devices: whether effects are on
/// for `direction`, and which preset. Reads the store, so it runs on a worker.
pub fn summary(direction: Direction) -> Option<String> {
    let state = Store::default().state();
    let direction_state = state.of(direction);
    let found = direction_state
        .enabled
        .then(|| direction_state.preset.clone());
    if direction == Direction::Output {
        *SUMMARY.lock().unwrap_or_else(|e| e.into_inner()) = Some(found.clone());
    }
    found
}

/// What [`summary`] found the last time it ran for the output, or `None`
/// before it has. A lookup, never a file read.
pub fn summary_cached() -> Option<String> {
    SUMMARY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .flatten()
}

/// The preset that applies when `direction`'s effects are on, from what the
/// popup last read. A lookup, never a file read.
pub fn active_preset_cached(direction: Direction) -> String {
    cached(direction).saved.of(direction).preset.clone()
}
