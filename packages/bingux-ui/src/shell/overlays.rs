//! Window overlays: alert dialogs (`AdwAlertDialog`, SwiftUI `.alert`)
//! and toasts (`AdwToast`), shown by any component through the
//! [`Overlays`] handle.
//!
//! Wrap the window content in [`OverlayHost`] once; it provides the
//! handle and paints the scrim, dialog and toast on a top overlay level.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

use crate::component::ComponentName;
use crate::focus::owner::{OverlayPhase, Restore, keeps_focus, restore_on_close};
use crate::motion::use_presence;
use crate::nav::try_router;
use crate::shell::banner::{Banner, BannerKind};
use crate::shell::dialog::{Dialog, DialogLayer};
use crate::shell::hide_clock::HideClock;
use crate::shell::menu::{Menu, MenuItem, MenuLayer, MenuNav};
use crate::shell::panel::{PanelTrigger, use_sheet_pull};
use crate::style::*;
use crate::surface::use_tokens;

/// A queued toast: a [`Banner`] that floats. It takes the banner's builders
/// (kind, title, icon, actions) and adds the part that makes it float: the
/// time it stays up. The pill, its colours and its buttons are the banner's,
/// drawn by one code path for an inline banner and a toast alike.
#[derive(Clone, PartialEq)]
pub struct Toast {
    banner: Banner,
    timeout: Option<Duration>,
}

impl Toast {
    /// A neutral toast with `text`: the dark floating pill.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            banner: Banner::new(text).kind(BannerKind::Neutral).floating(),
            timeout: None,
        }
    }

    pub fn kind(mut self, kind: BannerKind) -> Self {
        self.banner = self.banner.kind(kind);
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.banner = self.banner.title(title);
        self
    }

    pub fn icon(mut self, icon: crate::icons::Icon) -> Self {
        self.banner = self.banner.icon(icon);
        self
    }

    pub fn without_icon(mut self) -> Self {
        self.banner = self.banner.without_icon();
        self
    }

    /// A button such as "Undo"; pressing it also dismisses the toast.
    pub fn action(
        mut self,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.banner = self.banner.action(label, on_press);
        self
    }

    /// How long it stays up while nothing holds it: four seconds, six with an
    /// action.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn timeout_or_default(&self) -> Duration {
        self.timeout.unwrap_or(if self.banner.action_count() > 0 {
            Duration::from_secs(6)
        } else {
            Duration::from_secs(4)
        })
    }
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
    /// The stack entry the open menu was opened on. A menu belongs to that
    /// page, so the window stops drawing it once the page changes.
    menu_page: State<Option<u64>>,
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

    /// Whether a dialog is open or fading out. A component that floats its
    /// own card over the page, such as a hover card, stays hidden meanwhile.
    pub fn dialog_open(self) -> bool {
        self.dialog.read().is_some()
    }

    /// Whether a menu is open or fading out. A hover tooltip stays hidden
    /// meanwhile, so it never covers the menu.
    pub(crate) fn menu_open(self) -> bool {
        self.menu.read().is_some()
    }

    /// Close the open dialog. It animates out before it leaves the window.
    pub fn dismiss_dialog(mut self) {
        if self.dialog.read().is_some() && !*self.dialog_closing.read() {
            self.dialog_closing.set(true);
        }
    }

    /// Whether the open dialog is fading out. Its responses have run by then.
    pub(crate) fn dialog_closing(self) -> bool {
        *self.dialog_closing.peek()
    }

    pub(crate) fn finish_dialog(mut self) {
        if *self.dialog_closing.read() {
            let closed = self
                .dialog
                .peek()
                .as_ref()
                .and_then(|dialog| dialog.on_closed.clone());
            self.dialog.set(None);
            self.dialog_closing.set(false);
            if let Some(closed) = closed {
                closed.call(());
            }
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
        self.menu_page
            .set(try_router().map(|router| router.current_id()));
        self.menu_nav.set(MenuNav::new(owner, &items, keyboard));
        self.menu.set(Some(Menu { trigger, items }));
    }

    /// Whether the open menu was opened by the control `owner`.
    pub(crate) fn menu_owned_by(self, owner: AccessibilityId) -> bool {
        self.menu.read().is_some() && self.menu_nav.read().owner == Some(owner)
    }

    /// Whether `owner` still counts as focused for the menu it opened: while
    /// the menu is up, and while it closes back to `owner`. See
    /// [`crate::focus::owner`]; controls read this through
    /// [`use_visual_focus`](crate::focus::owner::use_visual_focus).
    pub(crate) fn keeps_focus(self, owner: AccessibilityId) -> bool {
        let phase = if !self.menu_owned_by(owner) {
            OverlayPhase::Closed
        } else if *self.menu_closing.read() {
            OverlayPhase::Closing
        } else {
            OverlayPhase::Open
        };
        let holds = *Platform::get()
            .current_window()
            .focused_accessibility_id
            .read()
            == self.menu_panel;
        keeps_focus(phase, holds)
    }

    /// Close the open menu from the keyboard. Focus returns to the owner when
    /// the menu finishes closing.
    pub(crate) fn dismiss_menu_by_key(self) {
        self.close_menu();
    }

    /// Close the open menu. Focus goes back to its owner at once, if the
    /// menu still holds it, so the owner is really focused again before the
    /// menu has finished fading out; an item that opens a popup or moves
    /// focus afterwards keeps its new focus.
    pub(crate) fn close_menu(mut self) {
        if self.menu.read().is_some() && !*self.menu_closing.read() {
            self.menu_closing.set(true);
            self.restore_focus();
        }
    }

    /// Gives focus back to the menu's owner when the menu still holds it.
    fn restore_focus(self) {
        let owner = self.menu_nav.peek().owner;
        let holds = *Platform::get()
            .current_window()
            .focused_accessibility_id
            .peek()
            == self.menu_panel;
        if let (Some(owner), Restore::ToOwner) = (owner, restore_on_close(owner.is_some(), holds))
        {
            owner.request_focus();
        }
    }

    /// Remove the menu once it has animated out. Focus has already gone back
    /// to the owner (see [`close_menu`](Self::close_menu)); this covers a
    /// menu whose panel only took focus while it was fading.
    pub(crate) fn finish_menu(mut self) {
        if !*self.menu_closing.read() {
            return;
        }
        self.restore_focus();
        self.menu.set(None);
        self.menu_nav.set(MenuNav::default());
        self.menu_closing.set(false);
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
        self.show_toast(Toast::new(text));
    }

    /// Queue a toast with a button, such as "Undo".
    pub fn toast_with_action(
        self,
        text: impl Into<String>,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) {
        self.show_toast(Toast::new(text).action(label, on_press));
    }

    /// Queue a built [`Toast`]: a kind, a title, several actions. Toasts show
    /// one at a time, in the order they were queued.
    pub fn show_toast(mut self, toast: Toast) {
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
        // The window host below uses this core too, so the menus and dialogs
        // drawn beside it share the page's focus state.
        let _ = crate::focus::provide_focus_core();
        let scope = current_scope_id();
        let overlays = use_hook(move || Overlays::in_scope(scope));
        use_provide_context(|| overlays);
        // A vetoed pop asks its question through these overlays.
        use_hook(move || {
            if let Some(router) = try_router() {
                router.attach_overlays(overlays);
            }
        });
        // The menu belongs to the window's page. Without a router (a gallery
        // or the inspector) both sides are None and the menu stays.
        let page = use_try_consume::<crate::nav::Router>().map(|router| router.use_current_id());
        use_side_effect_with_deps(&page, move |_| overlays.forget_menu());

        // Every application window answers the About shortcut: the main window
        // of an app that went through `Application::run` (which registered its
        // facts) and has a router. A gallery, a test host and the inspector's
        // window have none of the two. A dialog that is already open keeps the
        // window: it holds a question.
        let about = page.is_some() && crate::about::registered();
        rect()
            .expanded()
            .maybe(about, |host| {
                host.on_global_key_down(move |event: Event<KeyboardEventData>| {
                    if crate::about::is_shortcut(&event) && !overlays.dialog_open() {
                        event.stop_propagation();
                        crate::about::open(overlays);
                    }
                })
            })
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
    /// The entry of the window's stack the window's menu must still be on. A
    /// popup has no stack, so its menu is never filtered by it.
    pub(crate) page: Option<u64>,
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

/// How far a finger drags a toast sideways before letting go dismisses it.
const SWIPE_DISMISS: f32 = 72.0;
/// The sideways drag at which a toast has faded out completely.
const SWIPE_FADE: f32 = 160.0;
/// How often a toast's timer looks at whether it is held.
const TOAST_TICK: Duration = Duration::from_millis(100);

/// The hide clock of one toast, shared by its handlers and its timer task.
type SharedClock = Rc<RefCell<HideClock>>;

/// The floating shell of a toast: places the [`Banner`] it holds at the
/// bottom of the window above the content, slides and fades it in and out,
/// runs its [`HideClock`] and dismisses it on a sideways swipe. It draws
/// nothing of its own: the pill is the banner.
///
/// The clock is held while the pointer is anywhere over the toast (the shell
/// listens for the pointer over its whole box, not only where nothing else
/// listens: the action and dismiss buttons are inside it and take nothing
/// away), while a finger is down on it and while one of its buttons has
/// keyboard focus. When the last of those lets go the full delay starts again.
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
        let timeout = self.toast.timeout_or_default();
        let clock: SharedClock = use_hook(move || Rc::new(RefCell::new(HideClock::new(timeout))));
        // Where a finger went down, and how far it has dragged since.
        let mut grab = use_state(|| None::<f32>);
        let mut pull = use_state(|| 0.0_f32);
        // One id per action and one for the dismiss cross.
        let buttons = self.toast.banner.action_count() + 1;
        let button_ids =
            use_hook(move || (0..buttons).map(|_| AccessibilityId::new_unique()).collect::<Vec<_>>());
        let progress = use_presence(!self.closing, tokens.motion(140), move || {
            overlays.finish_toast(id)
        });
        let dx = use_sheet_pull(pull, grab().is_some(), tokens.motion(140));
        let focused = Platform::get().current_window().focused_accessibility_id;
        // Each toast that takes over the layer gets its own timer. It counts
        // down only while nothing holds the toast, and the clock says when.
        let timer_ids = button_ids.clone();
        let timer_clock = clock.clone();
        use_side_effect_with_deps(&id, move |&id: &u64| {
            let ids = timer_ids.clone();
            let clock = timer_clock.clone();
            spawn(async move {
                let mut last = Instant::now();
                loop {
                    timer(TOAST_TICK).await;
                    let now = Instant::now();
                    let elapsed = now.saturating_duration_since(last);
                    last = now;
                    let focus_held = ids.contains(&*focused.peek());
                    let mut clock = clock.borrow_mut();
                    clock.set_focus(focus_held);
                    if clock.tick(elapsed) {
                        break;
                    }
                }
                overlays.dismiss_toast(id);
            });
        });
        let dismiss = EventHandler::new(move |_: ()| overlays.dismiss_toast(id));
        let banner = self
            .toast
            .banner
            .clone()
            .after_action(dismiss.clone())
            .dismissible(dismiss)
            .button_ids(button_ids);
        let fade = progress * (1.0 - (dx.abs() / SWIPE_FADE).min(1.0));

        // `offset_y` shifts an element's children, not the element: the toast
        // slides inside a wrapper, so its background moves with its text.
        rect()
            .position(
                Position::new_absolute().bottom(S6.px() + Dim::SafeBottom.px()).left(0.0),
            )
            .layer(self.layer)
            .tw(&tokens, &[w_screen(), flex_col(), items(Align::Center)])
            .child(
                rect()
                    .component("Toast")
                    .offset_y(S2.px() * (1.0 - progress))
                    .offset_x(dx)
                    .opacity(fade)
                    // Over and out, not enter and leave: those two belong to
                    // the deepest element that listens, so the pointer on the
                    // toast's own button would count as having left it.
                    .on_pointer_over({
                        let clock = clock.clone();
                        move |event: Event<PointerEventData>| {
                            // A finger holds the toast while it is down (below);
                            // it has no pointer that rests there afterwards.
                            if !matches!(event.data(), PointerEventData::Touch(_)) {
                                clock.borrow_mut().set_pointer(true);
                            }
                        }
                    })
                    .on_pointer_out({
                        let clock = clock.clone();
                        move |_| clock.borrow_mut().set_pointer(false)
                    })
                    .on_pointer_down({
                        let clock = clock.clone();
                        move |event: Event<PointerEventData>| {
                            if matches!(event.data(), PointerEventData::Touch(_)) {
                                pull.set(0.0);
                                grab.set(Some(event.global_location().x as f32));
                                clock.borrow_mut().set_touch(true);
                            }
                        }
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        if let Some(start) = *grab.peek() {
                            pull.set(event.global_location().x as f32 - start);
                        }
                    })
                    .on_global_pointer_up({
                        let clock = clock.clone();
                        move |_| {
                            if grab.peek().is_none() {
                                return;
                            }
                            grab.set(None);
                            clock.borrow_mut().set_touch(false);
                            // A long enough swipe dismisses and leaves the toast
                            // where the finger let go; a short one springs back.
                            if pull.peek().abs() >= SWIPE_DISMISS {
                                overlays.dismiss_toast(id);
                            } else {
                                pull.set(0.0);
                            }
                        }
                    })
                    .child(banner),
            )
    }
}
