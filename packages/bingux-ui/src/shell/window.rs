//! Borderless window chrome: a header bar that drags the window and the
//! minimise, maximise and close controls drawn as quiet circles.

use freya::prelude::*;

use crate::controls::{SpinnerSize, spinner};
use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::{surface_color, use_tokens};
use crate::theme::{HEADER_HEIGHT, WINDOW_CONTROL_BUTTON};

/// Run `callback` with the winit window of the current Freya window.
pub fn with_window(callback: impl FnOnce(&mut freya::winit::window::Window) + 'static) {
    Platform::get().with_window(Platform::window_id(), callback);
}

/// Starts a compositor move when the primary mouse button goes down on a
/// header bar. Controls inside the bar stop propagation first.
pub fn drag_on_pointer_down(event: Event<PointerEventData>) {
    if matches!(event.data(), PointerEventData::Mouse(_)) && event.data().is_primary() {
        with_window(|window| {
            let _ = window.drag_window();
        });
    }
}

/// A header bar: optional leading element, a title that fills the middle,
/// and trailing elements. Dragging the bar moves the window.
#[derive(PartialEq)]
pub struct HeaderBar {
    title: String,
    centered: bool,
    leading: Vec<Element>,
    trailing: Vec<Element>,
    background: Option<Color>,
    title_slot: Option<Element>,
    busy: bool,
}

impl HeaderBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            centered: false,
            leading: Vec::new(),
            trailing: Vec::new(),
            background: None,
            title_slot: None,
            busy: false,
        }
    }

    /// Centre the title, as the sidebar header does.
    pub fn centered(mut self) -> Self {
        self.centered = true;
        self
    }

    pub fn leading(mut self, element: impl Into<Element>) -> Self {
        self.leading.push(element.into());
        self
    }

    pub fn trailing(mut self, element: impl Into<Element>) -> Self {
        self.trailing.push(element.into());
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Show a spinner beside the title while a page is busy.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    /// Draw `element` where the title goes, in place of the title text.
    pub fn title_slot(mut self, element: impl Into<Element>) -> Self {
        self.title_slot = Some(element.into());
        self
    }
}

impl Component for HeaderBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let title = label()
            .text(self.title.clone())
            .width(Size::fill())
            .font_size(TEXT_MD)
            .font_weight(FontWeight::SEMI_BOLD)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .text_align(if self.centered {
                TextAlign::Center
            } else {
                TextAlign::Left
            });
        let text: Element = match &self.title_slot {
            Some(slot) => rect().width(Size::flex(1.0)).child(slot.clone()).into(),
            None => rect().width(Size::flex(1.0)).child(title).into(),
        };
        rect()
            .width(Size::fill())
            .height(Size::px(HEADER_HEIGHT))
            .direction(Direction::Horizontal)
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .padding((0.0, S2, 0.0, if self.leading.is_empty() { S3 } else { S2 }))
            .spacing(S1)
            .background(self.background.unwrap_or(tokens.surface))
            .on_pointer_down(drag_on_pointer_down)
            .children(self.leading.clone())
            .child(text)
            .maybe_child(self.busy.then(busy_indicator))
            .children(self.trailing.clone())
    }
}

/// A small spinner with a "Working" tooltip, shown beside the header title.
fn busy_indicator() -> Element {
    TooltipContainer::new(Tooltip::new_text("Working"))
        .child(spinner(SpinnerSize::Small))
        .into()
}

/// Minimise, maximise/restore and close, drawn as 24 px circles.
#[derive(PartialEq)]
pub struct WindowControls;

impl Component for WindowControls {
    fn render(&self) -> impl IntoElement {
        let maximized = freya::borderless::use_maximized();
        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(S1)
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
                surface_color(&tokens, Color::TRANSPARENT, true, false, false)
            }
        } else {
            Color::TRANSPARENT
        };
        rect()
            .width(Size::px(WINDOW_CONTROL_BUTTON))
            .height(Size::px(WINDOW_CONTROL_BUTTON))
            .corner_radius(WINDOW_CONTROL_BUTTON / 2.0)
            .background(circle)
            .center()
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
            .child(icons::icon(self.icon, ICON_MD, tokens.text))
    }
}
