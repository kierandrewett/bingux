//! The window shell: [`Screen`]. An app fills its slots; the screen lays the
//! window out and paints its tiers (`design/boundaries.md`):
//!
//! * the strip of document tabs on `Paint::Strip`;
//! * the chrome (header, sidebar, aside, footer) on `Paint::Sidebar`, one
//!   continuous tone;
//! * the content on `Paint::Bg`, with one hairline where it meets the chrome.
//!
//! An immersive screen draws the content under the whole window and floats
//! the chrome over it as glass. On a phone the header's switcher takes a row
//! of its own, and the sidebar and the aside become screens of their own.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::carousel::PageDots;
use crate::focus::{RegionKind, use_region};
use crate::shell::boundary::{
    BottomBar, BottomBarHost, ChromeItem, ContentItem, Region, ViewSwitcher, within,
};
use crate::shell::window::HeaderBar;
use crate::style::*;
use crate::surface::use_tokens;

/// How much the content behind glass chrome is blurred.
const GLASS_BLUR: f32 = 20.0;

/// Which pane a phone shows. A phone has room for one at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    /// The content (the default).
    #[default]
    Content,
    /// The sidebar, as a screen of its own: a list of places to go.
    Sidebar,
    /// The aside, as a screen of its own: the history, an inspector.
    Aside,
}

/// A window: a header over the content, with the optional strip, sidebar,
/// aside, footer and pager. Build with [`Screen::new`].
///
/// ```
/// use bingux_ui::prelude::*;
/// use freya::prelude::*;
/// let window = Screen::new(HeaderBar::new("Weather").trailing(WindowControls))
///     .sidebar(rect())
///     .content(rect());
/// ```
#[derive(PartialEq)]
pub struct Screen {
    header: HeaderBar,
    strip: Option<HeaderBar>,
    sidebar: Option<Element>,
    sidebar_width: Dim,
    content: Option<Element>,
    content_bar: Option<Element>,
    aside: Option<Element>,
    footer: Option<Element>,
    pager: Option<Element>,
    immersive: bool,
    compact_pane: Pane,
}

impl Screen {
    /// A window under `header`: the bar that holds the title or the view
    /// switcher, the navigation and the actions. The screen paints it in the
    /// chrome tone.
    pub fn new(header: HeaderBar) -> Self {
        Self {
            header,
            strip: None,
            sidebar: None,
            sidebar_width: Dim::Sidebar,
            content: None,
            content_bar: None,
            aside: None,
            footer: None,
            pager: None,
            immersive: false,
            compact_pane: Pane::Content,
        }
    }

    /// The strip of document tabs above the header: a bar built with
    /// [`HeaderBar::tab_slot`] that also holds the window buttons. It is
    /// outside the browsing frame, so it has its own tone.
    pub fn strip(mut self, strip: HeaderBar) -> Self {
        self.strip = Some(strip);
        self
    }

    /// The leading pane: the places to go (folders, pages, saved places) and
    /// the controls over that list. Chrome tone, continuous with the header.
    pub fn sidebar(mut self, sidebar: impl ContentItem) -> Self {
        self.sidebar = Some(sidebar.into_content());
        self
    }

    /// How wide the sidebar and the aside are: [`Dim::Sidebar`] unless a list
    /// of short names wants less (Files' places use [`Dim::PlacesColumn`]).
    pub fn sidebar_width(mut self, width: Dim) -> Self {
        self.sidebar_width = width;
        self
    }

    /// What the window is for: the document, the list, the calculation. A
    /// control that chooses what shows here is not content: put it in the
    /// header.
    pub fn content(mut self, content: impl ContentItem) -> Self {
        self.content = Some(content.into_content());
        self
    }

    /// A bar on the content's top edge that acts on the content: find and
    /// replace, go to line. It is inside the boundary, in the content tone.
    pub fn content_bar(mut self, bar: impl ContentItem) -> Self {
        self.content_bar = Some(bar.into_content());
        self
    }

    /// The trailing pane: the history, an inspector, details of the
    /// selection. Chrome tone.
    pub fn aside(mut self, aside: impl ContentItem) -> Self {
        self.aside = Some(aside.into_content());
        self
    }

    /// A bar that belongs to the app rather than to one view: what is playing,
    /// the status of the document. It runs the full width of the window,
    /// under the sidebar and the content, with one hairline above it.
    pub fn footer(mut self, footer: impl ContentItem) -> Self {
        self.footer = Some(footer.into_content());
        self
    }

    /// The page dots of a paged content, over the content's bottom edge.
    pub fn pager(mut self, dots: PageDots) -> Self {
        self.pager = Some(dots.into_chrome());
        self
    }

    /// Draw the content under the whole window (a sky, a film, a map) and
    /// float the chrome over it as glass. With reduce transparency the glass
    /// is the opaque chrome tone.
    pub fn immersive(mut self, immersive: bool) -> Self {
        self.immersive = immersive;
        self
    }

    /// The pane a phone shows. A desktop window shows them all.
    pub fn compact_pane(mut self, pane: Pane) -> Self {
        self.compact_pane = pane;
        self
    }
}

/// A full-width hairline.
fn rule(tokens: &crate::theme::Tokens) -> Rect {
    rect().tw(tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)])
}

/// The pager's dots on a pill of glass, centred on the bottom edge.
fn pager_row(tokens: &crate::theme::Tokens, dots: Element) -> Rect {
    rect()
        .position(Position::new_absolute().bottom(S3.px()).left(0.0))
        .layer(Layer::Relative(crate::layers::HEADER))
        .width(Size::fill())
        .tw(tokens, &[flex_row(), justify(Align::Center)])
        .child(
            rect()
                .backdrop_blur(GLASS_BLUR)
                .tw(tokens, &[rounded(Radius::Pill), bg(Paint::Frosted)])
                .child(dots),
        )
}

impl Component for Screen {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let header_region = use_region(RegionKind::Header);
        let content_region = use_region(RegionKind::Content);
        let sidebar_region = use_region(RegionKind::Sidebar);
        // How tall the glass over an immersive content is, for the content to
        // start its words below it ([`use_chrome_inset`]).
        let mut inset = use_state(|| 0.0_f32);
        use_provide_context(move || ChromeInset(inset));
        // On a phone the screen draws the header's switcher as the bottom bar
        // when it can (two to five views, each with an icon). From the window's
        // bottom edge up: the bottom bar, the footer, then the content.
        use_provide_context(|| BottomBarHost);
        let bottom_bar = self
            .header
            .view_switcher()
            .filter(|switcher| compact && switcher.fits_bottom_bar())
            .map(|switcher: &ViewSwitcher| {
                rect()
                    .width(Size::fill())
                    .tw(&tokens, &[flex_col(), bg(Paint::Sidebar)])
                    .child(rule(&tokens))
                    .child(within(
                        Region::Chrome,
                        BottomBar {
                            switcher: switcher.clone(),
                        },
                    ))
            });

        let header_fill = if self.immersive {
            Paint::Frosted
        } else {
            Paint::Sidebar
        };
        let header = {
            let press = header_region.clone();
            rect()
                .width(Size::fill())
                .tw(&tokens, &[flex_col()])
                .on_press(move |event: Event<PressEventData>| press.press(&event))
                .child(header_region.sentinel())
                .maybe_child(
                    self.strip
                        .as_ref()
                        .map(|strip| within(Region::Chrome, strip.clone().fill(Paint::Strip))),
                )
                .child(within(
                    Region::Chrome,
                    self.header.clone().fill(header_fill),
                ))
        };
        let content = {
            let press = content_region.clone();
            rect()
                .tw(&tokens, &[w_full(), h_flex(), flex_col(), grow_children()])
                .on_press(move |event: Event<PressEventData>| press.press(&event))
                .child(content_region.sentinel())
                .maybe_child(self.content_bar.clone())
                .child(
                    rect()
                        .tw(&tokens, &[w_full(), h_flex()])
                        .maybe_child(self.content.clone().map(|c| within(Region::Content, c))),
                )
        };
        let pane = |child: &Element, fill: Paint| -> Element {
            let press = sidebar_region.clone();
            rect()
                .tw(
                    &tokens,
                    &[w(self.sidebar_width), h_full(), flex_col(), bg(fill)],
                )
                .on_press(move |event: Event<PressEventData>| press.press(&event))
                .child(sidebar_region.sentinel())
                .child(within(Region::Chrome, child.clone()))
                .into()
        };
        let footer = self.footer.clone().map(|footer| {
            rect()
                .component("ScreenFooter")
                .width(Size::fill())
                .tw(&tokens, &[flex_col(), bg(header_fill)])
                .maybe(self.immersive, |bar| bar.backdrop_blur(GLASS_BLUR))
                .child(rule(&tokens))
                .child(within(Region::Chrome, footer))
        });

        // A phone that shows the sidebar or the aside as a screen of its own
        // draws it opaque, as any list.
        if self.immersive && !(compact && self.compact_pane != Pane::Content) {
            // The content runs under the whole header; the chrome is glass
            // over it. A desktop sidebar is a glass pane beside the content.
            let top = rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .layer(Layer::Relative(crate::layers::HEADER))
                .width(Size::fill())
                .backdrop_blur(GLASS_BLUR)
                .tw(&tokens, &[flex_col()])
                .on_sized(move |event: Event<SizedEventData>| {
                    inset.set_if_modified(event.area.height());
                })
                .child(header)
                .child(rule(&tokens));
            let sidebar = self.sidebar.as_ref().filter(|_| !compact).map(|sidebar| {
                let press = sidebar_region.clone();
                rect()
                    .tw(
                        &tokens,
                        &[
                            w(self.sidebar_width),
                            h_full(),
                            flex_col(),
                            bg(Paint::Frosted),
                        ],
                    )
                    .on_press(move |event: Event<PressEventData>| press.press(&event))
                    .child(sidebar_region.sentinel())
                    .child(rect().height(Size::px(inset())))
                    .child(within(Region::Chrome, sidebar.clone()))
            });
            let has_sidebar = sidebar.is_some();
            return rect()
                .component("Screen")
                .expanded()
                .tw(&tokens, &[flex_col(), grow_children(), bg(Paint::Bg)])
                .child(
                    rect()
                        .tw(&tokens, &[w_full(), h_flex(), flex_row(), grow_children()])
                        .maybe_child(sidebar)
                        .maybe_child(has_sidebar.then(|| rect().tw(&tokens, SPLIT_HAIRLINE)))
                        .child(
                            rect()
                                .tw(&tokens, &[w_flex(), h_full(), flex_col(), grow_children()])
                                .child(content)
                                .maybe_child(
                                    self.pager.clone().map(|dots| pager_row(&tokens, dots)),
                                ),
                        ),
                )
                .child(top)
                .maybe_child(footer)
                .maybe_child(bottom_bar);
        }

        // The content column: one hairline along its top, where it meets the
        // header, and the pager over its bottom edge.
        let column = |pane_content: Rect| -> Element {
            rect()
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
                .child(rule(&tokens))
                .child(pane_content)
                .maybe_child(self.pager.clone().map(|dots| pager_row(&tokens, dots)))
                .into()
        };
        let body: Element = if compact {
            // A phone shows one pane; a pane shown as a screen of its own is
            // that screen's content.
            let shown = match self.compact_pane {
                Pane::Sidebar => self.sidebar.clone(),
                Pane::Aside => self.aside.clone(),
                Pane::Content => None,
            };
            match shown {
                Some(pane) => rect()
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
                    .child(rule(&tokens))
                    .child(within(Region::Chrome, pane))
                    .into(),
                None => rect()
                    .tw(&tokens, &[w_full(), h_flex(), flex_row(), grow_children()])
                    .child(column(content))
                    .into(),
            }
        } else {
            rect()
                .tw(&tokens, &[w_full(), h_flex(), flex_row(), grow_children()])
                .maybe_child(
                    self.sidebar
                        .as_ref()
                        .map(|sidebar| pane(sidebar, Paint::Sidebar)),
                )
                .maybe_child(
                    self.sidebar
                        .is_some()
                        .then(|| rect().tw(&tokens, SPLIT_HAIRLINE)),
                )
                .child(column(content))
                .maybe_child(
                    self.aside
                        .is_some()
                        .then(|| rect().tw(&tokens, SPLIT_HAIRLINE)),
                )
                .maybe_child(self.aside.as_ref().map(|aside| pane(aside, Paint::Sidebar)))
                .into()
        };
        rect()
            .component("Screen")
            .expanded()
            .tw(&tokens, &[flex_col(), grow_children(), bg(Paint::Sidebar)])
            .child(header)
            .child(body)
            .maybe_child(footer)
            .maybe_child(bottom_bar)
    }
}

/// The height of the glass over an immersive screen's content.
#[derive(Clone, Copy, PartialEq)]
struct ChromeInset(State<f32>);

/// How far an immersive content's words must start from its top to clear
/// the glass header ([`Screen::immersive`]). Its picture runs under the glass;
/// its text and controls start this far down. Zero outside an immersive
/// screen.
pub fn use_chrome_inset() -> f32 {
    try_consume_context::<ChromeInset>().map_or(0.0, |inset| *inset.0.read())
}
