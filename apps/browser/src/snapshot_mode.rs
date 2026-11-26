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

use std::cell::Cell;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use bingux_ui::nav::{RouteTable, provide_router};
use bingux_ui::prelude::Location;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::web_view::test_double::TestDoubleBackend;
use bingux_ui::web_view::{BrowserEvent, Capabilities, FailureCause, TabEvent, TabId};
use freya::prelude::*;

use crate::browser::Options;
use crate::commands::Command;
use crate::engine::Engine;
use crate::tabs::Tab;
use crate::target::Target;
use crate::ui::{App, Ctx};

thread_local! {
    /// The window's handles, published by the window when it mounts, so a
    /// scenario can do what a person would through the model.
    static CTX: Cell<Option<Ctx>> = const { Cell::new(None) };
}

/// Called by the window root on every render.
pub fn publish(ctx: Ctx) {
    CTX.with(|slot| slot.set(Some(ctx)));
}

fn ctx() -> Ctx {
    CTX.with(Cell::get).expect("the window has mounted")
}

/// What a scenario works with.
struct Rig {
    double: Arc<TestDoubleBackend>,
}

type Run = fn(&mut Snapshot, &Rig);

struct Scenario {
    name: &'static str,
    /// The addresses the window opens on.
    start: &'static [&'static str],
    /// Loads wait until the scenario finishes them.
    manual: bool,
    private: bool,
    /// The double to run over, when it is not the one that can do everything.
    engine: Option<fn() -> Arc<TestDoubleBackend>>,
    /// The window is made this many times taller: a long page is looked at whole,
    /// not scrolled (a scrolled page puts labels under the toolbar for the layout lint).
    tall: f32,
    run: Run,
}

const TABS: &[&str] = &[
    "example.com",
    "news.bingux.dev",
    "video.example.net",
    "crash.example.net",
    "dialog.example.net",
    "docs.example.org/a-page-with-a-very-long-title-that-will-not-fit",
    "plain.example.net",
    "files.example.net",
];

const fn scenario(name: &'static str, start: &'static [&'static str], run: Run) -> Scenario {
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

fn nothing(_: &mut Snapshot, _: &Rig) {}

/// A window that opens on a blank tab, with the address field focused: its
/// caret stays solid while the picture is taken.
fn blank(shot: &mut Snapshot, _: &Rig) {
    wait_typing(shot, 600);
}

fn wait(shot: &mut Snapshot, ms: u64) {
    shot.settle(Duration::from_millis(ms));
}

/// A window whose animations jump to their end, so what a picture shows does
/// not depend on how fast the machine ran the frames before it: a spinner is
/// at its first turn, a fade is over. Freya's animation clock is the hook.
fn still(shot: &mut Snapshot) {
    shot.runner().animation_clock().disable();
}

/// The caret blinks on a real-time timer that starts half a second after the
/// last key, so a picture taken in that half second always shows it solid:
/// keep pressing a key that does not move it while waiting.
fn wait_typing(shot: &mut Snapshot, ms: u64) {
    for _ in 0..ms / 100 {
        shot.key(Key::Named(NamedKey::End));
        wait(shot, 100);
    }
}

/// The engine session of the `n`th tab (from 0).
fn session(n: usize) -> TabId {
    ctx()
        .browser
        .peek()
        .tabs
        .all()
        .get(n)
        .and_then(|tab| tab.session)
        .expect("a tab with a session")
}

fn key_of(n: usize) -> u64 {
    ctx().browser.peek().tabs.all().get(n).expect("a tab").key
}

fn tell(rig: &Rig, n: usize, event: TabEvent) {
    rig.double.emit(BrowserEvent::tab(session(n), event));
}

fn run_command(shot: &mut Snapshot, command: Command) {
    let at = ctx();
    // Inside the window's runtime, as a handler would be: a command may open a menu.
    shot.runner().run_in(|| at.run(command));
    wait(shot, 500);
}

/// The tab strip with every state a tab can be in.
fn tabs(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 400);
    let ctx = ctx();
    let (first, second, third) = (key_of(0), key_of(1), key_of(2));
    ctx.with(|browser| {
        browser.set_pinned(first, true);
        browser.set_pinned(second, true);
        browser.select(third);
    });
    tell(
        rig,
        2,
        TabEvent::Audio(bingux_ui::web_view::Audio {
            playing: true,
            muted: false,
        }),
    );
    tell(
        rig,
        7,
        TabEvent::Audio(bingux_ui::web_view::Audio {
            playing: true,
            muted: true,
        }),
    );
    wait(shot, 500);
}

fn tab_menu(shot: &mut Snapshot, rig: &Rig) {
    tabs(shot, rig);
    shot.right_click(300.0, 38.0);
    wait(shot, 500);
}

fn loading(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 300);
    tell(
        rig,
        0,
        TabEvent::Load {
            started: false,
            loading: true,
            progress: 0.45,
            error: None,
        },
    );
    wait(shot, 400);
}

fn permission(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 600);
}

fn cert_details(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    shot.click_text("Details\u{2026}");
    wait(shot, 700);
}

fn find(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 400);
    run_command(shot, Command::Find);
    shot.run_script("type exam", Target::SCHEME)
        .expect("typing");
    wait_typing(shot, 600);
}

fn find_missing(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 400);
    run_command(shot, Command::Find);
    shot.run_script("type ab", Target::SCHEME).expect("typing");
    wait_typing(shot, 600);
}

fn downloads(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    rig.double.advance_downloads(1_000_000);
    wait(shot, 300);
    run_command(shot, Command::Downloads);
}

fn info(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::PageInfo);
}

fn menu(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    shot.click_name("Main menu");
    wait(shot, 500);
}

fn settings(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::Settings);
    wait(shot, 500);
}

fn suggestions(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::FocusAddress);
    shot.run_script("type ex", Target::SCHEME).expect("typing");
    wait(shot, 700);
}

/// A right click on the link of the double's page.
fn context(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    let (x, y) = *ctx().chrome.page_origin.peek();
    shot.right_click(x + 100.0, y + 290.0);
    wait(shot, 600);
}

fn hover(shot: &mut Snapshot, _: &Rig) {
    let active = |predicate: fn(&Tab) -> bool| ctx().browser.peek().active().is_some_and(predicate);
    // The page is loaded first: a load that starts after the pointer arrived
    // would take the hovered link away again.
    shot.settle_until(Duration::from_secs(10), |_| {
        active(|tab| !tab.load.loading && !tab.title.is_empty())
    });
    wait(shot, 500);
    let (x, y) = *ctx().chrome.page_origin.peek();
    // The pointer keeps moving until the page says it is over the link: the
    // page view may not have taken the first move, and the engine answers a
    // change of what is under the pointer, not a move.
    let mut step = 0.0;
    shot.settle_until(Duration::from_secs(5), |shot| {
        if active(|tab| tab.hover.is_some()) {
            return true;
        }
        step = 1.0 - step;
        shot.hover(x + 100.0 + step, y + 290.0);
        false
    });
    wait(shot, 200);
}

fn zoomed(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::ZoomIn);
    run_command(shot, Command::ZoomIn);
}

fn fullscreen(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    tell(rig, 0, TabEvent::Fullscreen(true));
    wait(shot, 500);
}

fn tab_list(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    // A phone opens the list from the tab count; a desktop window has no such
    // button, so the sheet is opened by the same call the button makes.
    if !shot.click_text("3") {
        let at = ctx();
        shot.runner().run_in(|| crate::tabs::TabsSheet::open(at));
    }
    wait(shot, 1500);
}

fn popup_window(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    let opener = session(0);
    rig.double
        .open_window(opener, "https://news.bingux.dev/popup");
    wait(shot, 600);
}

fn rtl(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    tell(rig, 0, TabEvent::Title("\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629} \u{2013} \u{645}\u{648}\u{642}\u{639} \u{627}\u{62e}\u{628}\u{627}\u{631}".to_string()));
    wait(shot, 500);
}

/// An engine that accepts none of the host-answered request kinds: pages and
/// favicons only. (The engine of 2026-11-25 accepts every kind the browser
/// answers; an older or a cut-down one may not.)
fn engine_of_today() -> Arc<TestDoubleBackend> {
    TestDoubleBackend::with_capabilities(Capabilities {
        favicons: true,
        ..Capabilities::default()
    })
}

/// An engine that does not start because it does not trust the profile folder.
fn engine_unsafe_profile() -> Arc<TestDoubleBackend> {
    TestDoubleBackend::failed_start(
        FailureCause::ProfileUnsafe,
        "profile_dir: /home/kieran/.local/share/bingux-browser/profile is writable by group or others.",
    )
}

/// The main menu over an engine that cannot do everything: what it cannot do
/// is disabled, not hidden.
fn capabilities(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    shot.click_name("Main menu");
    wait(shot, 500);
}

/// The engine process is killed: the window says so, and offers to start it again.
fn engine_failed(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    rig.double.fail_engine(
        FailureCause::Crashed,
        "the engine process was killed",
        137,
        true,
    );
    wait(shot, 600);
}

/// "Start again": a new engine opens the tabs again where they were.
fn engine_restart(shot: &mut Snapshot, rig: &Rig) {
    engine_failed(shot, rig);
    shot.click_text("Start again");
    wait(shot, 1_200);
}

/// The engine broke the protocol and the library ended it.
fn engine_violation(shot: &mut Snapshot, rig: &Rig) {
    wait(shot, 500);
    rig.double.fail_engine(
        FailureCause::ProtocolViolation,
        "a frame was sent for a session that is not there",
        0,
        true,
    );
    wait(shot, 600);
}

/// The settings page with the engine's own account of its security, in a window
/// tall enough to show all of it.
fn security(shot: &mut Snapshot, _: &Rig) {
    wait(shot, 500);
    run_command(shot, Command::Settings);
    wait(shot, 600);
}

/// The same before the engine has produced a report (`NOT_READY`).
fn security_pending(shot: &mut Snapshot, rig: &Rig) {
    rig.double.set_report(None);
    security(shot, rig);
}

fn scenarios() -> Vec<Scenario> {
    vec![
        scenario("newtab", &[], blank),
        scenario("page", &["example.com"], nothing),
        scenario("tabs", TABS, tabs),
        scenario("tab-menu", TABS, tab_menu),
        Scenario {
            manual: true,
            ..scenario("loading", &["example.com"], loading)
        },
        scenario("permission", &["perm.example.net"], permission),
        scenario("dialog", &["dialog.example.net"], permission),
        scenario("auth", &["auth.example.net"], permission),
        scenario("cert", &["expired.example.net"], nothing),
        scenario("cert-details", &["expired.example.net"], cert_details),
        scenario("cert-pinned", &["pinned.example.net"], nothing),
        scenario("error", &["missing.example.net"], nothing),
        scenario("crash", &["crash.example.net"], nothing),
        scenario("find", &["example.com"], find),
        scenario("find-missing", &["example.com"], find_missing),
        scenario("downloads", &["files.example.net"], downloads),
        scenario("info", &["example.com"], info),
        scenario("info-plain", &["plain.example.net"], info),
        scenario("menu", &["example.com"], menu),
        scenario("settings", &["example.com"], settings),
        scenario(
            "suggestions",
            &["example.com", "news.bingux.dev", "docs.example.org"],
            suggestions,
        ),
        scenario("context", &["example.com"], context),
        scenario("hover", &["example.com"], hover),
        scenario("zoom", &["example.com"], zoomed),
        scenario("fullscreen", &["video.example.net"], fullscreen),
        scenario(
            "tab-list",
            &["example.com", "news.bingux.dev", "video.example.net"],
            tab_list,
        ),
        scenario("popup", &["example.com"], popup_window),
        scenario("rtl", &["news.bingux.dev"], rtl),
        Scenario {
            private: true,
            ..scenario("private", &[], blank)
        },
        Scenario {
            engine: Some(engine_of_today),
            ..scenario("capabilities", &["example.com"], capabilities)
        },
        scenario(
            "engine-failed",
            &["example.com", "news.bingux.dev"],
            engine_failed,
        ),
        scenario(
            "engine-restart",
            &["example.com", "news.bingux.dev"],
            engine_restart,
        ),
        scenario("engine-violation", &["example.com"], engine_violation),
        Scenario {
            engine: Some(engine_unsafe_profile),
            ..scenario("profile-unsafe", &[], nothing)
        },
        Scenario {
            tall: 3.2,
            ..scenario("security", &["example.com"], security)
        },
        Scenario {
            tall: 3.2,
            ..scenario("security-pending", &["example.com"], security_pending)
        },
    ]
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

fn sizes() -> Vec<(&'static str, (f32, f32))> {
    if let Ok(text) = std::env::var("BINGUX_BROWSER_SNAPSHOT_SIZE") {
        if let Some((width, height)) = text.split_once('x') {
            if let (Ok(width), Ok(height)) = (width.trim().parse(), height.trim().parse()) {
                return vec![("custom", (width, height))];
            }
        }
    }
    vec![("desktop", (1100.0, 700.0)), ("phone", (390.0, 800.0))]
}

/// `--snapshot DIR real ADDRESS,...`: the window over the real engine, which
/// opens each address and is looked at once its page has a title and has had
/// a few seconds to paint. Needs a `vivarium` build and an SDK.
#[cfg(any(feature = "vivarium", feature = "dynamic-load"))]
fn run_real(dir: &str, addresses: &[String]) {
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
            run_real(&dir, &addresses);
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

/// The window's controls pressed the way a person presses them, over the test
/// double: what opens the downloads list.
///
/// Failures these tests guard against:
///   1. the toolbar's downloads button does nothing while a download runs,
///      which is when the person wants it (the button was busy, and a busy
///      icon button swallows presses);
///   2. the toast's "Show" does not open the list;
///   3. the button does nothing when nothing runs either (the control);
///   4. Escape does not close the downloads list, which every popover is
///      documented to do (a page that holds the keyboard hears it first);
///   5. the downloads button's tooltip stays up behind the list that opens
///      over it, peeking out from under its edge (a popover is a modal
///      surface, and tips go down for those);
///   6. a download that failed or was cancelled has no way to be tried
///      again (the engine's Resume takes it from where it stopped).
#[cfg(test)]
mod tests {
    use super::*;

    fn open(shot: &mut Snapshot) -> bool {
        let _ = shot;
        ctx().downloads_popover.is_open()
    }

    fn window() -> (Snapshot, Rig) {
        CTX.with(|slot| slot.set(None));
        let rig = Rig {
            double: TestDoubleBackend::new(),
        };
        let scenario = scenario("press", &["files.example.net"], nothing);
        let mut shot = Snapshot::new(app(&rig, &scenario), (1100.0, 700.0));
        wait(&mut shot, 800);
        (shot, rig)
    }

    #[test]
    fn the_downloads_button_opens_the_list_while_a_download_runs() {
        let (mut shot, _rig) = window();
        assert_eq!(ctx().browser.peek().downloads.active(), 1);
        assert!(
            shot.click_name("Downloads: 1 running"),
            "the button is there"
        );
        wait(&mut shot, 400);
        assert!(open(&mut shot));
    }

    #[test]
    fn the_downloads_button_opens_the_list_when_nothing_runs() {
        let (mut shot, rig) = window();
        rig.double.advance_downloads(10_000_000);
        wait(&mut shot, 400);
        assert_eq!(ctx().browser.peek().downloads.active(), 0);
        assert!(shot.click_name("Downloads"), "the button is there");
        wait(&mut shot, 400);
        assert!(open(&mut shot));
    }

    #[test]
    fn the_toasts_show_opens_the_list() {
        let (mut shot, _rig) = window();
        // The button's words are drawn text, not an accessible name.
        let shown = shot.runner().find_many(|node, element| {
            let label = Label::try_downcast(element)?;
            (label.text == "Show").then(|| Area::center(&node.layout().area))
        });
        let centre = shown.first().expect("the toast for the download is up");
        shot.click(centre.x, centre.y);
        wait(&mut shot, 400);
        assert!(open(&mut shot));
    }

    #[test]
    fn escape_closes_the_downloads_list() {
        let (mut shot, _rig) = window();
        // The person clicked into the page: it holds the keyboard.
        shot.click(400.0, 400.0);
        wait(&mut shot, 200);
        let at = ctx();
        shot.runner().run_in(|| at.run(Command::Downloads));
        wait(&mut shot, 400);
        assert!(open(&mut shot), "the list is open");
        shot.run_script("key escape", Target::SCHEME)
            .expect("a key");
        wait(&mut shot, 400);
        assert!(!open(&mut shot), "Escape closed it");
    }

    /// The drawn bubbles that say `text`: the visible one and its hidden
    /// measuring copy are both labels, and a tip that is down has neither.
    fn bubbles(shot: &mut Snapshot, text: &str) -> usize {
        shot.runner()
            .find_many(|_, element| {
                let label = Label::try_downcast(element)?;
                (label.text == text).then_some(())
            })
            .len()
    }

    #[test]
    fn a_tooltip_goes_down_when_the_list_opens_over_its_button() {
        let (mut shot, _rig) = window();
        let tip = "Downloads: 1 running";
        let button = shot.runner().find_many(|node, element| {
            (element.accessibility().builder.label() == Some(tip))
                .then(|| Area::center(&node.layout().area))
        });
        let at = button.first().expect("the button is there");
        shot.runner()
            .move_cursor((f64::from(at.x), f64::from(at.y)));
        wait(&mut shot, 900);
        assert!(
            bubbles(&mut shot, tip) > 0,
            "the tip is up while the pointer rests"
        );
        let window = ctx();
        shot.runner().run_in(|| window.run(Command::Downloads));
        wait(&mut shot, 900);
        assert!(open(&mut shot), "the list is open");
        assert_eq!(bubbles(&mut shot, tip), 0, "the tip went down with it");
    }

    /// A download the engine says failed: the page's own (id 1), as it stood.
    fn fail_the_download(rig: &Rig) {
        let mut failed = ctx().browser.peek().downloads.list()[0].clone();
        failed.state = bingux_ui::web_view::requests::DownloadState::Failed;
        failed.speed = 0;
        rig.double.emit(BrowserEvent::Download(failed));
    }

    #[test]
    fn a_failed_download_can_be_tried_again() {
        use bingux_ui::web_view::requests::DownloadState;
        use bingux_ui::web_view::test_double::Call;
        let (mut shot, rig) = window();
        let id = ctx().browser.peek().downloads.list()[0].id;
        fail_the_download(&rig);
        wait(&mut shot, 300);
        assert_eq!(
            ctx().browser.peek().downloads.list()[0].state,
            DownloadState::Failed
        );
        let at = ctx();
        shot.runner().run_in(|| at.run(Command::Downloads));
        wait(&mut shot, 400);
        assert!(
            shot.click_name("Retry"),
            "a failed download has a Retry button"
        );
        wait(&mut shot, 400);
        assert!(
            rig.double
                .calls()
                .iter()
                .any(|call| matches!(call, Call::ResumeDownload(known) if *known == id)),
            "the engine was asked to resume it"
        );
        assert_eq!(
            ctx().browser.peek().downloads.list()[0].state,
            DownloadState::Running
        );
    }

    #[test]
    fn a_download_that_runs_has_no_retry() {
        let (mut shot, _rig) = window();
        let at = ctx();
        shot.runner().run_in(|| at.run(Command::Downloads));
        wait(&mut shot, 400);
        assert!(!shot.click_name("Retry"), "pause and cancel, not retry");
    }
}
