//! The window: the header, the sidebar, and the view on show. Everything the
//! keyboard does, a file dropped on the window, and a second launch's
//! arguments are decided here.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use bingux_calendar_core::watch::Event as WatchEvent;
use bingux_calendar_core::{Prefs, Store, ViewKind};
use bingux_ui::calendar::Date;
use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use bingux_ui::task::Feed;
use freya::prelude::*;

use crate::calendars;
use crate::editor::{self, Seed};
use crate::header::Header;
use crate::model::{Clock, EventRef, Message, Model};
use crate::quick;
use crate::sidebar::Sidebar;
use crate::views::agenda::{AgendaView, SearchResults};
use crate::views::grid::{GridView, Span3, first_day};
use crate::views::month::MonthPage;
use crate::views::year::YearPage;

/// The window root: the content sits below the overlay host, which provides
/// the window handle, the dialogs and the menus.
#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(Content))
    }
}

/// Runs reminders with no window, until the process is stopped.
pub fn run_reminders() {
    let stop = Arc::new(AtomicBool::new(false));
    bingux_calendar_core::alarm::run(
        &Store::open(),
        &stop,
        || Prefs::load().twelve_hour(bingux_ui::locale::Locale::current().uses_12_hour()),
        |_, _| {},
    );
}

#[derive(PartialEq)]
struct Content;

impl Component for Content {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let location = use_location();
        let model = Model::create();
        let search_id = use_a11y();
        let root_id = use_a11y();
        let mut dropping = use_state(|| false);
        let maximized = freya::borderless::use_maximized();
        let _ = maximized;

        // The threads tell the window through one queue, which sleeps until
        // something comes.
        use_hook(move || {
            let feed: Arc<Feed<Message>> = Arc::new(Feed::default());
            model.start(feed.clone());
            spawn(async move {
                while let Some(message) = feed.next().await {
                    match message {
                        Message::Watch(WatchEvent::Minute(_)) => {
                            let mut clock = model.clock;
                            clock.set_if_modified(Clock::now());
                        }
                        Message::Watch(WatchEvent::Changed) => model.reload(),
                        Message::Watch(WatchEvent::Reminder { title, body }) => {
                            overlays.toast(format!("{title}. {body}"));
                        }
                        Message::SaveFailed(_) => overlays.toast(tr!("calendar-save-failed")),
                    }
                }
            });
        });

        // A launch argument, or a second launch's: a file to import or a day.
        let handled = use_hook(|| Rc::new(RefCell::new(None::<Location>)));
        if handled.borrow().as_ref() != Some(&location) {
            *handled.borrow_mut() = Some(location.clone());
            let location = location.clone();
            spawn(async move { act_on(model, overlays, &location) });
        }

        let compact = is_compact();
        let query = model.query.read().clone();
        let searching = !query.trim().is_empty();
        let view = *model.view.read();

        let shortcuts = move |event: Event<KeyboardEventData>| {
            keys(model, window, overlays, search_id, &event);
        };

        let body: Element = if searching {
            SearchResults {
                model,
                query: query.clone(),
            }
            .into_element()
        } else {
            match view {
                ViewKind::Day => GridView {
                    model,
                    span: Span3::Day,
                }
                .into_element(),
                ViewKind::Week => GridView {
                    model,
                    span: if compact {
                        Span3::ThreeDays
                    } else {
                        Span3::Week
                    },
                }
                .into_element(),
                ViewKind::Month => MonthPage { model }.into_element(),
                ViewKind::Year => YearPage { model }.into_element(),
                ViewKind::Agenda => AgendaView { model }.into_element(),
            }
        };

        let hint = (model.is_empty() && !*model.hint_dismissed.read() && !searching).then(|| {
            banner(tr!("calendar-empty-hint"))
                .title(tr!("calendar-empty-title"))
                .icon(Icon::XOfficeCalendar)
                .action(tr!("calendar-empty-add"), move |_| {
                    quick::open(model, window, overlays)
                })
                .action(tr!("calendar-import"), move |_| {
                    calendars::import(model, window, overlays)
                })
                .dismissible(move |_| {
                    let mut dismissed = model.hint_dismissed;
                    dismissed.set(true);
                })
        });

        let search_row = (compact && (*model.search_open.read() || searching)).then(|| {
            rect().tw(&tokens, &[w_full(), px(S2), py(S1)]).child(
                search_field(model.query, move |_: String| {})
                    .fill_width()
                    .a11y_id(search_id),
            )
        });

        let main = rect()
            .tw(
                &tokens,
                &[
                    w_flex(),
                    h_full(),
                    flex_col(),
                    grow_children(),
                    bg(Paint::Bg),
                ],
            )
            .maybe_child(search_row)
            .maybe_child(hint.map(|hint| rect().tw(&tokens, &[w_full(), p(S2)]).child(hint)))
            .child(rect().tw(&tokens, &[w_full(), h_flex()]).child(body));

        let drop_cover = dropping().then(|| {
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .tw(&tokens, &[w_full(), h_full(), center(), bg(Paint::Scrim)])
                .child(
                    status_page(Icon::DocumentOpen, tr!("calendar-drop-title"))
                        .description(tr!("calendar-drop-description")),
                )
        });

        rect()
            .component("Calendar")
            .a11y_id(root_id)
            .a11y_focusable(true)
            .a11y_auto_focus(true)
            .a11y_role(AccessibilityRole::Window)
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
            .on_key_down(shortcuts)
            .on_global_file_hover(move |_| dropping.set_if_modified(true))
            .on_global_file_hover_cancelled(move |_| dropping.set_if_modified(false))
            .on_file_drop(move |event: Event<FileEventData>| {
                dropping.set_if_modified(false);
                for path in event.file_paths.iter().cloned() {
                    calendars::import_file(model, overlays, path);
                }
            })
            .child(Header { model, search_id })
            .child(
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_row(), grow_children()])
                    .maybe_child((!compact).then(|| Sidebar {
                        model,
                        embedded: false,
                    }))
                    .child(main),
            )
            .maybe_child(drop_cover)
    }
}

/// What a launch address asks for.
fn act_on(model: Model, overlays: Overlays, location: &Location) {
    match location.segments() {
        [first, day] if first == "date" => {
            if day == "today" {
                model.today();
            } else if let Some(date) = Date::parse(day, model.clock.peek().today, false) {
                model.go(date);
                if matches!(*model.view.peek(), ViewKind::Agenda | ViewKind::Year) {
                    model.show(ViewKind::Day);
                }
            }
        }
        [first] if first == "open" => {
            if let Some(path) = location.query("file") {
                calendars::import_file(model, overlays, PathBuf::from(path));
            }
        }
        _ => {}
    }
}

/// The keys of the window.
fn keys(
    model: Model,
    window: Window,
    overlays: Overlays,
    search_id: AccessibilityId,
    event: &Event<KeyboardEventData>,
) {
    if bingux_ui::nav::modal_open() || overlays.dialog_open() {
        return;
    }
    // A text field keeps its letters and digits: 1-5, t, n and e typed in it
    // must not switch views or open a dialog.
    if bingux_ui::focus::text_entry_focused() {
        return;
    }
    let Key::Character(text) = &event.key else {
        return other_keys(model, window, overlays, event);
    };
    let text = text.to_lowercase();
    let command = event.modifiers.intersects(Modifiers::ctrl_or_meta());
    if command && text == "f" {
        event.stop_propagation();
        let mut open = model.search_open;
        open.set(true);
        search_id.request_focus();
        return;
    }
    if command || event.modifiers.contains(Modifiers::ALT) {
        return;
    }
    let handled = match text.as_str() {
        "t" => {
            model.today();
            true
        }
        "n" => {
            quick::open(model, window, overlays);
            true
        }
        "/" => {
            let mut open = model.search_open;
            open.set(true);
            search_id.request_focus();
            true
        }
        "1" => show(model, ViewKind::Day),
        "2" => show(model, ViewKind::Week),
        "3" => show(model, ViewKind::Month),
        "4" => show(model, ViewKind::Year),
        "5" => show(model, ViewKind::Agenda),
        "e" => open_selected(model, window, overlays),
        _ => false,
    };
    if handled {
        event.stop_propagation();
        event.prevent_default();
    }
}

/// The timed events a day or week grid shows, by day and then start.
fn grid_events(model: &Model, view: ViewKind, compact: bool) -> Vec<String> {
    let span = match (view, compact) {
        (ViewKind::Day, _) => Span3::Day,
        (_, true) => Span3::ThreeDays,
        _ => Span3::Week,
    };
    let anchor = *model.anchor.peek();
    let first = first_day(model, anchor, span);
    let mut found: Vec<_> = model
        .occurrences(first, first.add_days(span.count()))
        .into_iter()
        .filter(|occurrence| {
            occurrence.status != bingux_calendar_core::Status::Cancelled && !occurrence.is_banner()
        })
        .collect();
    found.sort_by(|a, b| a.start.cmp(&b.start));
    found
        .iter()
        .map(|occurrence| EventRef::of(occurrence).id())
        .collect()
}

fn show(model: Model, kind: ViewKind) -> bool {
    model.show(kind);
    true
}

fn open_selected(model: Model, window: Window, overlays: Overlays) -> bool {
    let selected = model.selected.peek().clone();
    match selected {
        Some(id) => {
            editor::open(
                model,
                window,
                overlays,
                Seed::Existing(EventRef::parse(&id)),
            );
            true
        }
        None => false,
    }
}

fn other_keys(model: Model, window: Window, overlays: Overlays, event: &Event<KeyboardEventData>) {
    let view = *model.view.peek();
    let month_like = matches!(view, ViewKind::Month | ViewKind::Year);
    let anchor = *model.anchor.peek();
    let picked = model.selected.peek().clone();
    // The month walks the events of its chosen day; the grids do it themselves.
    if view == ViewKind::Month {
        let events = model.day_events(anchor);
        let at = picked
            .as_ref()
            .and_then(|id| events.iter().position(|other| other == id));
        let walked = match &event.key {
            Key::Named(NamedKey::ArrowDown) if at.is_some() => {
                if let Some(next) = at.and_then(|at| events.get(at + 1)) {
                    model.selected.clone().set(Some(next.clone()));
                }
                true
            }
            Key::Named(NamedKey::ArrowUp) if at.is_some() => {
                let next = at.filter(|at| *at > 0).map(|at| events[at - 1].clone());
                model.selected.clone().set(next);
                true
            }
            Key::Named(NamedKey::Enter) => {
                match (picked.as_ref().filter(|_| at.is_some()), events.first()) {
                    (Some(_), _) => {
                        open_selected(model, window, overlays);
                    }
                    (None, Some(first)) => model.selected.clone().set(Some(first.clone())),
                    (None, None) => {
                        model.show(ViewKind::Day);
                    }
                }
                true
            }
            _ => false,
        };
        if walked {
            event.stop_propagation();
            event.prevent_default();
            return;
        }
    }
    // The grids walk their blocks with Up and Down; the same keys work when
    // the window itself has the focus.
    if matches!(view, ViewKind::Day | ViewKind::Week) {
        let events = grid_events(&model, view, is_compact());
        let at = picked
            .as_ref()
            .and_then(|id| events.iter().position(|other| other == id));
        let next = match &event.key {
            Key::Named(NamedKey::ArrowDown) => Some(match at {
                Some(at) => events.get(at + 1).or(events.first()),
                None => events.first(),
            }),
            Key::Named(NamedKey::ArrowUp) => Some(match at {
                Some(at) if at > 0 => events.get(at - 1),
                _ => events.last(),
            }),
            Key::Named(NamedKey::Enter) if at.is_some() => {
                open_selected(model, window, overlays);
                event.stop_propagation();
                return;
            }
            _ => None,
        };
        if let Some(next) = next {
            if let Some(next) = next {
                model.selected.clone().set(Some(next.clone()));
            }
            event.stop_propagation();
            event.prevent_default();
            return;
        }
    }
    let handled = match &event.key {
        Key::Named(NamedKey::ArrowLeft) => {
            if month_like {
                model.go(anchor.add_days(-1))
            } else {
                model.step(-1)
            }
            true
        }
        Key::Named(NamedKey::ArrowRight) => {
            if month_like {
                model.go(anchor.add_days(1))
            } else {
                model.step(1)
            }
            true
        }
        Key::Named(NamedKey::ArrowUp) if month_like => {
            model.go(anchor.add_days(-7));
            true
        }
        Key::Named(NamedKey::ArrowDown) if month_like => {
            model.go(anchor.add_days(7));
            true
        }
        Key::Named(NamedKey::PageUp) => {
            model.go(anchor.add_months(-1));
            true
        }
        Key::Named(NamedKey::PageDown) => {
            model.go(anchor.add_months(1));
            true
        }
        Key::Named(NamedKey::Delete) => {
            let selected = model.selected.peek().clone();
            let Some(id) = selected else {
                return;
            };
            let reference = EventRef::parse(&id);
            if let Some(item) = model.item(&reference.uid) {
                editor::delete_event(model, overlays, item, reference.key, None);
                true
            } else {
                false
            }
        }
        Key::Named(NamedKey::Escape) if picked.is_some() => {
            model.selected.clone().set(None);
            true
        }
        Key::Named(NamedKey::Escape) => {
            let mut query = model.query;
            if query.peek().is_empty() {
                return;
            }
            query.set(String::new());
            true
        }
        _ => false,
    };
    if handled {
        event.stop_propagation();
        event.prevent_default();
    }
}
