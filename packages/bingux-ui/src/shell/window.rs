//! Borderless window chrome: a header bar that drags the window and the
//! minimise, maximise and close controls drawn as quiet circles.

use freya::prelude::*;

use crate::controls::{SpinnerSize, hover_tip, spinner};
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::typography::EMPHASIS;

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

/// A header bar: optional leading elements, a title centred across the whole
/// bar, and trailing elements. Dragging the bar moves the window.
#[derive(PartialEq)]
pub struct HeaderBar {
    title: String,
    leading: Vec<Element>,
    trailing: Vec<Element>,
    background: Paint,
    title_slot: Option<Element>,
    busy: bool,
}

impl HeaderBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            leading: Vec::new(),
            trailing: Vec::new(),
            background: Paint::Surface,
            title_slot: None,
            busy: false,
        }
    }

    pub fn leading(mut self, element: impl Into<Element>) -> Self {
        self.leading.push(element.into());
        self
    }

    pub fn trailing(mut self, element: impl Into<Element>) -> Self {
        self.trailing.push(element.into());
        self
    }

    /// The bar's fill; the surface colour by default.
    pub fn fill(mut self, paint: Paint) -> Self {
        self.background = paint;
        self
    }

    /// Show a spinner beside the leading controls while a page is busy.
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
        // The title is laid out on the whole bar, so its centre is the bar's
        // centre whatever the controls weigh. Its side insets are the wider of
        // the two control clusters, so it never runs under a control.
        let mut leading_width = use_state(|| 0.0_f32);
        let mut trailing_width = use_state(|| 0.0_f32);
        let mut title_width = use_state(|| 0.0_f32);
        let inset = leading_width().max(trailing_width()) + S2.px() + S1.px();
        let text: Element = match &self.title_slot {
            Some(slot) => slot.clone(),
            None => EMPHASIS
                .label(&tokens, self.title.clone())
                .width(Size::fill())
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis)
                .text_align(TextAlign::Center)
                .into(),
        };
        // Drawn first, so the controls above it take the pointer where they overlap.
        // The title is inset on both sides by the wider cluster so it never runs under a control.
        // The measured title width less both insets is the title's run-time width.
        let centre_width = (title_width() - inset * 2.0).max(0.0);
        let title = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .tw(&tokens, &[w_full(), h_full()])
            .on_sized(move |event: Event<SizedEventData>| {
                title_width.set_if_modified(event.area.width());
            })
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(inset))
                    .width(Size::px(centre_width))
                    // Freya does not size an absolute box from top and bottom,
                    // so the row takes the bar's full height to centre the title.
                    .tw(&tokens, &[h_full(), flex_row(), items(Align::Center)])
                    .child(text),
            );
        let leading = rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S1)])
            .on_sized(move |event: Event<SizedEventData>| {
                leading_width.set_if_modified(event.area.width());
            })
            .children(self.leading.clone())
            .maybe_child(self.busy.then(busy_indicator));
        let trailing = rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S1)])
            .on_sized(move |event: Event<SizedEventData>| {
                trailing_width.set_if_modified(event.area.width());
            })
            .children(self.trailing.clone());
        rect()
            .tw(&tokens, HEADER)
            .tw(
                &tokens,
                &[
                    pl(if self.leading.is_empty() { S3 } else { S2 }),
                    bg(self.background),
                ],
            )
            .on_pointer_down(drag_on_pointer_down)
            .child(title)
            .child(leading)
            .child(rect().tw(&tokens, &[w_flex()]))
            .child(trailing)
    }
}

/// A small spinner with a "Working" tooltip, shown beside the header title.
fn busy_indicator() -> Element {
    hover_tip(spinner(SpinnerSize::Small), "Working")
}

/// Minimise, maximise/restore and close, drawn as 24 px circles.
#[derive(PartialEq)]
pub struct WindowControls;

impl Component for WindowControls {
    fn render(&self) -> impl IntoElement {
        let maximized = freya::borderless::use_maximized();
        rect()
            .tw_layout(&[flex_row(), items(Align::Center), gap(S1)])
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
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut hovering = use_state(|| false);
        let action = self.action;
        let fill = match (hovering(), self.danger) {
            (false, _) => Paint::Transparent,
            (true, true) => Paint::Danger,
            (true, false) => Paint::Hover,
        };
        rect()
            .tw(
                &tokens,
                &[
                    w(Dim::WindowControl),
                    h(Dim::WindowControl),
                    rounded(Radius::Pill),
                    bg(fill),
                    center(),
                ],
            )
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.tooltip)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    event.stop_propagation();
                    run_window_action(action);
                }
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                run_window_action(action);
            })
            .child(icons::icon(
                self.icon,
                IconSize::Control,
                Paint::Text.color(&tokens),
            ))
            .focusable(focus(), Radius::Pill)
    }
}

fn run_window_action(action: Action) {
    match action {
        Action::Minimize => with_window(|window| window.set_minimized(true)),
        Action::ToggleMaximize => {
            with_window(|window| window.set_maximized(!window.is_maximized()))
        }
        Action::Close => Platform::get().close_window(Platform::window_id()),
    }
}
