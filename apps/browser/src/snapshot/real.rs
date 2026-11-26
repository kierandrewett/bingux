//! `--snapshot DIR real ADDRESS,...`: the window over the real engine.

use std::cell::Cell;
use std::sync::Arc;

use bingux_ui::nav::{RouteTable, provide_router};
use bingux_ui::prelude::Location;
use bingux_ui::snapshot::Snapshot;
use freya::prelude::*;

use super::{CTX, sizes, still, wait};
use crate::browser::Options;
use crate::engine::Engine;
use crate::target::Target;
use crate::ui::App;

/// `--snapshot DIR real ADDRESS,...`: the window over the real engine, which
/// opens each address and is looked at once its page has a title and has had
/// a few seconds to paint. Needs a `vivarium` build and an SDK.
#[cfg(any(feature = "vivarium", feature = "dynamic-load"))]
pub(super) fn run_real(dir: &str, addresses: &[String]) {
    for (label, size) in sizes() {
        CTX.with(|slot| slot.set(None));
        let config = bingux_ui::web_view::engine::Config::from_environment(
            bingux_ui::web_view::engine::EngineFolders::Private,
        );
        // An engine that does not start is a window that says so: the shot shows it.
        let engine = Engine::new(bingux_ui::web_view::engine::start(config));
        let start = addresses.to_vec();
        let app = move || {
            provide_router(
                Arc::new(RouteTable::new(Target::SCHEME)),
                &Location::rooted(Target::SCHEME),
            );
            Element::from(App {
                engine: engine.clone(),
                options: Options { private: true },
                start: start.clone(),
                profile: crate::ui::Handoff::none(),
            })
        };
        let mut shot = Snapshot::new(app, size);
        still(&mut shot);
        // Gecko needs seconds, not frames: wait for the first page's title.
        let mut waited = 0;
        while waited < 90_000 {
            wait(&mut shot, 500);
            waited += 500;
            let ready = CTX.with(Cell::get).is_some_and(|ctx| {
                ctx.browser
                    .peek()
                    .active()
                    .is_some_and(|tab| !tab.title.is_empty() && !tab.load.loading)
            });
            if ready {
                break;
            }
        }
        // Then a few seconds for the first frames of it.
        wait(&mut shot, 4_000);
        let title = CTX
            .with(Cell::get)
            .map(|ctx| ctx.browser.peek().active().map(|tab| tab.title.clone()))
            .flatten();
        let path = format!("{dir}/real-{label}.png");
        shot.save(&path);
        println!("[snapshot] wrote {path} (title {title:?}, waited {waited} ms)");
        // The names assistive technology reads, over the real page: the page view
        // and the toolbar's buttons must be found by name.
        for name in ["Web page", "Back", "Forward", "Reload"] {
            let found = shot.click_name(name);
            println!(
                "[snapshot] accessible name {name:?} ({label}): {}",
                if found { "found" } else { "MISSING" }
            );
        }
    }
}
