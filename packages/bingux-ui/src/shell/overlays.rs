//! Window overlays: alert dialogs (`AdwAlertDialog`, SwiftUI `.alert`)
//! and toasts (`AdwToast`), shown by any component through the
//! [`Overlays`] handle.
//!
//! Wrap the window content in [`OverlayHost`] once; it provides the
//! handle and paints the scrim, dialog and toast on a top overlay level.

use std::time::Duration;

use freya::prelude::*;

use crate::address::Address;
use crate::app::current_address;
use crate::controls::button::{ActionButton, action_button, primary_button};
use crate::controls::press::use_press;
use crate::motion::use_presence;
use crate::shell::menu::{Menu, MenuItem, MenuLayer, MenuNav};
use crate::shell::panel::{Panel, PanelTrigger};
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
use crate::typography::{BODY, EMPHASIS, Ink, SECTION};

/// The toast pill: 44 px tall, text then action and dismiss, on the same dark
/// floating surface as a tooltip.
const TOAST: Recipe = &[
    flex_row(),
    items(Align::Center),
    h(Dim::Box11),
    pl(S4),
    pr(S1),
    gap(S3),
    rounded(Radius::Pill),
    bg(Paint::Floating),
    border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline)),
    shadow(Elevation::Raised),
];

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
    menu: State<Option<Menu>>,
    menu_closing: State<bool>,
    /// The focus node of the open menu's panel. The panel takes focus when it
    /// opens; when it closes, focus goes back to the owner only if the panel
    /// still holds it.
    pub(crate) menu_panel: AccessibilityId,
    pub(crate) menu_nav: State<MenuNav>,
    /// The window address the open menu was opened on. A menu belongs to that
    /// page, so the window stops drawing it once the address moves on.
    menu_page: State<Option<Address>>,
    toasts: State<Vec<(u64, Toast)>>,
    toast_closing: State<Option<u64>>,
    next_toast: State<u64>,
}

impl Overlays {
    /// Overlay state owned by `scope`, so a popup's overlays outlive the
    /// component that opened it and go when the popup is removed.
    pub(crate) fn in_scope(scope: ScopeId) -> Self {
        Self {
            dialog: State::create_in_scope(None, scope),
            dialog_closing: State::create_in_scope(false, scope),
            menu: State::create_in_scope(None, scope),
            menu_closing: State::create_in_scope(false, scope),
            menu_panel: AccessibilityId::new_unique(),
            menu_nav: State::create_in_scope(MenuNav::default(), scope),
            menu_page: State::create_in_scope(None, scope),
            toasts: State::create_in_scope(Vec::new(), scope),
            toast_closing: State::create_in_scope(None, scope),
            next_toast: State::create_in_scope(0, scope),
        }
    }

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

    /// Open a menu of `items` at `at`, a point in window coordinates. The
    /// menu closes when an item is picked, on Escape, or on a press outside it.
    pub fn show_menu(self, at: (f32, f32), items: Vec<MenuItem>) {
        self.show_menu_from(None, PanelTrigger::Pointer(at), items, false);
    }

    /// Open a menu of `items` against `trigger`, owned by the control `owner`.
    /// A menu opened from the keyboard highlights its first item. When the menu
    /// closes, focus returns to `owner` if the menu still holds it.
    pub(crate) fn show_menu_from(
        mut self,
        owner: Option<AccessibilityId>,
        trigger: PanelTrigger,
        items: Vec<MenuItem>,
        keyboard: bool,
    ) {
        self.menu_closing.set(false);
        self.menu_page.set(current_address());
        self.menu_nav.set(MenuNav::new(owner, &items, keyboard));
        self.menu.set(Some(Menu { trigger, items }));
    }

    /// Whether the open menu was opened by the control `owner`.
    pub(crate) fn menu_owned_by(self, owner: AccessibilityId) -> bool {
        self.menu.read().is_some() && self.menu_nav.read().owner == Some(owner)
    }

    /// Close the open menu from the keyboard. Focus returns to the owner when
    /// the menu finishes closing.
    pub(crate) fn dismiss_menu_by_key(self) {
        self.close_menu();
    }

    /// Close the open menu.
    pub(crate) fn close_menu(mut self) {
        if self.menu.read().is_some() && !*self.menu_closing.read() {
            self.menu_closing.set(true);
        }
    }

    /// Remove the menu once it has animated out. Focus goes back to the owner
    /// only if the panel still holds it, so an item that opened a popup or
    /// moved focus keeps its new focus.
    pub(crate) fn finish_menu(mut self) {
        if !*self.menu_closing.read() {
            return;
        }
        let owner = self.menu_nav.peek().owner;
        let panel_focused = *Platform::get()
            .current_window()
            .focused_accessibility_id
            .peek()
            == self.menu_panel;
        self.menu.set(None);
        self.menu_nav.set(MenuNav::default());
        self.menu_closing.set(false);
        if let (Some(owner), true) = (owner, panel_focused) {
            owner.request_focus();
        }
    }

    /// Drop the open menu at once, with no exit animation. The window's address
    /// moved on, so the page the menu opened on is gone.
    fn forget_menu(mut self) {
        if self.menu.peek().is_none() {
            return;
        }
        self.menu.set(None);
        self.menu_nav.set(MenuNav::default());
        self.menu_closing.set(false);
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

/// Provides [`Overlays`] for the main window and draws its layers over `child`.
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
        let scope = current_scope_id();
        let overlays = use_hook(move || Overlays::in_scope(scope));
        use_provide_context(|| overlays);
        // The menu belongs to the window's page. Without a window address (a
        // gallery or the inspector) both sides are None and the menu stays.
        let page = use_try_consume::<State<Address>>().map(|address| (*address.read()).clone());
        use_side_effect_with_deps(&page, move |_| overlays.forget_menu());

        rect()
            .expanded()
            .child(crate::window::WindowHost::new(self.child.clone()))
            .child(OverlayLayers {
                overlays,
                stack: OverlayStack::Window,
                page,
            })
    }
}

/// Where an [`OverlayLayers`] draws. The main window's layers sit on fixed
/// levels above its popups. A popup's layers sit inside its own frame, one
/// overlay step above the panel, so they stay above that popup's content and
/// below the main window's dialog and toasts.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum OverlayStack {
    Window,
    Popup,
}

impl OverlayStack {
    fn menu_layer(self) -> Layer {
        match self {
            Self::Window => Layer::OverlayLevel(8),
            Self::Popup => Layer::Overlay,
        }
    }

    fn dialog_layer(self) -> Layer {
        match self {
            Self::Window => Layer::OverlayLevel(12),
            Self::Popup => Layer::Overlay,
        }
    }

    fn toast_layer(self) -> Layer {
        match self {
            Self::Window => Layer::OverlayLevel(13),
            Self::Popup => Layer::Overlay,
        }
    }
}

/// Draws the toast, dialog and menu of one [`Overlays`] handle. The main
/// window renders one; each popup renders its own inside its frame.
#[derive(PartialEq)]
pub(crate) struct OverlayLayers {
    pub(crate) overlays: Overlays,
    pub(crate) stack: OverlayStack,
    /// The window address the window's menu must still be on. A popup has no
    /// window address, so its menu is never filtered by it.
    pub(crate) page: Option<Address>,
}

impl Component for OverlayLayers {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let stack = self.stack;
        let dialog = overlays.dialog.read().clone();
        let dialog_closing = *overlays.dialog_closing.read();
        let toast = overlays.toasts.read().first().cloned();
        let toast_closing = *overlays.toast_closing.read();
        let menu = overlays.menu.read().clone().filter(|_| match stack {
            OverlayStack::Window => self.page.is_none() || *overlays.menu_page.read() == self.page,
            OverlayStack::Popup => true,
        });
        let menu_closing = *overlays.menu_closing.read();

        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .tw(&tokens, &[w_screen(), h_screen()])
            .maybe_child(toast.map(|(id, toast)| ToastLayer {
                overlays,
                id,
                toast,
                closing: toast_closing == Some(id),
                layer: stack.toast_layer(),
            }))
            .maybe_child(dialog.map(|dialog| DialogLayer {
                overlays,
                dialog,
                closing: dialog_closing,
                layer: stack.dialog_layer(),
            }))
            .maybe_child(menu.map(|menu| MenuLayer {
                overlays,
                menu,
                closing: menu_closing,
                layer: stack.menu_layer(),
            }))
    }
}

/// The widest a dialog gets, as a share of the window width, so it keeps a
/// margin on a phone.
const DIALOG_MAX_WIDTH_PERCENT: f32 = 94.0;

#[derive(PartialEq)]
struct DialogLayer {
    overlays: Overlays,
    dialog: Dialog,
    closing: bool,
    layer: Layer,
}

impl Component for DialogLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let a11y_id = use_a11y();
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

        let card = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::AlertDialog)
            .a11y_alt(self.dialog.title.clone())
            .tw(&tokens, DIALOG)
            .tw(&tokens, &[w(Dim::Dialog)])
            .max_width(Size::window_percent(DIALOG_MAX_WIDTH_PERCENT))
            // A press on the card never reaches the panel's scrim.
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .child(
                SECTION
                    .label(&tokens, self.dialog.title.clone())
                    .text_align(TextAlign::Center)
                    .width(Size::fill()),
            )
            .maybe_child((!self.dialog.body.is_empty()).then(|| {
                BODY.in_ink(Ink::Muted)
                    .paragraph(&tokens, self.dialog.body.clone())
                    .width(Size::fill())
                    .text_align(TextAlign::Center)
            }))
            .maybe_child(self.dialog.content.clone())
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), grow_children(), gap(S2)])
                    .children(buttons),
            );

        Element::from(
            Panel::dialog(!self.closing, self.layer, vec![card.into_element()])
                .on_dismiss(EventHandler::new(move |_: ()| overlays.dismiss_dialog()))
                .on_closed(EventHandler::new(move |_: ()| overlays.finish_dialog())),
        )
    }
}

/// The widest a toast's text gets, as a share of the window width, so the
/// action and the close button stay on a phone.
const TOAST_TEXT_MAX_WIDTH_PERCENT: f32 = 60.0;

#[derive(PartialEq)]
struct ToastLayer {
    overlays: Overlays,
    id: u64,
    toast: Toast,
    closing: bool,
    layer: Layer,
}

impl Component for ToastLayer {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.id)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let id = self.id;
        let timeout = self.toast.timeout;
        let progress = use_presence(!self.closing, tokens.motion(140), move || {
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
            .position(Position::new_absolute().bottom(S6.px()).left(0.0))
            .layer(self.layer)
            .tw(&tokens, &[w_screen(), flex_col(), items(Align::Center)])
            .child(
                rect()
                    .a11y_role(AccessibilityRole::Status)
                    .a11y_alt(self.toast.text.clone())
                    .tw(&tokens, TOAST)
                    .opacity(progress)
                    .offset_y(S2.px() * (1.0 - progress))
                    .child(
                        BODY.label(&tokens, self.toast.text.clone())
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .max_width(Size::window_percent(TOAST_TEXT_MAX_WIDTH_PERCENT)),
                    )
                    .maybe_child(action)
                    .child(
                        crate::controls::icon_button(crate::icons::Icon::WindowClose, move |_| {
                            overlays.dismiss_toast(id)
                        })
                        .size(Dim::Box7)
                        .tooltip("Dismiss"),
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
        let press = use_press();
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        rect()
            .a11y_role(AccessibilityRole::Button)
            .a11y_focusable(true)
            .tw(
                &tokens,
                &[
                    h(Dim::Box7),
                    px(S2),
                    rounded(Radius::Card),
                    center(),
                    surface(Paint::Transparent, hovering(), false, false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_pointer_down(move |event: Event<PointerEventData>| press.begin(&event, None))
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .on_global_pointer_up(move |_| press.release())
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                // A touch that scrolled off the action is not a tap.
                if !press.tap(&event) {
                    return;
                }
                on_press.call(());
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    on_key.call(());
                }
            })
            .child(
                EMPHASIS
                    .in_ink(Ink::Accent)
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            )
    }
}
