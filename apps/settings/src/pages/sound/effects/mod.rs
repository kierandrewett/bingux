//! Sound > Effects: the chain of plugins between the applications and the
//! speakers (output) or between the microphone and the applications (input).
//!
//! The page is a front end for the `bingux-audio-effects` engine. It edits
//! the active preset, a plain JSON file, and the engine plays it:
//!
//! - a control moved on a slider is set in the running chain at once (`pw-cli`),
//!   so the change is heard while the slider moves;
//! - adding, removing, reordering or switching off a plugin restarts the
//!   engine, a gap of a fraction of a second, a moment after the last change;
//! - every change is also written to the preset file, a moment after the last.
//!
//! Nothing here polls. The meters run only while their rows are on screen in
//! a focused window (see `meter.rs`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use freya::prelude::*;

use bingux_audio_effects::graph::{self, Live};
use bingux_audio_effects::store::{State as Saved, Store};
use bingux_audio_effects::{Catalog, Direction, Preset, service};
use bingux_ui::prelude::{PageHead, Window, segmented, use_window};
use bingux_ui::style::*;

use crate::pages::loading::veiled;
use crate::system::background_then;
use crate::ui::{Overlays, page_column, use_overlays, use_tokens};

mod chain;
mod equalizer;
mod pane;
mod picker;
mod presets;
mod spectrum;

/// How long after the last edit the preset file is written.
const SAVE_DELAY: Duration = Duration::from_millis(250);
/// How long after the last structural edit the engine restarts.
const RESTART_DELAY: Duration = Duration::from_millis(400);
/// How long a live control change waits for a later one to replace it.
const LIVE_DELAY: Duration = Duration::from_millis(30);

static SAVE_VERSION: AtomicU64 = AtomicU64::new(0);
static RESTART_VERSION: AtomicU64 = AtomicU64::new(0);
static LIVE_BUSY: AtomicBool = AtomicBool::new(false);
/// Control changes waiting to be sent: the newest value of each parameter.
static LIVE_PENDING: LazyLock<Mutex<Vec<(String, f64)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

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
    /// The last store contents read for each direction, so a page opens with
    /// them at once. UI thread only.
    static LAST: std::cell::RefCell<std::collections::HashMap<Direction, Loaded>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// What the page shows before the store has been read: the last contents when
/// the page has been open before, else a flat, empty chain. A lookup, never a
/// file read.
fn cached(direction: Direction) -> Loaded {
    LAST.with(|last| last.borrow().get(&direction).cloned()).unwrap_or_else(|| Loaded {
        direction,
        saved: Saved::default(),
        presets: Vec::new(),
        preset: Preset::new("Flat", direction),
        ready: false,
    })
}

/// Read the store for `direction`: a few small files, and the factory presets
/// are written when they are missing. Runs on a worker.
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

/// What the running system has: whether the engine runs, and the nodes the
/// meters and live changes need. Read in the background.
#[derive(Clone, Default, PartialEq)]
pub(super) struct Runtime {
    pub running: bool,
    pub virtual_node: Option<u32>,
    pub device_node: Option<u32>,
    /// The real device, as the engine uses it.
    pub device: Option<String>,
    /// Its name for a person.
    pub device_label: Option<String>,
}

fn probe(direction: Direction, saved_device: Option<String>) -> Runtime {
    let nodes = service::list_nodes().unwrap_or_default();
    let device = saved_device.or_else(|| service::default_device(direction));
    let found = |name: &str| nodes.iter().find(|node| node.name == name);
    Runtime {
        running: service::is_running(direction),
        virtual_node: found(graph::node_name(direction)).map(|node| node.id),
        device_node: device.as_deref().and_then(found).map(|node| node.id),
        device_label: device
            .as_deref()
            .and_then(found)
            .map(|node| node.description.clone()),
        device,
    }
}

/// What an edit does to the sound.
pub(super) enum Effect {
    /// Only the saved preset changes (a name, a path).
    Nothing,
    /// The change is sent to the running chain.
    Live(Vec<Live>),
    /// The chain changed shape: the engine restarts.
    Restart,
}

/// Everything the parts of the page share. Copy it into closures.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Fx {
    pub data: State<Loaded>,
    pub runtime: State<Runtime>,
    pub catalog: State<Option<Catalog>>,
    /// The effect whose controls are shown, by plugin id.
    pub selected: State<Option<String>>,
    /// Whether the list of effects is shown beside the controls.
    pub list_shown: State<bool>,
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
        background_then(move || probe(direction, device), move |found| {
            if found.running || found != *runtime.peek() {
                runtime.set(found);
            }
        });
    }

    /// Show the settings and preset the store holds now, read in the background.
    pub fn reload(self) {
        let direction = self.active_direction();
        let mut data = self.data;
        background_then(move || load(direction), move |loaded| {
            LAST.with(|last| last.borrow_mut().insert(direction, loaded.clone()));
            // A direction chosen since this read began has its own read.
            if data.peek().direction == direction {
                data.set(loaded);
            }
        });
    }

    /// Change the active preset. The change is shown at once, saved a moment
    /// later and, as `effect` says, played.
    pub fn edit(mut self, change: impl FnOnce(&mut Preset), effect: Effect) {
        let mut preset = self.data.peek().preset.clone();
        change(&mut preset);
        self.data.write().preset = preset.clone();
        self.save_soon(preset.clone());
        match effect {
            Effect::Nothing => {}
            Effect::Restart => self.restart_soon(),
            Effect::Live(changes) => {
                let catalog = self.catalog.peek().clone().unwrap_or_else(Catalog::builtin);
                let params: Vec<(String, f64)> = changes
                    .iter()
                    .flat_map(|change| graph::live_params(&preset, &catalog, change))
                    .collect();
                self.send_live(params);
            }
        }
    }

    fn save_soon(self, preset: Preset) {
        let version = SAVE_VERSION.fetch_add(1, Ordering::SeqCst) + 1;
        let overlays = self.overlays;
        spawn(async move {
            timer(SAVE_DELAY).await;
            if SAVE_VERSION.load(Ordering::SeqCst) != version {
                return;
            }
            let shown = preset.clone();
            background_then(
                move || Store::default().save(&preset),
                move |result| match result {
                    Ok(_) => LAST.with(|last| {
                        if let Some(entry) = last.borrow_mut().get_mut(&shown.direction) {
                            entry.preset = shown;
                        }
                    }),
                    Err(e) => overlays.toast(format!("Could not save the preset. {}", e.0)),
                },
            );
        });
    }

    /// Restart the engine after the changes stop, when it is running.
    pub fn restart_soon(self) {
        let version = RESTART_VERSION.fetch_add(1, Ordering::SeqCst) + 1;
        let direction = self.active_direction();
        spawn(async move {
            timer(RESTART_DELAY).await;
            if RESTART_VERSION.load(Ordering::SeqCst) != version {
                return;
            }
            self.restart_now(direction);
        });
    }

    /// Write the preset and restart the engine, if the effects are on.
    pub fn restart_now(self, direction: Direction) {
        let overlays = self.overlays;
        let preset = self.data.peek().preset.clone();
        let enabled = self.data.peek().saved.of(direction).enabled;
        background_then(
            move || {
                Store::default().save(&preset).map_err(|e| e.0)?;
                if enabled { service::start(direction) } else { Ok(()) }
            },
            move |result: Result<(), String>| {
                if let Err(message) = result {
                    overlays.toast(message);
                }
                // The engine needs a moment to make its device.
                spawn(async move {
                    timer(Duration::from_millis(1200)).await;
                    self.refresh();
                });
            },
        );
    }

    /// Set parameters in the running chain, newest value of each only.
    fn send_live(self, params: Vec<(String, f64)>) {
        let Some(node) = self.runtime.peek().virtual_node else {
            return;
        };
        if params.is_empty() {
            return;
        }
        {
            let mut pending = LIVE_PENDING.lock().unwrap_or_else(|e| e.into_inner());
            for (name, value) in params {
                match pending.iter_mut().find(|(waiting, _)| *waiting == name) {
                    Some(entry) => entry.1 = value,
                    None => pending.push((name, value)),
                }
            }
        }
        if LIVE_BUSY.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(LIVE_DELAY);
                let batch = std::mem::take(&mut *LIVE_PENDING.lock().unwrap_or_else(|e| e.into_inner()));
                if batch.is_empty() {
                    LIVE_BUSY.store(false, Ordering::SeqCst);
                    // A change that arrived after the check starts another pass.
                    if LIVE_PENDING.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
                        || LIVE_BUSY.swap(true, Ordering::SeqCst)
                    {
                        return;
                    }
                    continue;
                }
                let _ = service::set_params(node, &batch);
            }
        });
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

/// The page's state, loaded once.
fn use_fx(direction: Direction, selected: Option<String>) -> Fx {
    let overlays = use_overlays();
    let window = use_window();
    let data = use_state(move || cached(direction));
    let runtime = use_state(Runtime::default);
    let catalog = use_state(|| None::<Catalog>);
    let selected = use_state(move || selected);
    let list_shown = use_state(|| true);
    let fx = Fx {
        data,
        runtime,
        catalog,
        selected,
        list_shown,
        overlays,
        window,
    };
    use_hook(move || {
        let mut catalog = catalog;
        // Scanning libraries can take a while, so it is done in the background
        // and the result is cached by file date.
        background_then(Catalog::load, move |found| catalog.set(Some(found)));
        fx.reload();
        fx.refresh();
    });
    fx
}

/// The token a route names an effect by: `output.equalizer-1`.
pub fn token(direction: Direction, id: &str) -> String {
    format!("{}.{id}", direction.name())
}

fn parse_token(token: &str) -> Option<(Direction, String)> {
    let (direction, id) = token.split_once('.')?;
    Some((Direction::parse(direction)?, id.to_string()))
}

/// The effects page: the master switch, the Output and Input tabs, the
/// spectrum, and the chain next to the selected effect. On a phone the chain
/// is the page and a selected effect is its own page (`sound/effects/:effect`).
#[derive(PartialEq)]
pub struct EffectsPage;

impl Component for EffectsPage {
    fn render(&self) -> impl IntoElement {
        let fx = use_fx(Direction::Output, None);
        let tokens = use_tokens();
        let data = fx.data.read().clone();
        let runtime = fx.runtime.read().clone();
        let direction = data.direction;
        let enabled = data.saved.of(direction).enabled;
        let compact = is_compact();

        let tabs = segmented(
            vec![
                ("output".to_string(), "Output".to_string()),
                ("input".to_string(), "Input".to_string()),
            ],
            direction.name(),
            move |name: String| {
                if let Some(next) = Direction::parse(&name) {
                    let mut data = fx.data;
                    data.set(cached(next));
                    let mut selected = fx.selected;
                    selected.set(None);
                    fx.reload();
                    fx.refresh();
                }
            },
        )
        .tabs();

        let mut page = page_column()
            .child(PageHead::new().switch("Effects", enabled, move |on: bool| fx.set_enabled(on)))
            .child(tabs)
            .child(spectrum::SpectrumStrip { direction })
            .child(presets::group(fx, &data, &runtime));

        let list = chain::list(fx, &data, &runtime);
        if compact {
            return veiled(page.child(list), !data.ready);
        }
        let shown = *fx.list_shown.read();
        let selected = fx.selected_id(&data);
        let pane = pane::PaneView {
            fx,
            data: data.clone(),
            runtime: runtime.clone(),
            selected: selected.clone(),
        };
        let mut row = rect().tw(&tokens, &[flex_row(), grow_children(), w_full(), gap(S4), items(Align::Start)]);
        if shown {
            row = row.child(rect().tw(&tokens, &[w(Dim::MenuWide)]).child(list));
        }
        row = row.child(rect().width(Size::flex(1.0)).child(pane));
        page = page.child(row);
        veiled(page, !data.ready)
    }
}

/// One effect on its own page, for a phone: `sound/effects/:effect`.
#[derive(PartialEq)]
pub struct EffectPage {
    pub token: String,
}

impl Component for EffectPage {
    fn render(&self) -> impl IntoElement {
        let (direction, id) = parse_token(&self.token).unwrap_or((Direction::Output, String::new()));
        let fx = use_fx(direction, Some(id.clone()));
        let data = fx.data.read().clone();
        let runtime = fx.runtime.read().clone();
        veiled(
            page_column()
                .child(PageHead::new())
                .child(pane::PaneView {
                    fx,
                    data: data.clone(),
                    runtime: runtime.clone(),
                    selected: Some(id.clone()),
                }),
            !data.ready,
        )
    }
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
    SUMMARY.lock().unwrap_or_else(|e| e.into_inner()).clone().flatten()
}

/// The preset that applies when `direction`'s effects are on, from what the
/// effects page last read. A lookup, never a file read.
pub fn active_preset_cached(direction: Direction) -> String {
    cached(direction).saved.of(direction).preset.clone()
}

