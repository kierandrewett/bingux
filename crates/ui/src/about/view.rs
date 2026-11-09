//! The About window: a card of one fixed size with a header and a page that
//! scrolls inside it.
//!
//! The main page is what a person came for (the icon, the name, the version,
//! what it does, where to read about it and where to report a problem) and
//! three doors: Licence, Credits and Legal. Each door opens a page in the same
//! card, with a Back button where Close was joined by it. The card does not
//! change size between pages.
//!
//! It is a [`framed_dialog`]: Escape and a press on the scrim close it, the
//! page behind is inert while it is open, and the keyboard focus goes back to
//! where it was when it closes. On a phone the card keeps a margin from the
//! window's edges and the controls take the compact density.

use freya::prelude::*;

use super::details::{self, Environment, details_text};
use super::info::AppInfo;
use super::link::{self, Legal, Lookup};
use crate::app_icon::app_icon;
use crate::component::ComponentName;
use crate::controls::{action_button, icon_button};
use crate::group::{Group, caption};
use crate::icons::Icon;
use crate::rows::row;
use crate::shell::{HeaderBar, Overlays, framed_dialog};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::use_resource;
use crate::theme::Tokens;
use crate::typography::{BODY, CAPTION, Ink, TITLE, TextStyle};

/// One page of the About window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Licence,
    Credits,
    Legal,
}

impl Page {
    fn title(self) -> String {
        match self {
            Page::Main => crate::tr!("ui-about-page-main"),
            Page::Licence => crate::tr!("ui-about-licence"),
            Page::Credits => crate::tr!("ui-about-credits"),
            Page::Legal => crate::tr!("ui-about-legal"),
        }
    }
}

/// Open the About window of `info` in the window `overlays` belongs to, on its
/// main page.
pub fn open_with(overlays: Overlays, info: AppInfo) {
    open_at(overlays, info, Page::Main);
}

/// [`open_with`] on `page`. The gallery and the tests use it to show a page
/// without clicking to it.
pub fn open_at(overlays: Overlays, info: AppInfo, page: Page) {
    overlays.show_dialog(framed_dialog(
        crate::tr!("ui-about-title", app = info.name.as_str()),
        Element::from(AboutBody {
            info,
            overlays,
            start: page,
        }),
        Dim::Dialog.px(),
        Dim::PopupHeight.px(),
    ));
}

/// The card's content: the header, and the page that is showing.
#[derive(PartialEq)]
pub struct AboutBody {
    pub info: AppInfo,
    pub overlays: Overlays,
    pub start: Page,
}

/// The facts the details block carries that depend on this window. The block
/// is copied into bug reports, so it stays in English, like the labels of
/// `details::details_text`.
fn environment(tokens: &Tokens) -> Environment {
    Environment {
        facts: details::facts(),
        scale: *Platform::get().current_window().scale_factor.read(),
        theme: if tokens.scheme.is_dark() {
            "Dark"
        } else {
            "Light"
        }
        .to_owned(),
        accent: tokens.accent_name.to_owned(),
        density: if is_compact() { "Compact" } else { "Regular" }.to_owned(),
    }
}

/// The face of the dependency list: caption size, monospace, so the names and
/// versions line up.
const LISTING: TextStyle = TextStyle {
    mono: true,
    ..CAPTION
};

/// How many lines of the dependency list one text block holds. The list is a
/// few hundred lines; blocks keep each paragraph to a size the text engine
/// lays out at once.
const LISTING_BLOCK: usize = 40;

impl Component for AboutBody {
    fn render(&self) -> impl IntoElement {
        let direction = crate::controls::direction::use_direction();
        let tokens = use_tokens();
        let overlays = self.overlays;
        let info = &self.info;
        let start = self.start;
        let mut page = use_state(move || start);
        let current = page();
        // The rows of its pages know their width on their first frame: the
        // card's, less the page's padding on both sides.
        crate::rows::provide_row_width(
            crate::shell::dialog::frame_width(Dim::Dialog.px()) - 2.0 * S4.px(),
        );
        let scrolls = [
            use_scroll_controller(ScrollConfig::default),
            use_scroll_controller(ScrollConfig::default),
            use_scroll_controller(ScrollConfig::default),
            use_scroll_controller(ScrollConfig::default),
        ];
        let legal = use_resource(
            Lookup {
                wanted: current == Page::Legal,
                id: info.id.clone(),
                given: info.third_party.clone(),
            },
            link::find_third_party,
        );
        let legal = legal.with(|read| read.value().cloned());

        let content: Vec<Element> = match current {
            Page::Main => main_page(&tokens, info, overlays, &environment(&tokens), page),
            Page::Licence => licence_page(&tokens, info),
            Page::Credits => credits_page(info),
            Page::Legal => legal_page(&tokens, info, legal),
        };

        let mut header = HeaderBar::new(current.title()).draggable(false).trailing(
            icon_button(Icon::WindowClose, move |_| overlays.dismiss_dialog())
                .tooltip(&crate::tr!("ui-dialog-close")),
        );
        if current != Page::Main {
            header = header.leading(
                icon_button(Icon::GoPrevious, move |_| page.set(Page::Main))
                    .tooltip(&crate::tr!("ui-nav-back")),
            );
        }
        let scroll = scrolls[match current {
            Page::Main => 0,
            Page::Licence => 1,
            Page::Credits => 2,
            Page::Legal => 3,
        }];

        rect()
            .component("About")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                // Alt+Left goes back, as it does in every window with a Back (Alt+Right where the
                // window reads from the right).
                let back = event.modifiers.contains(Modifiers::ALT)
                    && direction.reading_key(&event.key) == Key::Named(NamedKey::ArrowLeft);
                if back && *page.peek() != Page::Main {
                    event.stop_propagation();
                    page.set(Page::Main);
                }
            })
            .child(header)
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .component("AboutPage")
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(
                        ScrollView::new_controlled(scroll)
                            .key(current.title())
                            .child(
                                rect()
                                    .tw(&tokens, &[flex_col(), w_full(), gap(S4), p(S4)])
                                    .children(content),
                            ),
                    ),
            )
    }
}

/// A link row: the title, the address it opens as its value, and the whole row
/// pressable. A link that cannot start says so in a toast.
fn link_row(title: String, url: &str, overlays: Overlays) -> Element {
    let url_owned = url.to_owned();
    row(title)
        .value(link::shown(url))
        .on_press(move |_| {
            link::open(&url_owned, move || {
                overlays.toast(crate::tr!("ui-about-link-failed"));
            });
        })
        .into()
}

fn main_page(
    tokens: &Tokens,
    info: &AppInfo,
    overlays: Overlays,
    environment: &Environment,
    mut page: State<Page>,
) -> Vec<Element> {
    let centred = |style: TextStyle, text: &str| {
        style
            .paragraph(tokens, text.to_owned())
            .width(Size::fill())
            .text_align(TextAlign::Center)
    };
    let mut hero = rect()
        .component("AboutHero")
        .tw(
            tokens,
            &[flex_col(), items(Align::Center), w_full(), gap(S2), pt(S2)],
        )
        .child(app_icon(info.icon.clone(), IconSize::AppLarge))
        .child(centred(TITLE, &info.name));
    let version = info.version_line();
    if !version.is_empty() {
        hero = hero.child(centred(CAPTION.in_ink(Ink::Muted), &version));
    }
    if !info.description.is_empty() {
        hero = hero.child(centred(BODY.in_ink(Ink::Muted), &info.description));
    }

    let mut elements: Vec<Element> = vec![hero.into()];

    let mut links = Group::new(None::<String>);
    let mut any_link = false;
    if link::is_openable(&info.website) {
        links = links.row(link_row(crate::tr!("ui-about-website"), &info.website, overlays));
        any_link = true;
    }
    if link::is_openable(&info.issues) {
        links = links.row(link_row(crate::tr!("ui-about-report-issue"), &info.issues, overlays));
        any_link = true;
    }
    if any_link {
        elements.push(links.into());
    }

    let mut doors = Group::new(None::<String>);
    doors = doors.row(
        row(crate::tr!("ui-about-licence"))
            .value(info.licence.clone())
            .navigates()
            .on_press(move |_| page.set(Page::Licence)),
    );
    doors = doors.row(
        row(crate::tr!("ui-about-credits"))
            .navigates()
            .on_press(move |_| page.set(Page::Credits)),
    );
    doors = doors.row(
        row(crate::tr!("ui-about-legal"))
            .navigates()
            .on_press(move |_| page.set(Page::Legal)),
    );
    elements.push(doors.into());

    let mut facts = Group::new(Some(crate::tr!("ui-about-details")));
    let mut any_fact = false;
    if !info.id.is_empty() {
        facts = facts.row(row(crate::tr!("ui-about-app-id")).mono_value(info.id.clone()));
        any_fact = true;
    }
    let build = [super::info::short_commit(&info.commit), info.built.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if !build.is_empty() {
        facts = facts.row(row(crate::tr!("ui-about-build")).mono_value(build));
        any_fact = true;
    }
    if any_fact {
        elements.push(facts.into());
    }

    let text = details_text(info, environment);
    elements.push(
        rect()
            .tw(tokens, &[flex_row(), justify(Align::Center), w_full()])
            .child(
                action_button(
                    crate::tr!("ui-error-copy-details"),
                    move |_| match freya::clipboard::Clipboard::set(text.clone()) {
                        Ok(()) => overlays.toast(crate::tr!("ui-error-details-copied")),
                        Err(_) => overlays.toast(crate::tr!("ui-error-details-not-copied")),
                    },
                )
                .icon(Icon::EditCopy),
            )
            .into(),
    );
    elements
}

/// The licence: the SPDX name, then the text when the application has it.
fn licence_page(tokens: &Tokens, info: &AppInfo) -> Vec<Element> {
    let mut elements: Vec<Element> = Vec::new();
    if !info.licence.is_empty() {
        elements.push(
            Group::new(None::<String>)
                .row(row(info.name.clone()).value(info.licence.clone()))
                .into(),
        );
    }
    if info.licence_text.trim().is_empty() {
        let note = if info.licence.is_empty() {
            crate::tr!("ui-about-no-licence", app = info.name.as_str())
        } else {
            crate::tr!(
                "ui-about-licence-without-text",
                app = info.name.as_str(),
                licence = info.licence.as_str()
            )
        };
        elements.push(caption(note));
    } else {
        elements.push(text_card(
            None,
            tokens,
            BODY,
            &details::paragraphs(&info.licence_text),
        ));
    }
    elements
}

/// The authors, and what the application is built with.
fn credits_page(info: &AppInfo) -> Vec<Element> {
    let mut elements: Vec<Element> = Vec::new();
    if info.authors.is_empty() {
        elements.push(caption(crate::tr!("ui-about-no-authors", app = info.name.as_str())));
    } else {
        let mut authors = Group::new(Some(crate::tr!("ui-about-written-by")));
        for author in &info.authors {
            authors = authors.row(row(author.clone()));
        }
        elements.push(authors.into());
    }
    let mut built = Group::new(Some(crate::tr!("ui-about-built-with")));
    built = built.row(row("bingux-ui").value(details::BINGUX_UI));
    if !details::FREYA.is_empty() {
        built = built.row(row("Freya").value(details::FREYA));
    }
    if !details::RUSTC.is_empty() {
        built = built.row(row("Rust").value(details::RUSTC.trim_start_matches("rustc ")));
    }
    elements.push(built.into());
    elements
}

/// The libraries linked into the application and their licences.
fn legal_page(tokens: &Tokens, info: &AppInfo, legal: Option<Legal>) -> Vec<Element> {
    let mut elements: Vec<Element> = Vec::new();
    elements.push(
        Group::new(Some(crate::tr!("ui-about-interface-library")))
            .row(row("bingux-ui").value(details::BINGUX_UI))
            .row(row(crate::tr!("ui-about-licence")).value("GPL-3.0-or-later"))
            .into(),
    );
    let Some(legal) = legal else {
        // Nothing is read before the page is open, and the read is quick: the
        // note holds the place until it lands.
        elements.push(caption(crate::tr!(
            "ui-about-no-library-licences",
            app = info.name.as_str()
        )));
        return elements;
    };
    let (summary, crates) = details::dependency_list(&legal.text);
    if !summary.is_empty() {
        elements.push(
            Group::new(Some(crate::tr!("ui-about-libraries")))
                .row(row(crate::tr!("ui-about-licences")).value(summary))
                .into(),
        );
    }
    if crates.is_empty() {
        elements.push(text_card(
            None,
            tokens,
            LISTING,
            &details::paragraphs(&legal.text),
        ));
    } else {
        let blocks: Vec<String> = crates
            .chunks(LISTING_BLOCK)
            .map(|lines| lines.join("\n"))
            .collect();
        elements.push(text_card(
            Some(&crate::tr!("ui-about-licences-each")),
            tokens,
            LISTING,
            &blocks,
        ));
    }
    if let Some(path) = legal.path {
        elements.push(caption(crate::tr!("ui-about-licence-files", path = path.as_str())));
    }
    elements
}

/// Paragraphs of `style` on one card, as a group's content, under `heading`
/// when there is one. The text is selectable (drag, double and triple click,
/// Ctrl+C, Copy in the right-click menu) and is a Tab stop, so a keyboard
/// user can select all of it: a licence is a text people copy from.
fn text_card(
    heading: Option<&str>,
    tokens: &Tokens,
    style: TextStyle,
    paragraphs: &[String],
) -> Element {
    let mut column = rect().tw(tokens, &[flex_col(), w_full(), gap(S3)]);
    for text in paragraphs {
        let block = crate::controls::select_text(text.clone())
            .style(style)
            .tab_stop(true);
        let block = if style.mono {
            block.breaks_anywhere()
        } else {
            block
        };
        column = column.child(Element::from(block));
    }
    Group::new(heading.map(str::to_owned))
        .content(column)
        .into()
}
