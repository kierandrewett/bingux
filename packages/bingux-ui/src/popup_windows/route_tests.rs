//! The route a `Panel` takes: with a popup backend a context menu is handed to it and not drawn
//! in its window; without one (the headless path every other test runs) it is drawn in the
//! window as before.

use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;

use super::backend::{Call, Parent};
use super::{FakeBackend, reset, set_backend};
use crate::shell::menu::{menu_item, menu_separator};
use crate::shell::overlays::{OverlayHost, Overlays};
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _ = provide_tokens();
        WindowHost::new(OverlayHost::new(Page))
    }
}

#[derive(PartialEq)]
struct Page;

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let _ = crate::surface::use_tokens();
        let overlays = try_consume_context::<Overlays>();
        rect()
            .expanded()
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let right = matches!(
                    event.data(),
                    PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
                );
                let (Some(overlays), true) = (overlays, right) else {
                    return;
                };
                let at = event.global_location();
                overlays.show_menu(
                    (at.x as f32, at.y as f32),
                    vec![
                        menu_item("Copy the whole element", |_| {}),
                        menu_separator(),
                        menu_item("Properties", |_| {}),
                    ],
                );
            })
    }
}

fn properties_label_in_window(shot: &mut Snapshot) -> bool {
    !shot
        .runner()
        .find_many(|_, element| {
            let label = Label::try_downcast(element)?;
            (label.text == "Properties").then_some(())
        })
        .is_empty()
}

#[test]
fn a_context_menu_goes_to_the_popup_backend_and_not_into_its_window() {
    reset();
    let fake = Rc::new(FakeBackend::default());
    set_backend(Some(fake.clone()));
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    assert!(fake.calls().is_empty(), "nothing opens before someone asks");

    shot.right_click(300.0, 200.0);
    shot.settle(Duration::from_millis(300));
    let calls = fake.calls();
    let Some(Call::Open { placement, parent, .. }) = calls.first() else {
        panic!("the menu was not handed to the backend: {calls:?}");
    };
    assert!(matches!(parent, Parent::Window(_)), "a menu is a popup of its window");
    assert_eq!((placement.anchor.x, placement.anchor.y), (300.0, 200.0));
    assert!(placement.size.0 > 100.0 && placement.size.1 > 50.0, "{:?}", placement.size);
    assert!(
        !properties_label_in_window(&mut shot),
        "the card is the popup's, it is not drawn in the window"
    );
    reset();
}

#[test]
fn a_menu_the_compositor_dismissed_can_be_opened_again() {
    reset();
    let fake = Rc::new(FakeBackend::default());
    set_backend(Some(fake.clone()));
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    shot.right_click(100.0, 100.0);
    shot.settle(Duration::from_millis(300));
    let first = *fake.open_tokens().first().expect("the menu opened");
    fake.dismiss(first);
    shot.settle(Duration::from_millis(400));
    shot.right_click(200.0, 150.0);
    shot.settle(Duration::from_millis(300));
    let opens = fake.calls().iter().filter(|call| matches!(call, Call::Open { .. })).count();
    assert_eq!(opens, 2, "{:?}", fake.calls());
    reset();
}

#[test]
fn a_refused_popup_draws_the_menu_in_the_window() {
    reset();
    let fake = Rc::new(FakeBackend::default());
    fake.refuse_next(true);
    set_backend(Some(fake.clone()));
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    shot.right_click(300.0, 200.0);
    shot.settle(Duration::from_millis(400));
    assert!(properties_label_in_window(&mut shot), "a refused popup falls back to the window");
    reset();
}

#[test]
fn without_a_backend_the_menu_is_drawn_in_its_window() {
    reset();
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    shot.right_click(300.0, 200.0);
    shot.settle(Duration::from_millis(300));
    assert!(properties_label_in_window(&mut shot));
}

// ---- Stacking, with the real backend code paths and the fake compositor ----

mod stacking {
    use super::*;
    use crate::layers::{WindowKind, with_stack};
    use crate::popup_windows::backend::{OpenRequest, PopupBackend, stack_id};
    use crate::popup_windows::place::{Adjust, Edge, PopupRequest};
    use crate::popup_windows::PopupContent;
    use crate::shell::panel::PanelRect;
    use freya::winit::window::WindowId;

    fn fresh() {
        reset();
        with_stack(|stack| *stack = crate::layers::WindowStack::new());
    }

    fn open(fake: &FakeBackend, parent: Parent) -> u64 {
        fake.open(OpenRequest {
            placement: PopupRequest {
                anchor: PanelRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                edge: Edge::TopLeft,
                gravity: Edge::BottomRight,
                adjust: Adjust::ALL,
                size: (100.0, 100.0),
            },
            parent,
            content: PopupContent::new(rect().into_element()),
            contexts: ContextSnapshot::default(),
            origin: (0.0, 0.0),
            on_event: Rc::new(|_| {}),
        })
    }

    #[test]
    fn a_submenu_chain_stacks_in_order_and_closes_newest_first() {
        fresh();
        let fake = FakeBackend::default();
        let window = WindowId::from(7_u64);
        let menu = open(&fake, Parent::Window(window));
        let sub = open(&fake, Parent::Popup(menu));
        let subsub = open(&fake, Parent::Popup(sub));
        assert_eq!(
            with_stack(|stack| stack.ids()),
            vec![7, stack_id(menu), stack_id(sub), stack_id(subsub)]
        );
        fake.clear_calls();
        fake.close(menu);
        let closed: Vec<_> = fake
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Close { token } => Some(token),
                _ => None,
            })
            .collect();
        assert_eq!(closed, vec![subsub, sub, menu], "a popup is destroyed before its parent");
        assert_eq!(with_stack(|stack| stack.ids()), vec![7], "closing returns to the window");
    }

    #[test]
    fn a_menu_is_above_its_window_and_a_dialog_opened_after_it_stays_below_it() {
        fresh();
        let fake = FakeBackend::default();
        let window = WindowId::from(7_u64);
        let menu = open(&fake, Parent::Window(window));
        // The app opens a modal dialog from the window while the menu is up: the menu is above
        // everything of its app, the dialog above the window.
        with_stack(|stack| stack.open(900, WindowKind::Dialog, Some(7), true, "About"));
        assert_eq!(with_stack(|stack| stack.ids()), vec![7, 900, stack_id(menu)]);
        assert!(with_stack(|stack| stack.blocked(7)));
        fake.close(menu);
        assert_eq!(with_stack(|stack| stack.ids()), vec![7, 900]);
    }

    #[test]
    fn a_popup_from_a_dialog_is_above_the_dialog_and_a_chooser_over_it_is_above_both() {
        fresh();
        let fake = FakeBackend::default();
        with_stack(|stack| {
            stack.open(7, WindowKind::Toplevel, None, false, "Files");
            stack.open(900, WindowKind::Dialog, Some(7), true, "Preferences");
        });
        let dropdown = open(&fake, Parent::Window(WindowId::from(900_u64)));
        with_stack(|stack| stack.open(901, WindowKind::Dialog, Some(900), true, "Choose a file"));
        // The chooser is a dialog of the dialog: above it, below the dropdown the dialog opened.
        assert_eq!(with_stack(|stack| stack.ids()), vec![7, 900, 901, stack_id(dropdown)]);
        fake.close(dropdown);
        with_stack(|stack| stack.close(901));
        assert_eq!(with_stack(|stack| stack.ids()), vec![7, 900]);
    }

    #[test]
    fn focusing_the_parent_raises_the_windows_open_from_it() {
        fresh();
        let fake = FakeBackend::default();
        with_stack(|stack| {
            stack.open(7, WindowKind::Toplevel, None, false, "Files");
            stack.open(8, WindowKind::Toplevel, None, false, "Inspector");
        });
        let menu = open(&fake, Parent::Window(WindowId::from(7_u64)));
        with_stack(|stack| stack.raise(7));
        assert_eq!(with_stack(|stack| stack.ids()), vec![8, 7, stack_id(menu)]);
        // The inspector's own popup sits above the inspector, not above Files.
        let other = open(&fake, Parent::Window(WindowId::from(8_u64)));
        let ids = with_stack(|stack| stack.ids());
        let at = |id: u64| ids.iter().position(|i| *i == id).unwrap();
        assert!(at(stack_id(other)) > at(8) && at(stack_id(menu)) > at(7));
    }

    #[test]
    fn a_refused_popup_is_never_in_the_stack() {
        fresh();
        let fake = FakeBackend::default();
        fake.refuse_next(true);
        let _ = open(&fake, Parent::Window(WindowId::from(7_u64)));
        assert!(with_stack(|stack| stack.ids()).iter().all(|id| *id == 7));
        reset();
    }

    #[derive(PartialEq)]
    struct SubWindowRoot;

    impl Component for SubWindowRoot {
        fn render(&self) -> impl IntoElement {
            let _ = provide_tokens();
            WindowHost::new(OverlayHost::new(SubWindowPage))
        }
    }

    #[derive(PartialEq)]
    struct SubWindowPage;

    impl Component for SubWindowPage {
        fn render(&self) -> impl IntoElement {
            let _ = crate::surface::use_tokens();
            let window = crate::window::use_window();
            rect().expanded().on_press(move |_: Event<PressEventData>| {
                use crate::window::WindowParent;
                // ui-lint: allow(test fixture: a sub-window body of a fixed size)
                let _ = window.open(crate::window::popup("Sub-window", rect().width(Size::px(40.0)).height(Size::px(40.0))));
            })
        }
    }

    #[test]
    fn an_in_window_sub_window_is_a_modal_child_of_its_window_in_the_stack() {
        fresh();
        let mut shot = Snapshot::new(|| SubWindowRoot.into_element(), (800.0, 600.0));
        shot.settle(Duration::from_millis(300));
        shot.click(100.0, 100.0);
        shot.settle(Duration::from_millis(500));
        let dump = crate::layers::dump_stack();
        let lines: Vec<&str> = dump.lines().collect();
        assert!(lines.len() >= 2, "{dump}");
        assert!(lines[0].contains("Sub-window") && lines[0].contains("Dialog") && lines[0].contains("modal=true"), "{dump}");
        assert!(lines[1].contains("Toplevel"), "{dump}");
        shot.key(Key::Named(NamedKey::Escape));
        shot.settle(Duration::from_millis(800));
        assert!(
            !crate::layers::dump_stack().contains("Sub-window"),
            "closing the sub-window takes it out of the stack: {}",
            crate::layers::dump_stack()
        );
    }
}
