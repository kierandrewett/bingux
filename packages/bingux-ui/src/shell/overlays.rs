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
use crate::shell::menu::{Menu, MenuItem, MenuLayer};
use crate::shell::sheet::{Sheet, SheetLayer};
use crate::surface::{is_activation_key, use_tokens};
use crate::typography::MODAL_TITLE;

const DIALOG_WIDTH: f32 = 420.0;
/// The corner radius of an alert dialog.
const RADIUS_DIALOG: f32 = S5 + S1;

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
    dialog_closing: State<bool>,
    sheet: State<Option<Sheet>>,
    sheet_closing: State<bool>,
    menu: State<Option<Menu>>,
    menu_closing: State<bool>,
    toasts: State<Vec<(u64, Toast)>>,
    toast_closing: State<Option<u64>>,
    next_toast: State<u64>,
}

impl Overlays {
    /// Open `dialog`, replacing any dialog already open.
    pub fn show_dialog(mut self, dialog: Dialog) {
        self.dialog_closing.set(false);
        self.dialog.set(Some(dialog));
    }

    /// Close the open dialog. It animates out before it leaves the window.
    pub fn dismiss_dialog(mut self) {
        if self.dialog.read().is_some() && !*self.dialog_closing.read() {
            self.dialog_closing.set(true);
        }
    }

    fn finish_dialog(mut self) {
        if *self.dialog_closing.read() {
            self.dialog.set(None);
            self.dialog_closing.set(false);
        }
    }

    /// Open `sheet`, replacing any sheet already open.
    pub fn show_sheet(mut self, sheet: Sheet) {
        self.sheet_closing.set(false);
        self.sheet.set(Some(sheet));
    }

    /// Close the open sheet. The primary action calls this once its work
    /// succeeds; Cancel and Escape call it themselves.
    pub fn close_sheet(mut self) {
        if self.sheet.read().is_some() && !*self.sheet_closing.read() {
            self.sheet_closing.set(true);
        }
    }

    pub(crate) fn finish_sheet(mut self) {
        if *self.sheet_closing.read() {
            self.sheet.set(None);
            self.sheet_closing.set(false);
        }
    }

    /// Open a menu of `items` at `at`, a point in window coordinates. The
    /// menu closes when an item is picked, on Escape, or on a press outside it.
    pub fn show_menu(mut self, at: (f32, f32), items: Vec<MenuItem>) {
        self.menu_closing.set(false);
        self.menu.set(Some(Menu {
            x: at.0,
            y: at.1,
            items,
        }));
    }

    /// Close the open menu.
    pub(crate) fn close_menu(mut self) {
        if self.menu.read().is_some() && !*self.menu_closing.read() {
            self.menu_closing.set(true);
        }
    }

    pub(crate) fn finish_menu(mut self) {
        if *self.menu_closing.read() {
            self.menu.set(None);
            self.menu_closing.set(false);
        }
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

    /// Start the exit of toast `id`, if it is the one on screen.
    fn dismiss_toast(mut self, id: u64) {
        let on_screen = self
            .toasts
            .read()
            .first()
            .is_some_and(|(first, _)| *first == id);
        if on_screen && self.toast_closing.read().is_none() {
            self.toast_closing.set(Some(id));
        }
    }

    fn finish_toast(mut self, id: u64) {
        if *self.toast_closing.read() == Some(id) {
            self.toasts.write().retain(|(other, _)| *other != id);
            self.toast_closing.set(None);
        }
    }
}

/// The handle provided by the nearest [`OverlayHost`].
pub fn use_overlays() -> Overlays {
    use_consume::<Overlays>()
}

/// The enter and exit progress of one overlay layer, from 0 (hidden) to 1
/// (shown), on the shared popup motion. When `closing` turns on, the progress
/// runs back to 0 and `finish(key)` runs once it has, so the layer stays
/// mounted through its exit. A new `key` plays the enter again.
pub(crate) fn use_presence(
    key: u64,
    closing: bool,
    motion: u64,
    finish: impl Fn(u64) + Clone + 'static,
) -> f32 {
    let appear = use_animation(move |conf| {
        conf.on_creation(OnCreation::Run);
        AnimNum::new(0.0, 1.0)
            .time(motion)
            .function(Function::Quad)
            .ease(Ease::Out)
    });
    use_side_effect_with_deps(&(key, closing), move |&(key, closing): &(u64, bool)| {
        let mut appear = appear;
        if closing {
            appear.reverse();
            let finish = finish.clone();
            spawn(async move {
                timer(Duration::from_millis(motion)).await;
                finish(key);
            });
        } else {
            appear.start();
        }
    });
    appear.get().value()
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
            dialog_closing: State::create(false),
            toasts: State::create(Vec::new()),
            next_toast: State::create(0),
            toast_closing: State::create(None),
            sheet: State::create(None),
            sheet_closing: State::create(false),
            menu: State::create(None),
            menu_closing: State::create(false),
        });
        use_provide_context(|| overlays);
        let dialog = overlays.dialog.read().clone();
        let dialog_closing = *overlays.dialog_closing.read();
        let toast = overlays.toasts.read().first().cloned();
        let toast_closing = *overlays.toast_closing.read();
        let sheet = overlays.sheet.read().clone();
        let sheet_closing = *overlays.sheet_closing.read();
        let menu = overlays.menu.read().clone();
        let menu_closing = *overlays.menu_closing.read();

        rect()
            .expanded()
            .child(self.child.clone())
            .maybe_child(dialog.map(|dialog| DialogLayer {
                overlays,
                dialog,
                closing: dialog_closing,
            }))
            .maybe_child(toast.map(|(id, toast)| ToastLayer {
                overlays,
                id,
                toast,
                closing: toast_closing == Some(id),
            }))
            .maybe_child(sheet.map(|sheet| SheetLayer {
                overlays,
                sheet,
                closing: sheet_closing,
            }))
            .maybe_child(menu.map(|menu| MenuLayer {
                overlays,
                menu,
                closing: menu_closing,
            }))
    }
}

#[derive(PartialEq)]
struct DialogLayer {
    overlays: Overlays,
    dialog: Dialog,
    closing: bool,
}

impl Component for DialogLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let a11y_id = use_a11y();
        let progress = use_presence(0, self.closing, tokens.motion(120), move |_| {
            overlays.finish_dialog()
        });
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
            Element::from(button)
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
                    .padding(S5)
                    .spacing(S3)
                    .corner_radius(RADIUS_DIALOG)
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
                            .y(24.0)
                            .blur(64.0)
                            .color(Color::BLACK.with_af32(0.55)),
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
                        MODAL_TITLE
                            .paragraph(&tokens, self.dialog.title.clone())
                            .width(Size::fill()),
                    )
                    .maybe_child((!self.dialog.body.is_empty()).then(|| {
                        paragraph()
                            .width(Size::fill())
                            .color(tokens.muted)
                            .font_size(TEXT_MD)
                            .child(self.dialog.body.clone())
                    }))
                    .maybe_child(self.dialog.content.clone())
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Horizontal)
                            .main_align(Alignment::End)
                            .spacing(S2)
                            .padding((S2, 0.0, 0.0, 0.0))
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
    closing: bool,
}

impl Component for ToastLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let id = self.id;
        let timeout = self.toast.timeout;
        let progress = use_presence(id, self.closing, tokens.motion(140), move |id| {
            overlays.finish_toast(id)
        });
        // Each toast that takes over the layer gets its own timer.
        use_side_effect_with_deps(&(id, timeout), move |&(id, timeout): &(u64, Duration)| {
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
                    .offset_y(S2 * (1.0 - progress))
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
