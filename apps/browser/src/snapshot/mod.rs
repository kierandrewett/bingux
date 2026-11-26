//! `bingux-browser --snapshot DIR [SCENARIO,...]`: the window rendered
//! headlessly to PNG, over the in-memory test double of `bingux_ui::web_view`
//! (never the engine), at the desktop size and at 390 x 800.
//!
//! Each scenario puts the window in one state: a page, a tab that crashed, a
//! permission prompt, the find bar. The pictures are what the UI looks like
//! in each state; the pages in them are the double's wireframes, not a web
//! engine's. This module exists only in a `snapshot` build.
//!
//! * `BINGUX_BROWSER_SNAPSHOT_SIZE=WxH` renders one size instead of both.
//! * `--list` prints the scenarios.

#[cfg(any(feature = "vivarium", feature = "dynamic-load"))]
mod real;

use std::cell::Cell;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use bingux_ui::nav::{RouteTable, provide_router};
use bingux_ui::prelude::Location;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::web_view::test_double::TestDoubleBackend;
use bingux_ui::web_view::{BrowserEvent, TabEvent, TabId};
use freya::prelude::*;

use crate::browser::Options;
use crate::commands::Command;
use crate::engine::Engine;
use crate::target::Target;
use crate::ui::{App, Ctx};

thread_local! {
    /// The window's handles, published by the window when it mounts, so a
    /// scenario can do what a person would through the model.
    pub(super) static CTX: Cell<Option<Ctx>> = const { Cell::new(None) };
}

/// Called by the window root on every render.
pub fn publish(ctx: Ctx) {
    CTX.with(|slot| slot.set(Some(ctx)));
}

pub(crate) fn ctx() -> Ctx {
    CTX.with(Cell::get).expect("the window has mounted")
}

/// What a scenario works with.
pub(crate) struct Rig {
    pub(crate) double: Arc<TestDoubleBackend>,
}

impl Rig {
    /// The engine session of the `n`th tab (from 0).
    pub(crate) fn session(n: usize) -> TabId {
        ctx()
            .browser
            .peek()
            .tabs
            .all()
            .get(n)
            .and_then(|tab| tab.session)
            .expect("a tab with a session")
    }

    /// The key of the `n`th tab (from 0).
    pub(crate) fn key_of(n: usize) -> u64 {
        ctx().browser.peek().tabs.all().get(n).expect("a tab").key
    }

    /// The engine says `event` of the `n`th tab.
    pub(crate) fn tell(&self, n: usize, event: TabEvent) {
        self.double.emit(BrowserEvent::tab(Self::session(n), event));
    }
}

pub(crate) type Run = fn(&mut Snapshot, &Rig);

pub(crate) struct Scenario {
    pub(crate) name: &'static str,
    /// The addresses the window opens on.
    pub(crate) start: &'static [&'static str],
    /// Loads wait until the scenario finishes them.
    pub(crate) manual: bool,
    pub(crate) private: bool,
    /// The double to run over, when it is not the one that can do everything.
    pub(crate) engine: Option<fn() -> Arc<TestDoubleBackend>>,
    /// The window is made this many times taller: a long page is looked at whole,
    /// not scrolled (a scrolled page puts labels under the toolbar for the layout lint).
    pub(crate) tall: f32,
    pub(crate) run: Run,
}

pub(crate) const fn scenario(
    name: &'static str,
    start: &'static [&'static str],
    run: Run,
) -> Scenario {
    Scenario {
        name,
        start,
        manual: false,
        private: false,
        engine: None,
        tall: 1.0,
        run,
    }
}

pub(crate) fn nothing(_: &mut Snapshot, _: &Rig) {}

/// A window that opens on a blank tab, with the address field focused: its
/// caret stays solid while the picture is taken.
pub(crate) fn blank(shot: &mut Snapshot, _: &Rig) {
    wait_typing(shot, 600);
}

pub(crate) fn wait(shot: &mut Snapshot, ms: u64) {
    shot.settle(Duration::from_millis(ms));
}

/// A window whose animations jump to their end, so what a picture shows does
/// not depend on how fast the machine ran the frames before it: a spinner is
/// at its first turn, a fade is over. Freya's animation clock is the hook.
pub(crate) fn still(shot: &mut Snapshot) {
    shot.runner().animation_clock().disable();
}

/// The caret blinks on a real-time timer that starts half a second after the
/// last key, so a picture taken in that half second always shows it solid:
/// keep pressing a key that does not move it while waiting.
pub(crate) fn wait_typing(shot: &mut Snapshot, ms: u64) {
    for _ in 0..ms / 100 {
        shot.key(Key::Named(NamedKey::End));
        wait(shot, 100);
    }
}

pub(crate) fn run_command(shot: &mut Snapshot, command: Command) {
    let at = ctx();
    // Inside the window's runtime, as a handler would be: a command may open a menu.
    shot.runner().run_in(|| at.run(command));
    wait(shot, 500);
}

/// A window over the test double that opens on `start`, settled, for a test that presses its
/// controls.
#[cfg(test)]
pub(crate) fn window(start: &'static [&'static str]) -> (Snapshot, Rig) {
    window_of(start, (1100.0, 700.0))
}

/// [`window`] at `size`: a phone is 390 by 800.
#[cfg(test)]
pub(crate) fn window_of(start: &'static [&'static str], size: (f32, f32)) -> (Snapshot, Rig) {
    crate::locale::register();
    CTX.with(|slot| slot.set(None));
    let rig = Rig {
        double: TestDoubleBackend::new(),
    };
    let scenario = scenario("press", start, nothing);
    let mut shot = Snapshot::new(app(&rig, &scenario), size);
    wait(&mut shot, 800);
    (shot, rig)
}

/// Every scenario, feature by feature.
fn scenarios() -> Vec<Scenario> {
    [
        crate::new_tab::scenarios(),
        crate::app::scenarios(),
        crate::tabs::scenarios(),
        crate::navigation::scenarios(),
        crate::prompts::scenarios(),
        crate::error_pages::scenarios(),
        crate::find::scenarios(),
        crate::downloads::scenarios(),
        crate::site_info::scenarios(),
        crate::menus::scenarios(),
        crate::settings::scenarios(),
        crate::address::scenarios(),
        crate::zoom::scenarios(),
        crate::private::scenarios(),
        crate::engine::scenarios(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The window root a snapshot mounts.
fn app(rig: &Rig, scenario: &Scenario) -> impl Fn() -> Element + use<> {
    let engine = Engine::new(rig.double.clone());
    let options = Options {
        private: scenario.private,
    };
    let start: Vec<String> = scenario
        .start
        .iter()
        .map(|url| (*url).to_string())
        .collect();
    move || {
        provide_router(
            Arc::new(RouteTable::new(Target::SCHEME)),
            &Location::rooted(Target::SCHEME),
        );
        Element::from(App {
            engine: engine.clone(),
            options,
            start: start.clone(),
            profile: crate::ui::Handoff::none(),
        })
    }
}

pub(super) fn sizes() -> Vec<(&'static str, (f32, f32))> {
    if let Ok(text) = std::env::var("BINGUX_BROWSER_SNAPSHOT_SIZE") {
        if let Some((width, height)) = text.split_once('x') {
            if let (Ok(width), Ok(height)) = (width.trim().parse(), height.trim().parse()) {
                return vec![("custom", (width, height))];
            }
        }
    }
    vec![("desktop", (1100.0, 700.0)), ("phone", (390.0, 800.0))]
}

/// Render the scenarios named (all of them when none is) into `arguments[0]`.
pub fn run(arguments: &[OsString]) {
    let scenarios = scenarios();
    if arguments.first().is_some_and(|first| first == "--list") {
        for scenario in &scenarios {
            println!("{}", scenario.name);
        }
        return;
    }
    let dir = arguments.first().map_or_else(
        || "/tmp/bingux-browser-snapshots".into(),
        |dir| dir.to_string_lossy().into_owned(),
    );
    let only: Vec<String> = arguments
        .get(1)
        .map(|list| {
            list.to_string_lossy()
                .split(',')
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    std::fs::create_dir_all(&dir).expect("create the snapshot folder");
    if only.first().is_some_and(|name| name == "real") {
        #[cfg(any(feature = "vivarium", feature = "dynamic-load"))]
        {
            let addresses: Vec<String> = std::env::var("BINGUX_BROWSER_REAL_URLS")
                .unwrap_or_default()
                .split(',')
                .filter(|url| !url.is_empty())
                .map(str::to_string)
                .collect();
            real::run_real(&dir, &addresses);
        }
        #[cfg(not(any(feature = "vivarium", feature = "dynamic-load")))]
        eprintln!("the `real` scenario needs a build with the `vivarium` feature");
        return;
    }
    for scenario in &scenarios {
        if !only.is_empty() && !only.iter().any(|name| name == scenario.name) {
            continue;
        }
        for (label, (width, height)) in sizes() {
            let size = (width, height * scenario.tall);
            CTX.with(|slot| slot.set(None));
            let double = match scenario.engine {
                Some(make) => make(),
                None if scenario.manual => TestDoubleBackend::manual(),
                None => TestDoubleBackend::new(),
            };
            let rig = Rig { double };
            let mut shot = Snapshot::new(app(&rig, scenario), size);
            still(&mut shot);
            (scenario.run)(&mut shot, &rig);
            wait(&mut shot, 300);
            let path = format!("{dir}/{}-{label}.png", scenario.name);
            shot.save(&path);
            println!("[snapshot] wrote {path}");
        }
    }
}
