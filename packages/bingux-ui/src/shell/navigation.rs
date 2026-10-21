//! The navigation split view: a dark sidebar with a search field, an
//! optional profile card and a list of destinations with coloured icon
//! tiles, beside a lighter content column with its own header bar.
//!
//! The sidebar follows macOS System Settings; the content column follows
//! GNOME. Below [`SIDEBAR_OVERLAY_BELOW`] the sidebar becomes an overlay
//! opened from the content header.

use freya::prelude::*;

use crate::controls::icon_button;
use crate::icons::{self, Icon, icon_tile};
use crate::shell::window::{HeaderBar, WindowControls};
use crate::surface::use_tokens;
use crate::theme::{SIDEBAR_OVERLAY_BELOW, SIDEBAR_WIDTH, WINDOW_CONTROL_BUTTON};

/// One sidebar destination.
pub trait NavItem: Copy + PartialEq + 'static {
    fn title(self) -> &'static str;
    fn icon(self) -> Icon;
    /// Base colour of the icon tile.
    fn tile_rgb(self) -> [f32; 3];
    /// Extra words the search field matches, space separated.
    fn keywords(self) -> &'static str {
        ""
    }
}

/// The split view. `content` renders the selected item's page.
#[derive(PartialEq)]
pub struct NavigationSplitView<T: NavItem> {
    pub app_title: String,
    pub items: Vec<T>,
    pub selected: State<T>,
    pub content: Element,
    pub profile: Option<ProfileCard<T>>,
    pub page_width: f32,
}

impl<T: NavItem> NavigationSplitView<T> {
    pub fn new(
        app_title: impl Into<String>,
        items: Vec<T>,
        selected: State<T>,
        content: impl Into<Element>,
    ) -> Self {
        Self {
            app_title: app_title.into(),
            items,
            selected,
            content: content.into(),
            profile: None,
            page_width: 640.0,
        }
    }

    pub fn profile(mut self, card: ProfileCard<T>) -> Self {
        self.profile = Some(card);
        self
    }

    pub fn page_width(mut self, width: f32) -> Self {
        self.page_width = width;
        self
    }
}

impl<T: NavItem> Component for NavigationSplitView<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let query = use_state(String::new);
        let mut sidebar_open = use_state(|| false);
        let width = Platform::get().current_window().root_size.read().width;
        let narrow = width < SIDEBAR_OVERLAY_BELOW;
        let selected = self.selected;
        let sidebar = |overlay: bool| Sidebar {
            app_title: self.app_title.clone(),
            items: self.items.clone(),
            selected,
            query,
            sidebar_open,
            overlay,
            profile: self.profile.clone(),
        };
        let header = HeaderBar::new(selected().title())
            .maybe(narrow, |bar| {
                bar.leading(
                    icon_button(Icon::SidebarShow, move |_| {
                        sidebar_open.set(!sidebar_open())
                    })
                    .size(WINDOW_CONTROL_BUTTON)
                    .tooltip("Show pages"),
                )
            })
            .trailing(WindowControls);

        rect()
            .expanded()
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .background(tokens.surface)
            .maybe_child((!narrow).then(|| sidebar(false)))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .child(header)
                    .child(
                        rect().width(Size::fill()).height(Size::flex(1.0)).child(
                            ScrollView::new().child(
                                rect()
                                    .width(Size::fill())
                                    .cross_align(Alignment::Center)
                                    .padding((8.0, 24.0, 24.0, 24.0))
                                    .child(
                                        rect()
                                            .width(Size::px(self.page_width))
                                            .max_width(Size::fill())
                                            .child(self.content.clone()),
                                    ),
                            ),
                        ),
                    ),
            )
            .maybe_child((narrow && sidebar_open()).then(|| {
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::fill())
                    .height(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .child(sidebar(true))
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .height(Size::fill())
                            .background(Color::from_af32rgb(0.4, 0, 0, 0))
                            .on_press(move |_: Event<PressEventData>| sidebar_open.set(false)),
                    )
            }))
    }
}

#[derive(PartialEq)]
struct Sidebar<T: NavItem> {
    app_title: String,
    items: Vec<T>,
    selected: State<T>,
    query: State<String>,
    sidebar_open: State<bool>,
    overlay: bool,
    profile: Option<ProfileCard<T>>,
}

impl<T: NavItem> Component for Sidebar<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let selected = self.selected;
        let query = self.query;
        let sidebar_open = self.sidebar_open;
        let overlay = self.overlay;
        let needle = query.read().trim().to_lowercase();
        let visible: Vec<T> = self
            .items
            .iter()
            .copied()
            .filter(|item| {
                needle.is_empty()
                    || item.title().to_lowercase().contains(&needle)
                    || item
                        .keywords()
                        .split_whitespace()
                        .any(|word| word.starts_with(&needle))
            })
            .collect();
        let header = HeaderBar::new(self.app_title.clone())
            .centered()
            .background(tokens.bg)
            .leading(rect().width(Size::px(WINDOW_CONTROL_BUTTON)))
            .trailing(if overlay {
                Element::from(
                    icon_button(Icon::SidebarShow, move |_| {
                        let mut sidebar_open = sidebar_open;
                        sidebar_open.set(false)
                    })
                    .size(WINDOW_CONTROL_BUTTON)
                    .tooltip("Hide pages"),
                )
            } else {
                Element::from(rect().width(Size::px(WINDOW_CONTROL_BUTTON)))
            });

        rect()
            .width(Size::px(SIDEBAR_WIDTH))
            .height(Size::fill())
            .background(tokens.bg)
            .border(
                Border::new()
                    .width(BorderWidth {
                        top: 0.0,
                        right: 1.0,
                        bottom: 0.0,
                        left: 0.0,
                    })
                    .alignment(BorderAlignment::Inner)
                    .fill(tokens.bar_divider),
            )
            .child(header)
            .child(SearchField { query })
            .maybe_child(self.profile.clone().map(|card| ProfileCardView {
                card,
                selected,
                sidebar_open,
                overlay,
            }))
            .child(
                rect().width(Size::fill()).height(Size::flex(1.0)).child(
                    ScrollView::new().child(
                        rect()
                            .width(Size::fill())
                            .padding((4.0, 10.0, 12.0, 10.0))
                            .spacing(1.0)
                            .children(visible.iter().map(|item| {
                                Element::from(NavEntry {
                                    item: *item,
                                    selected,
                                    sidebar_open,
                                    overlay,
                                })
                            }))
                            .maybe_child(visible.is_empty().then(|| {
                                label()
                                    .text("Nothing matches.")
                                    .color(tokens.muted)
                                    .font_size(13.0)
                                    .margin((12.0, 10.0, 0.0, 10.0))
                            })),
                    ),
                ),
            )
    }
}

/// The sidebar search field: a rounded well with a search glyph.
#[derive(PartialEq)]
pub struct SearchField {
    pub query: State<String>,
}

impl Component for SearchField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .width(Size::fill())
            .padding((2.0, 12.0, 10.0, 12.0))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(32.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .spacing(6.0)
                    .padding((0.0, 10.0))
                    .corner_radius(9.0)
                    .background(tokens.elevated)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(tokens.outline),
                    )
                    .child(icons::icon(Icon::Search, 14.0, tokens.muted))
                    .child(
                        rect().width(Size::flex(1.0)).child(
                            Input::new(self.query)
                                .flat()
                                .compact()
                                .placeholder("Search")
                                .width(Size::fill()),
                        ),
                    ),
            )
    }
}

/// A card at the top of the sidebar with an initials avatar, a name and a
/// subtitle; pressing it selects `target`.
#[derive(Clone, PartialEq)]
pub struct ProfileCard<T: NavItem> {
    pub name: String,
    pub subtitle: String,
    pub target: T,
}

#[derive(PartialEq)]
struct ProfileCardView<T: NavItem> {
    card: ProfileCard<T>,
    selected: State<T>,
    sidebar_open: State<bool>,
    overlay: bool,
}

/// Up to two initials for an avatar, from the first and last word.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let letters: String = match words.as_slice() {
        [] => String::new(),
        [only] => only.chars().take(1).collect(),
        [first, .., last] => first.chars().take(1).chain(last.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

impl<T: NavItem> Component for ProfileCardView<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut selected = self.selected;
        let mut sidebar_open = self.sidebar_open;
        let overlay = self.overlay;
        let target = self.card.target;
        let is_selected = selected() == target;
        let background = if hovering() {
            tokens.hover.with_af32(0.42)
        } else if is_selected {
            tokens.selection.with_af32(0.44)
        } else {
            Color::TRANSPARENT
        };
        rect()
            .width(Size::fill())
            .padding((0.0, 10.0, 8.0, 10.0))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(60.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .spacing(12.0)
                    .padding((0.0, 10.0))
                    .corner_radius(10.0)
                    .background(background)
                    .cursor(CursorIcon::Pointer)
                    .a11y_role(AccessibilityRole::Button)
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| hovering.set(false))
                    .on_press(move |_: Event<PressEventData>| {
                        selected.set(target);
                        if overlay {
                            sidebar_open.set(false);
                        }
                    })
                    .child(
                        rect()
                            .width(Size::px(40.0))
                            .height(Size::px(40.0))
                            .corner_radius(20.0)
                            .background(tokens.accent)
                            .center()
                            .child(
                                label()
                                    .text(initials(&self.card.name))
                                    .color(tokens.accent_foreground)
                                    .font_size(15.0)
                                    .font_weight(FontWeight::SEMI_BOLD),
                            ),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .spacing(1.0)
                            .child(
                                label()
                                    .text(self.card.name.clone())
                                    .font_size(14.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .max_lines(1)
                                    .text_overflow(TextOverflow::Ellipsis),
                            )
                            .child(
                                label()
                                    .text(self.card.subtitle.clone())
                                    .color(tokens.muted)
                                    .font_size(12.0)
                                    .max_lines(1),
                            ),
                    )
                    .child(icons::icon(Icon::GoNext, 12.0, tokens.muted)),
            )
    }
}

#[derive(PartialEq)]
struct NavEntry<T: NavItem> {
    item: T,
    selected: State<T>,
    sidebar_open: State<bool>,
    overlay: bool,
}

impl<T: NavItem> Component for NavEntry<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let item = self.item;
        let mut selected = self.selected;
        let mut sidebar_open = self.sidebar_open;
        let overlay = self.overlay;
        let is_selected = selected() == item;
        let background = if is_selected {
            if pressing() {
                Color::lerp(tokens.accent, Color::BLACK, 0.16)
            } else {
                tokens.accent
            }
        } else if pressing() {
            tokens.pressed.with_af32(0.72)
        } else if hovering() {
            tokens.hover.with_af32(0.42)
        } else {
            Color::TRANSPARENT
        };
        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(tokens.text)
        } else {
            Border::new()
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(move |builder| builder.set_selected(is_selected))
            .width(Size::fill())
            .height(Size::px(36.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(10.0)
            .padding((0.0, 8.0))
            .corner_radius(8.0)
            .background(background)
            .border(border)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |_: Event<PointerEventData>| pressing.set(true))
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| {
                selected.set(item);
                if overlay {
                    sidebar_open.set(false);
                }
                a11y_id.request_focus();
            })
            .child(icon_tile(item.icon(), item.tile_rgb(), 24.0))
            .child(
                label()
                    .text(item.title())
                    .color(if is_selected {
                        tokens.accent_foreground
                    } else {
                        tokens.text
                    })
                    .font_size(14.0)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}
