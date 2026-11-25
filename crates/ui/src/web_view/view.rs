//! The web view control: one page of the web, drawn and driven like any other
//! control. It is IE's WebBrowser control for Bingux: the page's frames come
//! from the engine through a [`PageSurface`] into a [frame view](crate::frame_view),
//! and everything the person does over it (pointer, wheel, keys, text and
//! composition, fingers, focus) goes back to the engine as a [`PageInput`].
//!
//! The host (the app that embeds the view) shapes it the way IE's
//! `IDocHostUIHandler` did, with three hooks:
//!
//! | Hook | Called when | Default |
//! | ---- | ----------- | ------- |
//! | [`WebView::on_host_key`] | a key goes down over the page; return `true` to take it (`TranslateAccelerator`) | every key goes to the page |
//! | [`WebView::on_resize`] | the view's size or scale changes | the engine is told the new size |
//! | [`WebView::on_local_menu`] | a right click on an engine that cannot say what is under the pointer | nothing |
//!
//! Input goes straight to the engine, not through the host's state: a pointer
//! move is not news to the rest of the window.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use freya::prelude::*;

use crate::frame_view::{
    FocusEscape, FrameSource, KeyInput, PointerAction, PointerInput, TextInput, TouchInput, ViewSize, WheelInput,
    frame_view,
};
use crate::prelude::*;
use crate::web_view::backend::BrowserBackend;
use crate::web_view::ids::TabId;
use crate::web_view::input::{FocusAction, Mods, PageInput, TouchKind};
use crate::web_view::surface::PageSurface;
use crate::web_view::touch::{LONG_PRESS, TouchScroller};

/// The page of one engine session, as a control. Build it with [`web_view`].
pub struct WebView {
    backend: Arc<dyn BrowserBackend>,
    tab: TabId,
    surface: Arc<PageSurface>,
    label: String,
    host_key: Option<Callback<KeyInput, bool>>,
    resize: Option<EventHandler<ViewSize>>,
    local_menu: Option<EventHandler<(f32, f32)>>,
}

impl PartialEq for WebView {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.backend, &other.backend)
            && self.tab == other.tab
            && Arc::ptr_eq(&self.surface, &other.surface)
            && self.label == other.label
            && self.host_key == other.host_key
            && self.resize == other.resize
            && self.local_menu == other.local_menu
    }
}

/// The page of `tab`, drawn from `surface` and driven through `backend`.
pub fn web_view(backend: Arc<dyn BrowserBackend>, tab: TabId, surface: Arc<PageSurface>) -> WebView {
    WebView { backend, tab, surface, label: "Web page".into(), host_key: None, resize: None, local_menu: None }
}

impl WebView {
    /// What assistive technology calls the page area ("Web page" by default).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Keys the host takes before the page sees them: return `true` to keep
    /// the key (its release is then the host's too). A browser takes its own
    /// shortcuts here, so a page cannot take Ctrl+T.
    pub fn on_host_key(mut self, handler: impl FnMut(KeyInput) -> bool + 'static) -> Self {
        self.host_key = Some(Callback::new(handler));
        self
    }

    /// The view's size or scale changed. Without a handler the engine is told
    /// the new size directly; a host with a handler tells it itself (a browser
    /// sizes every tab, not only the one shown).
    pub fn on_resize(mut self, handler: impl Into<EventHandler<ViewSize>>) -> Self {
        self.resize = Some(handler.into());
        self
    }

    /// A right click on an engine without page context menus
    /// (`Capabilities::context_menus` is off), at the pointer's place in the
    /// view, after the page has seen the click: the host shows a menu of its own.
    pub fn on_local_menu(mut self, handler: impl Into<EventHandler<(f32, f32)>>) -> Self {
        self.local_menu = Some(handler.into());
        self
    }
}

/// The key that opens the page's context menu from the keyboard.
fn is_menu_key(input: &KeyInput) -> bool {
    input.key_string() == "ContextMenu" || (input.key_string() == "F10" && input.modifiers.contains(Modifiers::SHIFT))
}

impl Component for WebView {
    fn render(&self) -> impl IntoElement {
        let backend = self.backend.clone();
        let tab = self.tab;
        let capabilities = backend.capabilities();
        // The keys the host took when they went down: their release is not the
        // page's either.
        let taken = use_hook(|| Rc::new(RefCell::new(HashSet::<String>::new())));
        let fingers = use_hook(|| Rc::new(RefCell::new(TouchScroller::new())));
        let send = {
            let backend = backend.clone();
            move |input: PageInput| {
                // A page that is gone has nothing to hear it.
                let _ = backend.send(tab, input);
            }
        };

        let resize = self.resize.clone();
        let local_menu = self.local_menu.clone();
        let host_key = self.host_key.clone();
        let mut view = frame_view(self.surface.clone() as Arc<dyn FrameSource>, self.label.clone())
            .role(AccessibilityRole::Document)
            .radius(Radius::Mark)
            .focus_escape(FocusEscape::Producer)
            .producer_draws_focus()
            .placeholder("Loading")
            .on_resize({
                let backend = backend.clone();
                move |size: ViewSize| match &resize {
                    Some(handler) => handler.call(size),
                    None => {
                        let _ = backend.set_size(
                            tab,
                            size.width.round() as u32,
                            size.height.round() as u32,
                            size.scale as f32,
                        );
                    }
                }
            })
            .on_pointer({
                let send = send.clone();
                move |input: PointerInput| {
                    let menu = !capabilities.context_menus
                        && input.action == PointerAction::Up
                        && input.button == Some(MouseButton::Right);
                    let position = input.position;
                    if let Some(input) = PageInput::from_pointer(&input) {
                        send(input);
                    }
                    if menu && let Some(handler) = &local_menu {
                        handler.call(position);
                    }
                }
            })
            .on_wheel({
                let send = send.clone();
                move |input: WheelInput| send(PageInput::from_wheel(&input))
            })
            .on_key({
                let send = send.clone();
                let taken = taken.clone();
                move |input: KeyInput| {
                    let code = input.code_string();
                    if input.pressed {
                        if let Some(host) = &host_key
                            && host.call(input.clone())
                        {
                            taken.borrow_mut().insert(code);
                            return;
                        }
                        if is_menu_key(&input) {
                            taken.borrow_mut().insert(code);
                            send(PageInput::ContextMenu { at: None, modifiers: Mods::from_freya(input.modifiers) });
                            return;
                        }
                    } else if taken.borrow_mut().remove(&code) {
                        return;
                    }
                    send(PageInput::from_key(&input));
                }
            })
            .on_text({
                let send = send.clone();
                move |input: TextInput| {
                    // An engine that cannot compose shows only what is finished.
                    if matches!(input, TextInput::Preedit { .. }) && !capabilities.composition {
                        return;
                    }
                    send(PageInput::from_text(&input));
                }
            })
            .on_focus({
                let send = send.clone();
                move |focused: bool| {
                    send(PageInput::Focus(if focused { FocusAction::Activate } else { FocusAction::Deactivate }));
                }
            });
        if capabilities.touch {
            // Fingers as they are: the engine does the tapping and the scrolling.
            view = view.on_touch({
                let send = send.clone();
                move |input: TouchInput| send(PageInput::from_touch(&input))
            });
        } else {
            // Fingers as a mouse would be: a tap clicks, a drag scrolls, a held
            // finger asks for the page's menu.
            view = view.on_touch({
                let send = send.clone();
                let fingers = fingers.clone();
                move |input: TouchInput| {
                    let PageInput::Touch(event) = PageInput::from_touch(&input) else {
                        return;
                    };
                    let now = Instant::now();
                    for out in fingers.borrow_mut().touch(event, now) {
                        send(out);
                    }
                    if event.kind == TouchKind::Down {
                        // The held press is asked about once the time has passed.
                        let fingers = fingers.clone();
                        let send = send.clone();
                        spawn(async move {
                            timer(LONG_PRESS).await;
                            if let Some(out) = fingers.borrow_mut().held(Instant::now()) {
                                send(out);
                            }
                        });
                    }
                }
            });
        }
        // The page fills the area; its box does not depend on the frame.
        rect().tw_layout(&[w_full(), h_flex()]).child(view)
    }
}
