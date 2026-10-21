//! The window: header bar, navigation sidebar and the page column.
//!
//! The window is borderless; the header bar draws its own controls and
//! drags the window. Below [`NARROW_BREAKPOINT`] the sidebar becomes an
//! overlay opened from a header button.

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::pages::PageId;
use crate::theme::{
    self, SharedTokens, WINDOW_CONTROL_BUTTON, WINDOW_CONTROL_CIRCLE, WINDOW_RADIUS, freya_theme,
};
use crate::ui::{
    HEADER_HEIGHT, NARROW_BREAKPOINT, PAGE_PADDING, PAGE_WIDTH, SIDEBAR_WIDTH, icon_button,
    use_tokens,
};

/// Window root. `initial` is the page named on the command line.
#[derive(PartialEq)]
pub struct App {
    pub initial: PageId,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = use_hook(|| SharedTokens(State::create(theme::load_tokens())));
        use_hook(move || theme::watch_appearance(tokens.0));
        use_provide_root_context(|| tokens);
        let current = *tokens.0.read();
        let mut theme_state = use_init_theme(|| freya_theme(&current));
        use_side_effect(move || {
            let tokens = *tokens.0.read();
            theme_state.set(freya_theme(&tokens));
        });

        let page = use_state(|| self.initial);
        let mut sidebar_open = use_state(|| false);
        let width = Platform::get().current_window().root_size.read().width;
        let narrow = width < NARROW_BREAKPOINT;
        let maximized = freya::borderless::use_maximized();

        rect()
            .expanded()
            .content(Content::Flex)
            .background(current.bg)
            .color(current.text)
            .font_family(theme::FONT_FAMILY)
            .corner_radius(if maximized() { 0.0 } else { WINDOW_RADIUS })
            .overflow(Overflow::Clip)
            .child(HeaderBar {
                page: page(),
                narrow,
                sidebar_open,
            })
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .maybe_child((!narrow).then(|| Sidebar {
                        page,
                        sidebar_open,
                        overlay: false,
                    }))
                    .child(PageColumn { page: page() }),
            )
            .maybe_child((narrow && sidebar_open()).then(|| {
                rect()
                    .position(Position::new_absolute().top(HEADER_HEIGHT).left(0.0))
                    .width(Size::fill())
                    .height(Size::fill())
                    .direction(Direction::Horizontal)
                    .content(Content::Flex)
                    .child(Sidebar {
                        page,
                        sidebar_open,
                        overlay: true,
                    })
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
struct HeaderBar {
    page: PageId,
    narrow: bool,
    sidebar_open: State<bool>,
}

fn with_window(callback: impl FnOnce(&mut freya::winit::window::Window) + 'static) {
    Platform::get().with_window(Platform::window_id(), callback);
}

impl Component for HeaderBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let maximized = freya::borderless::use_maximized();
        let mut sidebar_open = self.sidebar_open;
        let title = if self.narrow {
            self.page.title()
        } else {
            "Settings"
        };

        rect()
            .width(Size::fill())
            .height(Size::px(HEADER_HEIGHT))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .padding((0.0, 8.0))
            .spacing(4.0)
            .background(tokens.surface)
            .border(
                Border::new()
                    .width(BorderWidth {
                        top: 0.0,
                        right: 0.0,
                        bottom: 1.0,
                        left: 0.0,
                    })
                    .alignment(BorderAlignment::Inner)
                    .fill(tokens.bar_divider),
            )
            .on_pointer_down(move |event: Event<PointerEventData>| {
                if matches!(event.data(), PointerEventData::Mouse(_)) {
                    with_window(|window| {
                        let _ = window.drag_window();
                    });
                }
            })
            .maybe_child(self.narrow.then(|| {
                icon_button(Icon::SidebarShow, move |_| {
                    sidebar_open.set(!sidebar_open())
                })
                .size(WINDOW_CONTROL_BUTTON)
                .tooltip("Show pages")
            }))
            .child(
                label()
                    .text(title)
                    .font_size(15.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .width(Size::flex(1.0))
                    .text_align(TextAlign::Center)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
            .child(WindowControl {
                icon: Icon::WindowMinimize,
                tooltip: "Minimise",
                danger: false,
                action: Action::Minimize,
            })
            .child(WindowControl {
                icon: if maximized() {
                    Icon::WindowRestore
                } else {
                    Icon::WindowMaximize
                },
                tooltip: if maximized() { "Restore" } else { "Maximise" },
                danger: false,
                action: Action::ToggleMaximize,
            })
            .child(WindowControl {
                icon: Icon::WindowClose,
                tooltip: "Close",
                danger: true,
                action: Action::Close,
            })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Minimize,
    ToggleMaximize,
    Close,
}

#[derive(PartialEq)]
struct WindowControl {
    icon: Icon,
    tooltip: &'static str,
    danger: bool,
    action: Action,
}

impl Component for WindowControl {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let action = self.action;
        let circle = if hovering() {
            if self.danger {
                tokens.danger
            } else {
                tokens.hover
            }
        } else {
            tokens.elevated
        };
        rect()
            .width(Size::px(WINDOW_CONTROL_BUTTON))
            .height(Size::px(WINDOW_CONTROL_BUTTON))
            .center()
            .cursor(CursorIcon::Pointer)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.tooltip)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                match action {
                    Action::Minimize => with_window(|window| window.set_minimized(true)),
                    Action::ToggleMaximize => {
                        with_window(|window| window.set_maximized(!window.is_maximized()))
                    }
                    Action::Close => Platform::get().close_window(Platform::window_id()),
                }
            })
            .child(
                rect()
                    .width(Size::px(WINDOW_CONTROL_CIRCLE))
                    .height(Size::px(WINDOW_CONTROL_CIRCLE))
                    .corner_radius(WINDOW_CONTROL_CIRCLE / 2.0)
                    .background(circle)
                    .center()
                    .child(icons::icon(self.icon, 12.0, tokens.text)),
            )
    }
}

#[derive(PartialEq)]
struct Sidebar {
    page: State<PageId>,
    sidebar_open: State<bool>,
    overlay: bool,
}

impl Component for Sidebar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let page = self.page;
        let sidebar_open = self.sidebar_open;
        let overlay = self.overlay;
        rect()
            .width(Size::px(SIDEBAR_WIDTH))
            .height(Size::fill())
            .background(if overlay {
                tokens.surface
            } else {
                Color::TRANSPARENT
            })
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
            .child(
                ScrollView::new().child(
                    rect()
                        .width(Size::fill())
                        .padding(8.0)
                        .spacing(2.0)
                        .children(PageId::ALL.into_iter().map(|id| {
                            Element::from(NavEntry {
                                id,
                                selected: page() == id,
                                page,
                                sidebar_open,
                                overlay,
                            })
                        })),
                ),
            )
    }
}

#[derive(PartialEq)]
struct NavEntry {
    id: PageId,
    selected: bool,
    page: State<PageId>,
    sidebar_open: State<bool>,
    overlay: bool,
}

impl Component for NavEntry {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let id = self.id;
        let mut page = self.page;
        let mut sidebar_open = self.sidebar_open;
        let overlay = self.overlay;
        let background = if self.selected {
            tokens.selection
        } else if hovering() {
            tokens.hover
        } else {
            Color::TRANSPARENT
        };
        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(2.0)
                .alignment(BorderAlignment::Inner)
                .fill(tokens.accent)
        } else {
            Border::new()
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(move |builder| builder.set_selected(id == page()))
            .width(Size::fill())
            .height(Size::px(38.0))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(12.0)
            .padding((0.0, 12.0))
            .corner_radius(8.0)
            .background(background)
            .border(border)
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |_: Event<PressEventData>| {
                page.set(id);
                if overlay {
                    sidebar_open.set(false);
                }
                a11y_id.request_focus();
            })
            .child(icons::icon(
                id.icon(),
                16.0,
                if self.selected {
                    tokens.text
                } else {
                    tokens.muted
                },
            ))
            .child(
                label()
                    .text(id.title())
                    .font_size(14.0)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}

#[derive(PartialEq)]
struct PageColumn {
    page: PageId,
}

impl Component for PageColumn {
    fn render(&self) -> impl IntoElement {
        rect().width(Size::flex(1.0)).height(Size::fill()).child(
            ScrollView::new().child(
                rect()
                    .width(Size::fill())
                    .cross_align(Alignment::Center)
                    .padding(PAGE_PADDING)
                    .child(
                        rect()
                            .width(Size::px(PAGE_WIDTH))
                            .max_width(Size::fill())
                            .child(self.page.render()),
                    ),
            ),
        )
    }
}
