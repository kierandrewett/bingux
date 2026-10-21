//! Window overlays: alert dialogs (`AdwAlertDialog`, SwiftUI `.alert`)
//! and toasts (`AdwToast`), shown by any component through the
//! [`Overlays`] handle.
//!
//! Wrap the window content in [`OverlayHost`] once; it provides the
//! handle and paints the scrim, dialog and toast on a top overlay level.

use std::time::Duration;

use freya::animation::*;
use freya::prelude::*;

use crate::controls::button::{ActionButton, action_button, primary_button};
use crate::scale::*;
use crate::surface::{is_activation_key, use_tokens};

const DIALOG_WIDTH: f32 = 400.0;

/// How a dialog response is styled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseKind {
    /// A plain elevated button.
    Default,
    /// The accent-filled button for the action the user most likely wants.
    Suggested,
    /// The danger-filled button for actions that lose data.
    Destructive,
}

/// One button at the bottom of a dialog.
#[derive(Clone, PartialEq)]
pub struct DialogResponse {
    pub label: String,
    pub kind: ResponseKind,
    pub on_press: EventHandler<()>,
}

/// An alert: title, body and one to three responses. Escape and the scrim
/// close it (the first `Default` response, or nothing, is treated as the
/// cancel action).
#[derive(Clone, PartialEq)]
pub struct Dialog {
    pub title: String,
    pub body: String,
    pub responses: Vec<DialogResponse>,
    /// Extra content between the body and the responses, such as a field.
    pub content: Option<Element>,
}

pub fn dialog(title: impl Into<String>, body: impl Into<String>) -> Dialog {
    Dialog {
        title: title.into(),
        body: body.into(),
        responses: Vec::new(),
        content: None,
    }
}

impl Dialog {
    pub fn response(
        mut self,
        label: impl Into<String>,
        kind: ResponseKind,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.responses.push(DialogResponse {
            label: label.into(),
            kind,
            on_press: on_press.into(),
        });
        self
    }

    /// A plain "Cancel"-style response that only closes the dialog.
    pub fn cancel(self, label: impl Into<String>) -> Self {
        self.response(label, ResponseKind::Default, |_| {})
    }

    pub fn content(mut self, content: impl Into<Element>) -> Self {
        self.content = Some(content.into());
        self
    }
}

/// A queued toast.
#[derive(Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    pub action: Option<(String, EventHandler<()>)>,
    pub timeout: Duration,
}

/// The shared handle. Copy it into closures; `show` and `dismiss` work
/// from anywhere below the [`OverlayHost`].
#[derive(Clone, Copy, PartialEq)]
pub struct Overlays {
    dialog: State<Option<Dialog>>,
    toasts: State<Vec<(u64, Toast)>>,
    next_toast: State<u64>,
}

impl Overlays {
    /// Open `dialog`, replacing any dialog already open.
    pub fn show_dialog(mut self, dialog: Dialog) {
        self.dialog.set(Some(dialog));
    }

    pub fn dismiss_dialog(mut self) {
        self.dialog.set(None);
    }

    /// Queue a toast that dismisses itself after four seconds.
    pub fn toast(self, text: impl Into<String>) {
        self.push(Toast {
            text: text.into(),
            action: None,
            timeout: Duration::from_secs(4),
        });
    }

    /// Queue a toast with a button, such as "Undo".
    pub fn toast_with_action(
        self,
        text: impl Into<String>,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) {
        self.push(Toast {
            text: text.into(),
            action: Some((label.into(), on_press.into())),
            timeout: Duration::from_secs(6),
        });
    }

    fn push(mut self, toast: Toast) {
        let id = *self.next_toast.read();
        self.next_toast.set(id + 1);
        self.toasts.write().push((id, toast));
    }

    fn dismiss_toast(mut self, id: u64) {
        self.toasts.write().retain(|(other, _)| *other != id);
    }
}

/// The handle provided by the nearest [`OverlayHost`].
pub fn use_overlays() -> Overlays {
    use_consume::<Overlays>()
}

/// Provides [`Overlays`] and draws the dialog and toasts over `child`.
#[derive(PartialEq)]
pub struct OverlayHost {
    child: Element,
}

impl OverlayHost {
    pub fn new(child: impl Into<Element>) -> Self {
        Self {
            child: child.into(),
        }
    }
}

impl Component for OverlayHost {
    fn render(&self) -> impl IntoElement {
        let overlays = use_hook(|| Overlays {
            dialog: State::create(None),
            toasts: State::create(Vec::new()),
            next_toast: State::create(0),
        });
        use_provide_context(|| overlays);
        let dialog = overlays.dialog.read().clone();
        let toast = overlays.toasts.read().first().cloned();

        rect()
            .expanded()
            .child(self.child.clone())
            .maybe_child(dialog.map(|dialog| DialogLayer { overlays, dialog }))
            .maybe_child(toast.map(|(id, toast)| ToastLayer {
                overlays,
                id,
                toast,
            }))
    }
}

#[derive(PartialEq)]
struct DialogLayer {
    overlays: Overlays,
    dialog: Dialog,
}

impl Component for DialogLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let a11y_id = use_a11y();
        let motion = tokens.motion(160);
        let appear = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0)
                .time(motion)
                .function(Function::Cubic)
                .ease(Ease::Out)
        });
        let progress = appear.get().value();
        use_side_effect(move || a11y_id.request_focus());

        let responses = self.dialog.responses.clone();
        let buttons = responses.iter().map(|response| {
            let on_press = response.on_press.clone();
            let press = move |_: ()| {
                on_press.call(());
                overlays.dismiss_dialog();
            };
            let button: ActionButton = match response.kind {
                ResponseKind::Suggested => primary_button(response.label.clone(), press),
                ResponseKind::Destructive => {
                    action_button(response.label.clone(), press).destructive()
                }
                ResponseKind::Default => action_button(response.label.clone(), press),
            };
            Element::from(button.grow())
        });

        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::window_percent(100.0))
            .height(Size::window_percent(100.0))
            .layer(Layer::OverlayLevel(8))
            .center()
            .background(Color::BLACK.with_af32(0.4 * progress))
            .on_press(move |_: Event<PressEventData>| overlays.dismiss_dialog())
            .child(
                rect()
                    .a11y_id(a11y_id)
                    .a11y_focusable(true)
                    .a11y_role(AccessibilityRole::AlertDialog)
                    .a11y_alt(self.dialog.title.clone())
                    .width(Size::px(DIALOG_WIDTH))
                    .padding(S4)
                    .spacing(S3)
                    .corner_radius(RADIUS_XL)
                    .background(tokens.popover)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(tokens.outline),
                    )
                    .shadow(
                        Shadow::new()
                            .position(ShadowPosition::Normal)
                            .y(16.0)
                            .blur(48.0)
                            .color(Color::BLACK.with_af32(0.45)),
                    )
                    .scale(0.96 + 0.04 * progress)
                    .opacity(progress)
                    .on_press(|event: Event<PressEventData>| event.stop_propagation())
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if event.key == Key::Named(NamedKey::Escape) {
                            event.stop_propagation();
                            overlays.dismiss_dialog();
                        }
                    })
                    .child(
                        label()
                            .text(self.dialog.title.clone())
                            .font_size(TEXT_LG)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .color(tokens.text)
                            .text_align(TextAlign::Center)
                            .width(Size::fill()),
                    )
                    .maybe_child((!self.dialog.body.is_empty()).then(|| {
                        paragraph()
                            .width(Size::fill())
                            .color(tokens.muted)
                            .font_size(TEXT_MD)
                            .text_align(TextAlign::Center)
                            .child(self.dialog.body.clone())
                    }))
                    .maybe_child(self.dialog.content.clone())
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .content(Content::Flex)
                            .spacing(S2)
                            .children(buttons),
                    ),
            )
    }
}

#[derive(PartialEq)]
struct ToastLayer {
    overlays: Overlays,
    id: u64,
    toast: Toast,
}

impl Component for ToastLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let id = self.id;
        let timeout = self.toast.timeout;
        let motion = tokens.motion(200);
        let appear = use_animation(move |conf| {
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 1.0)
                .time(motion)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let progress = appear.get().value();
        use_hook(move || {
            spawn(async move {
                timer(timeout).await;
                overlays.dismiss_toast(id);
            });
        });
        let action = self
            .toast
            .action
            .clone()
            .map(|(text, on_press)| ToastAction {
                text,
                on_press: EventHandler::new(move |_: ()| {
                    on_press.call(());
                    overlays.dismiss_toast(id);
                }),
            });

        rect()
            .position(Position::new_absolute().bottom(24.0).left(0.0))
            .width(Size::window_percent(100.0))
            .layer(Layer::OverlayLevel(9))
            .cross_align(Alignment::Center)
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Status)
                    .a11y_alt(self.toast.text.clone())
                    .height(Size::px(BOX_XL))
                    .padding((0.0, S1, 0.0, S4))
                    .spacing(S3)
                    .direction(Direction::Horizontal)
                    .cross_align(Alignment::Center)
                    .corner_radius(BOX_XL / 2.0)
                    .background(tokens.popover)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(tokens.outline),
                    )
                    .shadow(
                        Shadow::new()
                            .position(ShadowPosition::Normal)
                            .y(8.0)
                            .blur(24.0)
                            .color(Color::BLACK.with_af32(0.35)),
                    )
                    .opacity(progress)
                    .offset_y(12.0 * (1.0 - progress))
                    .child(
                        label()
                            .text(self.toast.text.clone())
                            .font_size(TEXT_MD)
                            .color(tokens.text)
                            .max_lines(1),
                    )
                    .maybe_child(action)
                    .child(
                        rect()
                            .width(Size::px(BOX_MD))
                            .height(Size::px(BOX_MD))
                            .corner_radius(RADIUS_LG)
                            .center()
                            .cursor(CursorIcon::Pointer)
                            .a11y_role(AccessibilityRole::Button)
                            .a11y_alt("Dismiss")
                            .on_press(move |_: Event<PressEventData>| overlays.dismiss_toast(id))
                            .child(crate::icons::icon(
                                crate::icons::Icon::WindowClose,
                                ICON_SM,
                                tokens.muted,
                            )),
                    ),
            )
    }
}

#[derive(PartialEq)]
struct ToastAction {
    text: String,
    on_press: EventHandler<()>,
}

impl Component for ToastAction {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_focusable(true)
            .height(Size::px(BOX_MD))
            .padding((0.0, S2))
            .corner_radius(RADIUS_LG)
            .center()
            .background(if hovering() {
                tokens.hover
            } else {
                Color::TRANSPARENT
            })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(());
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    on_key.call(());
                }
            })
            .child(
                label()
                    .text(self.text.clone())
                    .font_size(TEXT_SM)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(tokens.accent)
                    .max_lines(1),
            )
    }
}
