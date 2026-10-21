//! The navigation split view: a sidebar with a search field and a flat list
//! of destinations beside the page column, each with its own header bar,
//! like GNOME Settings. Destination icons are monochrome and the selected
//! destination has a neutral fill; colour is reserved for the accent in
//! controls.
//!
//! Below [`SIDEBAR_OVERLAY_BELOW`] the sidebar becomes an overlay opened
//! from the content header.

use freya::animation::*;
use freya::prelude::*;

use crate::controls::{icon_button, search_field};
use crate::icons::{self, Icon};
use crate::scale::*;
use crate::shell::window::{HeaderBar, WindowControls};
use crate::surface::{focus_border, is_activation_key, use_tokens};
use crate::theme::{
    HEADER_HEIGHT, SIDEBAR_OVERLAY_BELOW, SIDEBAR_WIDTH, Tokens, WINDOW_CONTROL_BUTTON,
};

/// One sidebar destination.
pub trait NavItem: Copy + PartialEq + 'static {
    fn title(self) -> &'static str;
    fn icon(self) -> Icon;
    /// The sidebar section this item is listed under.
    fn group(self) -> &'static str {
        ""
    }
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
            page_width: 640.0,
        }
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
        let side = if narrow { S4 } else { S6 };
        let selected = self.selected;
        // The overlay sidebar slides in over 160 ms while the scrim fades.
        let motion = tokens.motion(160);
        let overlay_anim = use_animation_transition(
            if narrow && sidebar_open() { 1.0 } else { 0.0 },
            move |from, to| {
                AnimNum::new(from, to)
                    .time(motion)
                    .function(Function::Expo)
                    .ease(Ease::Out)
            },
        );
        let overlay_progress = overlay_anim.get().value();
        let sidebar = |overlay: bool| Sidebar {
            app_title: self.app_title.clone(),
            items: self.items.clone(),
            selected,
            query,
            sidebar_open,
            overlay,
        };
        let header = HeaderBar::new("")
            .background(tokens.bg)
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
            .background(tokens.bg)
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
                                    .padding((S2, side, S6, side))
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
            .maybe_child(
                (narrow && (sidebar_open() || overlay_progress > 0.0)).then(|| {
                    rect()
                        .position(Position::new_absolute().top(0.0).left(0.0))
                        .width(Size::fill())
                        .height(Size::fill())
                        .direction(Direction::Horizontal)
                        .content(Content::Flex)
                        .layer(Layer::Overlay)
                        .child(
                            rect()
                                .width(Size::px(SIDEBAR_WIDTH))
                                .height(Size::fill())
                                .offset_x(-SIDEBAR_WIDTH * (1.0 - overlay_progress))
                                .child(sidebar(true)),
                        )
                        .child(
                            rect()
                                .width(Size::flex(1.0))
                                .height(Size::fill())
                                .background(Color::BLACK.with_af32(0.4 * overlay_progress))
                                .on_press(move |_: Event<PressEventData>| sidebar_open.set(false)),
                        )
                }),
            )
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
        let mut entries: Vec<Element> = Vec::new();
        let mut last_group = "";
        for item in &visible {
            let group = item.group();
            if group != last_group && !group.is_empty() {
                entries.push(
                    label()
                        .text(group)
                        .color(tokens.muted)
                        .font_size(TEXT_SM)
                        .margin((if entries.is_empty() { S1 } else { S4 }, 0.0, S1, S3))
                        .into(),
                );
            }
            last_group = group;
            entries.push(Element::from(NavEntry {
                item: *item,
                siblings: visible.clone(),
                selected,
                sidebar_open,
                overlay,
            }));
        }

        let close = overlay.then(|| {
            icon_button(Icon::SidebarShow, move |_| {
                let mut sidebar_open = sidebar_open;
                sidebar_open.set(false)
            })
            .size(WINDOW_CONTROL_BUTTON)
            .tooltip("Hide pages")
        });
        let field_width = SIDEBAR_WIDTH
            - 2.0 * S2
            - if overlay {
                WINDOW_CONTROL_BUTTON + S2
            } else {
                0.0
            };
        rect()
            .width(Size::px(SIDEBAR_WIDTH))
            .height(Size::fill())
            .background(tokens.sidebar)
            .border(
                Border::new()
                    .width(BorderWidth {
                        top: 0.0,
                        right: 1.0,
                        bottom: 0.0,
                        left: 0.0,
                    })
                    .alignment(BorderAlignment::Inner)
                    .fill(tokens.separator),
            )
            .maybe(overlay, |sidebar| {
                sidebar.shadow(
                    Shadow::new()
                        .position(ShadowPosition::Normal)
                        .y(8.0)
                        .blur(24.0)
                        .color(Color::BLACK.with_af32(0.45)),
                )
            })
            .child(search_band(query, field_width, close))
            .child(
                rect().width(Size::fill()).height(Size::flex(1.0)).child(
                    ScrollView::new().child(
                        rect()
                            .width(Size::fill())
                            .padding((S2, S2, S4, S2))
                            .spacing(S0)
                            .children(entries)
                            .maybe_child(visible.is_empty().then(|| {
                                label()
                                    .text("Nothing matches.")
                                    .color(tokens.muted)
                                    .font_size(TEXT_MD)
                                    .margin((S3, S2, 0.0, S2))
                            })),
                    ),
                ),
            )
    }
}

/// The sidebar search field: a rounded well with a search glyph, on the top band.
#[derive(PartialEq)]
pub struct SearchField {
    pub query: State<String>,
}

impl Component for SearchField {
    fn render(&self) -> impl IntoElement {
        search_band(self.query, SIDEBAR_WIDTH - 2.0 * S2, None::<Element>)
    }
}

/// The top band of the sidebar. It is HEADER_HEIGHT tall, like the page
/// header, so the search field sits on the window-control line. The overlay
/// puts its close button to the right of the field.
fn search_band(
    query: State<String>,
    field_width: f32,
    close: Option<impl IntoElement>,
) -> impl IntoElement {
    rect()
        .width(Size::fill())
        .height(Size::px(HEADER_HEIGHT))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .padding((0.0, S2))
        .spacing(S2)
        .child(search_field(query, |_| {}).width(field_width))
        .maybe_child(close)
}

#[derive(PartialEq)]
struct NavEntry<T: NavItem> {
    item: T,
    /// The items currently listed, for Up and Down.
    siblings: Vec<T>,
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
        let siblings = self.siblings.clone();
        let mut selected = self.selected;
        let mut sidebar_open = self.sidebar_open;
        let overlay = self.overlay;
        let is_selected = selected() == item;
        let background = item_fill(&tokens, hovering(), pressing(), is_selected);
        let border = focus_border(&tokens, focus());
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(move |builder| builder.set_selected(is_selected))
            .width(Size::fill())
            .height(Size::px(CONTROL_HEIGHT))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(S3)
            .padding((0.0, S3))
            .corner_radius(RADIUS)
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
            .on_key_down(move |event: Event<KeyboardEventData>| {
                // Walk from the selected item, not this entry: focus stays
                // here while repeated arrows move the selection on.
                let position = siblings.iter().position(|other| *other == selected());
                let target = match &event.key {
                    key if is_activation_key(key) => Some(item),
                    Key::Named(NamedKey::ArrowDown) => {
                        position.and_then(|index| siblings.get(index + 1)).copied()
                    }
                    Key::Named(NamedKey::ArrowUp) => position
                        .and_then(|index| index.checked_sub(1))
                        .and_then(|index| siblings.get(index))
                        .copied(),
                    Key::Named(NamedKey::Home) => siblings.first().copied(),
                    Key::Named(NamedKey::End) => siblings.last().copied(),
                    _ => None,
                };
                if let Some(target) = target {
                    event.stop_propagation();
                    selected.set(target);
                }
            })
            .child(icons::icon(item.icon(), ICON_LG, tokens.text))
            .child(
                label()
                    .text(item.title())
                    .color(tokens.text)
                    .font_size(TEXT_MD)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}

/// Background of a sidebar item. The selected item has a fill of its own
/// that gets stronger on hover and press, so it never looks weaker than an
/// idle item.
fn item_fill(tokens: &Tokens, hovering: bool, pressing: bool, selected: bool) -> Color {
    match (selected, pressing, hovering) {
        (true, true, _) | (true, false, true) => tokens.elevated_pressed,
        (true, false, false) => tokens.selection,
        (false, true, _) => tokens.pressed,
        (false, false, true) => tokens.hover,
        (false, false, false) => Color::TRANSPARENT,
    }
}
