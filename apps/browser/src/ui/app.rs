//! The window: the tab strip on the title bar and, under it, the toolbar, the
//! banners, the find bar and the page. On a phone there is no strip: the
//! address bar is on top, the history below, and the tabs are a list.

use bingux_ui::prelude::*;
use bingux_ui::shell::with_window;
use bingux_ui::web_view::PageEnvironment;
use bingux_ui::web_view::queue::Signal;
use freya::prelude::*;

use crate::address::FieldState;
use crate::browser::{Browser, Options, Profile};
use crate::commands::{Command, Platform};
use crate::downloads::CardState;
use crate::engine::{self, Engine, EngineFailedPage};
use crate::find::{BarState, FindBar};
use crate::menus::ContextMenuHost;
use crate::site_info::PopoverState;
use crate::tabs::Place;

use super::ctx::{Chrome, Ctx};
use super::page::PageArea;
use super::popovers::Popovers;
use super::toolbar::{PhoneBottom, Toolbar};
use crate::permissions::PermissionBanner;
use crate::prompts::{DialogFields, PromptHost};
use crate::tabs::Strip;

/// The profile a window opens in, handed from `main` to the window's first
/// frame once (it holds what cannot be copied, such as the store's writer).
#[derive(Clone)]
pub struct Handoff(std::rc::Rc<std::cell::RefCell<Option<Profile>>>);

impl Handoff {
    pub fn new(profile: Profile) -> Self {
        Self(std::rc::Rc::new(std::cell::RefCell::new(Some(profile))))
    }

    /// No profile: a test or a snapshot.
    pub fn none() -> Self {
        Self(std::rc::Rc::new(std::cell::RefCell::new(None)))
    }

    fn take(&self) -> Profile {
        self.0.borrow_mut().take().unwrap_or_default()
    }
}

impl PartialEq for Handoff {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The window root.
#[derive(PartialEq)]
pub struct App {
    pub engine: Engine,
    pub options: Options,
    /// What the tabs open on at start, when nothing else was asked for.
    pub start: Vec<String>,
    pub profile: Handoff,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(Content {
                engine: self.engine.clone(),
                options: self.options,
                start: self.start.clone(),
                profile: self.profile.clone(),
            }))
    }
}

#[derive(PartialEq)]
struct Content {
    engine: Engine,
    options: Options,
    start: Vec<String>,
    profile: Handoff,
}

/// The addresses a launch names: the `open` pairs of its location.
fn opened_by(location: &Location) -> Vec<String> {
    location
        .query_pairs()
        .iter()
        .filter(|(key, _)| key == "open")
        .map(|(_, value)| value.clone())
        .collect()
}

impl Component for Content {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let location = use_location();
        let engine = self.engine.clone();
        let options = self.options;
        // What pages see of the window's look: its scheme, accent, motion and
        // text scale, sent again whenever they change.
        let environment =
            PageEnvironment::from_tokens(&tokens, bingux_ui::access::text_scale(), Vec::new());
        let browser = use_state({
            let engine = engine.clone();
            let environment = environment.clone();
            let profile = self.profile.clone();
            move || {
                let mut browser =
                    Browser::with_profile(engine.backend().clone(), options, profile.take());
                browser.environment = environment;
                browser
            }
        });
        let chrome = Chrome {
            page_origin: use_state(|| (0.0, 0.0)),
        };
        let ctx = Ctx {
            browser,
            chrome,
            address: FieldState::use_new(),
            site_info: PopoverState::use_new(),
            find: BarState::use_new(),
            downloads_popover: CardState::use_new(),
            prompts: DialogFields::use_new(),
            overlays,
            window,
            platform: Platform::current(),
        };
        use_window_commands(crate::commands::Registry::commands(ctx));
        #[cfg(feature = "snapshot")]
        crate::snapshot_mode::publish(ctx);
        // The engine's events and the profile store's wake the window: it
        // takes them in and sleeps again. The first tabs open once the window
        // exists.
        let first: Vec<String> = {
            let launched = opened_by(&location);
            if launched.is_empty() {
                self.start.clone()
            } else {
                launched
            }
        };
        use_hook({
            let engine = engine.clone();
            move || {
                let signal = Signal::new();
                engine.backend().set_waker(signal.waker());
                ctx.browser.peek().set_store_waker(signal.waker());
                spawn(async move {
                    ctx.with(|browser| {
                        browser.poll();
                        browser.open_first(&first);
                    });
                    if first_is_blank(ctx) {
                        ctx.address.focus_at_open();
                    }
                    loop {
                        signal.notified().await;
                        ctx.pump();
                    }
                });
            }
        });
        use_side_effect_with_deps(&environment, move |environment: &PageEnvironment| {
            let environment = environment.clone();
            ctx.with(move |browser| browser.set_environment(environment));
        });
        // A second launch changes the location: what it names opens in new tabs.
        let handled = use_hook({
            let first = location.clone();
            move || std::rc::Rc::new(std::cell::RefCell::new(first))
        });
        if *handled.borrow() != location {
            *handled.borrow_mut() = location.clone();
            let urls = opened_by(&location);
            spawn(async move {
                ctx.with(|browser| {
                    for url in &urls {
                        browser.open_tab(url, Place::End, true);
                    }
                });
            });
        }

        // The window's title follows the tab on show.
        let (title, fullscreen, failed, key, find_open, empty_window) = {
            let browser = ctx.browser.read();
            let title = browser
                .active()
                .map_or_else(String::new, |tab| tab.display_title());
            (
                title,
                browser.fullscreen,
                match &browser.engine.status {
                    engine::Status::Failed {
                        message,
                        code,
                        cause,
                        restartable,
                    } => Some((message.clone(), *code, *cause, *restartable)),
                    _ => None,
                },
                browser.tabs.active_key(),
                browser.active().is_some_and(|tab| tab.find.is_open()),
                browser.tabs.is_empty(),
            )
        };
        let private = options.private;
        use_side_effect_with_deps(
            &(title.clone(), private),
            move |(title, private): &(String, bool)| {
                let name = match (title.is_empty(), private) {
                    (true, true) => tr!("browser-window-title-private"),
                    (true, false) => tr!("browser-title"),
                    (false, true) => {
                        tr!("browser-window-title-private-page", title = title.as_str())
                    }
                    (false, false) => tr!("browser-window-title-page", title = title.as_str()),
                };
                with_window(move |window| window.set_title(&name));
            },
        );
        // The window says it is the one the person is in, so the page shows a caret.
        let focused = bingux_ui::runtime::window_focus();
        use_side_effect_with_deps(&*focused.read(), move |focused: &bool| {
            ctx.with(|browser| browser.set_window_focus(*focused));
        });

        if let Some((message, code, cause, restartable)) = failed {
            return EngineFailedPage {
                ctx,
                cause,
                message,
                code,
                restartable,
            }
            .into_element();
        }
        if empty_window {
            // The last tab went: the window is closing.
            return rect().into_element();
        }

        let shortcuts = move |event: Event<KeyboardEventData>| {
            if let Some(command) = ctx.platform.window_command(&event.key, event.modifiers) {
                event.stop_propagation();
                ctx.with(|browser| browser.run(command));
            } else if event.key == Key::Named(NamedKey::Escape) && ctx.browser.peek().fullscreen {
                event.stop_propagation();
                ctx.with(|browser| browser.run(Command::Fullscreen));
            }
        };

        let page = PageArea {
            ctx,
            engine: engine.clone(),
        };
        // The find card floats over the page's top right and never moves it.
        let find = (find_open && !fullscreen).then(|| {
            rect()
                .key(key)
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::fill())
                .child(FindBar { ctx, key })
        });
        let compact = is_compact();
        // On a desktop the toolbar is the window's header and the tabs its strip
        // (`Screen`); a phone and a full-screen page keep the column of their own.
        let framed = !compact && !fullscreen;
        let body = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_flex(),
                    flex_col(),
                    grow_children(),
                    bg(Paint::Bg),
                ],
            )
            .maybe_child((compact && !fullscreen).then(|| Toolbar {
                ctx,
                in_header: false,
            }))
            .maybe_child((!fullscreen).then(|| PermissionBanner { ctx }))
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_col(), grow_children()])
                    .child(page)
                    .maybe_child(find),
            )
            .maybe_child((compact && !fullscreen).then(|| PhoneBottom { ctx }))
            .child(Popovers { ctx })
            .child(PromptHost { ctx })
            .child(ContextMenuHost { ctx });

        let root = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    flex_col(),
                    grow_children(),
                    bg(Paint::Bg),
                ],
            )
            .on_key_down(shortcuts);
        if !framed {
            return root.child(body).into_element();
        }
        // The tabs are the strip above the toolbar, with the window's buttons; the
        // toolbar is the header of the tab on show.
        let tabs = HeaderBar::new(title.clone())
            .trailing(WindowControls)
            .tab_slot(Strip::build(ctx));
        let header = HeaderBar::new(title).title_slot(Toolbar {
            ctx,
            in_header: true,
        });
        root.child(Screen::new(header).strip(tabs).content(body))
            .into_element()
    }
}

/// Whether the tab on show is the blank one a launch without an address makes.
fn first_is_blank(ctx: Ctx) -> bool {
    ctx.browser
        .peek()
        .active()
        .is_some_and(|tab| tab.is_blank())
}
