//! The page: the engine's frames in a frame view, and everything the person
//! does over it sent back to the engine.
//!
//! Input goes straight to the backend, not through the browser's state: a
//! pointer move is not news to the rest of the window, and writing state for
//! every move would redraw the whole chrome. Only what changes the chrome (a
//! shortcut, a resize) goes through [`Ctx::with`].

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use bingux_geckoview::input::{FocusAction, Mods, PageInput, TouchKind};
use bingux_geckoview::touch::TouchScroller;
use bingux_geckoview::{BrowserBackend, PageSurface, TabId};
use bingux_ui::frame_view::{FocusEscape, FrameSource, KeyInput, PointerAction, PointerInput, TextInput, TouchInput, ViewSize, WheelInput, frame_view};
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::model::shortcuts::page_command;

use super::ctx::{Ctx, Engine};

/// A surface, comparable by identity so it can be a prop.
#[derive(Clone)]
pub struct Surface(pub Arc<PageSurface>);

impl PartialEq for Surface {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The page of one tab.
#[derive(PartialEq)]
pub struct WebView {
    pub ctx: Ctx,
    pub engine: Engine,
    pub key: u64,
    pub session: TabId,
    pub surface: Surface,
}

/// The key that opens the page's context menu from the keyboard.
fn is_menu_key(input: &KeyInput) -> bool {
    input.key_string() == "ContextMenu"
        || (input.key_string() == "F10" && input.modifiers.contains(Modifiers::SHIFT))
}

impl Component for WebView {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let backend: Arc<dyn BrowserBackend> = self.engine.0.clone();
        let session = self.session;
        let capabilities = backend.capabilities();
        // The keys a shortcut took when they went down: their release is not
        // the page's either.
        let taken = use_hook(|| Rc::new(RefCell::new(HashSet::<String>::new())));
        let fingers = use_hook(|| Rc::new(RefCell::new(TouchScroller::new())));
        let send = {
            let backend = backend.clone();
            move |input: PageInput| {
                // A page that is gone has nothing to hear it.
                let _ = backend.send(session, input);
            }
        };

        let mut view = frame_view(self.surface.0.clone() as Arc<dyn FrameSource>, "Web page")
            .role(AccessibilityRole::Document)
            .radius(Radius::Mark)
            .focus_escape(FocusEscape::Producer)
            .producer_draws_focus()
            .placeholder("Loading")
            .on_resize(move |size: ViewSize| {
                ctx.with(|browser| browser.resize(size.width.round() as u32, size.height.round() as u32, size.scale as f32));
            })
            .on_pointer({
                let send = send.clone();
                let key = self.key;
                move |input: PointerInput| {
                    // An engine that cannot say what is under the pointer gets a menu
                    // of the browser's own once the page has seen the click.
                    let menu = !capabilities.context_menus
                        && input.action == PointerAction::Up
                        && input.button == Some(MouseButton::Right);
                    let position = input.position;
                    if let Some(input) = PageInput::from_pointer(&input) {
                        send(input);
                    }
                    if menu {
                        ctx.with(|browser| browser.open_local_menu(key, position));
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
                        // Escape leaves a fullscreen page, whatever the page says.
                        if input.key == Key::Named(NamedKey::Escape) && ctx.browser.peek().fullscreen {
                            taken.borrow_mut().insert(code);
                            ctx.with(|browser| browser.run(crate::model::shortcuts::Command::Fullscreen));
                            return;
                        }
                        // The browser's shortcuts come first: a page cannot take
                        // Ctrl+T, and Ctrl+C is the engine's own copy.
                        if let Some(command) = page_command(ctx.platform, &input.key, input.modifiers) {
                            taken.borrow_mut().insert(code);
                            ctx.with(|browser| browser.run(command));
                            return;
                        }
                        if is_menu_key(&input) {
                            taken.borrow_mut().insert(code);
                            send(PageInput::ContextMenu {
                                at: None,
                                modifiers: Mods::from_freya(input.modifiers),
                            });
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
                            timer(bingux_geckoview::touch::LONG_PRESS).await;
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
