//! Borderless window chrome: a header bar that drags the window and the
//! minimise, maximise and close controls drawn as quiet circles.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::press::hit_area;
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
    tab_slot: Option<Element>,
    busy: bool,
    draggable: bool,
    title_start: bool,
}

impl HeaderBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            leading: Vec::new(),
            trailing: Vec::new(),
            background: Paint::Surface,
            title_slot: None,
            tab_slot: None,
            busy: false,
            draggable: true,
            title_start: false,
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

    /// Draw `element` where the title goes, in place of the title text. The
    /// slot is as wide as the room between the controls, centred on the bar,
    /// and `w_full()` fills it. A row in it that gives one child `w_flex()`
    /// needs `grow_children()`, or that child takes the whole row and pushes
    /// the rest out under the controls.
    pub fn title_slot(mut self, element: impl Into<Element>) -> Self {
        self.title_slot = Some(element.into());
        self
    }

    /// Put a strip of tabs where the title goes: from the start of the room
    /// between the controls, filling it, and as tall as the bar. The bar is
    /// then the strip, [`Dim::TabBar`] tall rather than a header (the window
    /// has no second row above its tabs), and its icon buttons are
    /// [`Dim::TabButton`], the height of a tab. Leave the bar on its default
    /// fill, [`Paint::Surface`]: the strip fades its overflowing tabs into it.
    /// Replaces the title and [`title_slot`](Self::title_slot).
    pub fn tab_slot(mut self, strip: impl Into<Element>) -> Self {
        self.tab_slot = Some(strip.into());
        self
    }

    /// Whether pressing the bar drags the window (on by default). A bar inside
    /// a dialog or a sheet is not a window title bar, so it turns this off.
    pub fn draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    /// Lay the title out from the start, in the room between the leading and
    /// the trailing controls, instead of centring it on the bar. A viewer
    /// header with many buttons uses this: a centred title would give up the
    /// room of both sides' wider cluster and leave a narrow pane no name.
    pub fn title_start(mut self, start: bool) -> Self {
        self.title_start = start;
        self
    }
}

impl Component for HeaderBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        // Icon buttons below take the header button size (`HEADER_BUTTON`),
        // or in a bar that holds tabs the inline button size.
        use_provide_context(|| crate::controls::button::InHeader);
        let tabs = self.tab_slot.is_some();
        use_provide_context(move || crate::controls::button::InTabBar(tabs));
        // The title is laid out on the whole bar, so its centre is the bar's
        // centre whatever the controls weigh. Its side insets are the wider of
        // the two control clusters, so it never runs under a control.
        let mut leading_width = use_state(|| 0.0_f32);
        let mut trailing_width = use_state(|| 0.0_f32);
        let mut title_width = use_state(|| 0.0_f32);
        let inset = leading_width().max(trailing_width()) + S2.px() + S1.px();
        // The title layer sits in the bar's content box, inside the padding. A
        // bar whose title starts the bar and that has no leading control keeps
        // S3 at the start, so the title is clear of the edge; every other bar
        // has S2 at both ends, equal to its trailing end (`inset-asymmetry`). The
        // title is centred on the bar either way: both sides are cleared by the
        // wider padding, not on a content box that is off centre by half the
        // difference.
        let pad_left = if self.leading.is_empty() && self.title_start {
            S3
        } else {
            S2
        };
        let pad_right = S2;
        let pad = pad_left.px().max(pad_right.px());
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
        // The measured width is the content box's: the paddings are added back for the bar's,
        // and both clearances come off for the title's run-time width.
        let centre_width =
            (title_width() + pad_left.px() + pad_right.px() - 2.0 * (pad + inset)).max(0.0);
        let title = rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .tw(&tokens, &[w_full(), h_full()])
            .on_sized(move |event: Event<SizedEventData>| {
                title_width.set_if_modified(event.area.width());
            })
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(inset + pad - pad_left.px()))
                    .width(Size::px(centre_width))
                    // Freya does not size an absolute box from top and bottom,
                    // so the row takes the bar's full height to centre the title.
                    .tw(&tokens, &[h_full(), flex_row(), items(Align::Center)])
                    .child(text.clone()),
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
        let inert = try_consume_context::<InertControls>().is_some();
        let draggable = self.draggable;
        let bar = rect()
            .component("HeaderBar")
            .tw(&tokens, HEADER)
            // A bar that holds tabs is the strip: a tab bar tall, not a header.
            .maybe(tabs, |bar| bar.tw(&tokens, &[h(Dim::TabBar)]))
            .tw(
                &tokens,
                &[
                    pl(pad_left),
                    bg(self.background),
                ],
            )
            .on_pointer_down(move |event: Event<PointerEventData>| {
                if !inert && draggable {
                    drag_on_pointer_down(event);
                }
            });
        match &self.tab_slot {
            // A strip of tabs takes the room between the controls, from the
            // start, and is as tall as the bar. An empty leading cluster would
            // still take a gap, so it is left out.
            Some(strip) => bar
                .maybe_child((!self.leading.is_empty() || self.busy).then(|| leading))
                .child(rect().tw(&tokens, &[w_flex(), h_full()]).child(strip.clone()))
                .child(trailing),
            None if self.title_start => bar
                .child(leading)
                .child(
                    rect()
                        .tw(&tokens, &[w_flex(), h_full(), flex_row(), items(Align::Center), px(S2)])
                        .child(text),
                )
                .child(trailing),
            None => bar
                .child(title)
                .child(leading)
                .child(rect().tw(&tokens, &[w_flex()]))
                .child(trailing),
        }
    }
}

/// A small spinner with a "Working" tooltip, shown beside the header title.
fn busy_indicator() -> Element {
    hover_tip(spinner(SpinnerSize::Small), "Working")
}

/// Provide this below a window that is only a picture of a window (the
/// taxonomy's specimens, a documentation page): its [`WindowControls`] draw,
/// hover and focus as usual, and do nothing when pressed, so a demonstration
/// cannot minimise or close the app it is shown in.
#[derive(Clone, Copy, PartialEq)]
pub struct InertControls;

/// Minimise, maximise/restore and close, drawn as 24 px circles.
#[derive(PartialEq)]
pub struct WindowControls;

impl Component for WindowControls {
    fn render(&self) -> impl IntoElement {
        let maximized = freya::borderless::use_maximized();
        rect()
            .component("WindowControls")
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
        let hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let action = self.action;
        let inert = try_consume_context::<InertControls>().is_some();
        // Close lights red under the pointer and darker while pressed; the others
        // take the hover layer and then the press layer.
        let danger = self.danger && (hovering() || pressing());
        let base = if danger { Paint::Danger } else { Paint::Transparent };
        rect()
            .component("WindowControl")
            .tw(
                &tokens,
                &[
                    w(Dim::WindowControl),
                    h(Dim::WindowControl),
                    rounded(Radius::Pill),
                    surface(base, hovering() && !self.danger, pressing(), false),
                    center(),
                ],
            )
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.tooltip)
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                pressing.set(true);
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    event.stop_propagation();
                    if !inert {
                        run_window_action(action);
                    }
                }
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                if !inert {
                    run_window_action(action);
                }
            })
            .child(icons::icon(
                self.icon,
                IconSize::Control,
                if self.danger && hovering() {
                    Paint::OnDanger.color(&tokens)
                } else {
                    Paint::Text.color(&tokens)
                },
            ))
            // The circle is 24 px; what a finger or a hand presses is
            // Dim::WindowControlTarget (32 px, 44 px at compact density).
            .child({
                let side = Dim::WindowControlTarget.px();
                hit_area((Dim::WindowControl.px(), Dim::WindowControl.px()), (side, side), hovering, pressing)
            })
            .focusable(focus())
    }
}

fn run_window_action(action: Action) {
    // Test builds record the action; there is no window to act on.
    #[cfg(all(test, feature = "testing"))]
    crate::controls::hit_area_tests::record_action(match action {
        Action::Minimize => "minimise",
        Action::ToggleMaximize => "maximise",
        Action::Close => "close",
    });
    #[cfg(not(all(test, feature = "testing")))]
    match action {
        Action::Minimize => with_window(|window| window.set_minimized(true)),
        Action::ToggleMaximize => {
            with_window(|window| window.set_maximized(!window.is_maximized()))
        }
        Action::Close => Platform::get().close_window(Platform::window_id()),
    }
}
