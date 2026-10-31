//! The inspector window: the shell's header bar with the tabs, and under it the
//! tab's body. Elements is the application's element tree beside the selected
//! node's properties; every pane scrolls on its own.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;

use super::details::DetailsPane;
use super::dom_tree::{DomTree, Shot, ancestors};
use super::nav::{Tab, provide_nav, use_nav};
use super::snapshot::Snapshot;
use super::target::InstanceId;
use crate::controls::{SpinnerSize, spinner, split_pane};
use crate::icons::Icon;
use crate::shell::overlays::{OverlayHost, Overlays};
use crate::shell::status_page;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{Tokens, provide_tokens};

/// Below this width the tabs scroll sideways and the tree and the properties
/// stack, as their columns have no room side by side.
const NARROW_BELOW: f32 = 760.0;

/// The window's root. Installs the tokens and the body below them.
pub fn root(id: InstanceId) -> Element {
    InspectorWindow { id }.into()
}

#[derive(PartialEq)]
struct InspectorWindow {
    id: InstanceId,
}

impl Component for InspectorWindow {
    fn render(&self) -> impl IntoElement {
        // The inspector is a fixed Regular, mouse-driven window. Its OverlayHost
        // puts a WindowHost under this component, which sets the density from the
        // window's width and the pointer, so a narrow inspector would read
        // Compact. A forced pair wins over both; a device preview on the
        // application window never reaches this window, which has its own key.
        use_hook(|| {
            force_density(
                Platform::get().current_window().id,
                Some((SizeClass::Regular, PointerKind::Mouse)),
            );
        });
        // Bind before anything below renders: the components find their
        // instance, and so the window they watch, by this window.
        super::bind_current(self.id);
        let tokens = provide_tokens();
        provide_nav();
        super::set_tokens(tokens);
        rect()
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
            .child(OverlayHost::new(InspectorBody { id: self.id }))
    }
}

#[derive(PartialEq)]
struct InspectorBody {
    id: InstanceId,
}

impl Component for InspectorBody {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let nav = use_nav();
        let expanded = use_state::<HashSet<u64>>(HashSet::new);
        let head_seen = use_state::<Option<u64>>(|| None);
        let wrappers = use_state(|| false);
        let ratio = use_state(|| 0.5f32);
        let mut version = use_state(|| 0u64);
        let mut snapshot = use_state::<Option<Rc<Snapshot>>>(|| None);
        let mut inspect_on = use_state(super::picking);
        let mut paused_on = use_state(super::paused);
        let overlays = try_consume_context::<Overlays>();

        // Re-read the snapshot whenever the plugin has taken a new one.
        use_hook(move || {
            spawn(async move {
                let mut nav = nav;
                let mut expanded = expanded;
                let mut head_seen = head_seen;
                loop {
                    timer(Duration::from_millis(200)).await;
                    if let Some(message) = super::take_notice()
                        && let Some(overlays) = overlays
                    {
                        overlays.toast(message);
                    }
                    super::catch_up();
                    if let Some(node) = super::take_pick() {
                        nav.reveal(node);
                    }
                    let (current, next) = super::snapshot();
                    if current != *version.peek() {
                        version.set(current);
                        // Select the app's root on the first snapshot, as Chrome selects body,
                        // and open it with its children so the tree starts two levels deep.
                        if nav.selected.peek().is_none() {
                            if let Some(snap) = next.as_ref() {
                                let roots = snap.roots.clone();
                                let mut open = expanded.write();
                                for root in &roots {
                                    open.insert(*root);
                                    if let Some(node) = snap.node(*root) {
                                        open.extend(node.children.iter().copied());
                                    }
                                }
                                nav.selected.set(roots.first().copied());
                            }
                        }
                        // Open the way to the page heading, so the page is in view without
                        // digging through the window chrome. The first snapshot can be
                        // transient and a page rebuild gives its heading a new id, so this
                        // follows the heading's id rather than running once.
                        if let Some(snap) = next.as_ref() {
                            let head = snap
                                .nodes
                                .iter()
                                .find(|node| node.a11y.component.as_deref() == Some("PageHead"))
                                .map(|node| node.id);
                            if let Some(head) = head.filter(|&id| *head_seen.peek() != Some(id)) {
                                head_seen.set(Some(head));
                                let mut open = expanded.write();
                                open.extend(
                                    ancestors(snap, head).into_iter().filter(|&id| id != head),
                                );
                            }
                        }
                        snapshot.set(next);
                    }
                    let picking = super::picking();
                    if *inspect_on.peek() != picking {
                        inspect_on.set(picking);
                    }
                    let frozen = super::paused();
                    if *paused_on.peek() != frozen {
                        paused_on.set(frozen);
                    }
                }
            });
        });
        let tab = *nav.tab.read();
        let narrow = crate::surface::window_size().width < NARROW_BELOW;
        let header =
            super::toolbar::header(tab, nav.tab, *inspect_on.read(), *paused_on.read(), narrow);
        let body = match tab {
            Tab::Elements => elements(
                &tokens,
                nav.selected,
                expanded,
                wrappers,
                ratio,
                snapshot.read().clone(),
                narrow,
            ),
            Tab::Performance => super::perf::view(self.id),
            Tab::Accessibility => super::a11y::view(&tokens, &super::issues(), nav),
            Tab::Layout => super::layout_tab::view(&super::layout_findings(), nav),
        };
        let root: Element = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    flex_col(),
                    grow_children(),
                    bg(Paint::Bg),
                ],
            )
            .on_pointer_enter(|_| {
                // Keyboard focus follows the pointer into an inspector window.
                let window = Platform::window_id();
                if Platform::get()
                    .try_window(u64::from(window))
                    .is_some_and(|w| !*w.is_app_focused.peek())
                {
                    Platform::get().focus_window(window);
                }
            })
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let control = event.modifiers == Modifiers::CONTROL;
                if event.key == Key::Named(NamedKey::F5) {
                    super::refresh();
                    return;
                }
                if !control {
                    return;
                }
                let mut tab_state = nav.tab;
                // Ctrl+[ steps back a tab, Ctrl+] forward, wrapping around;
                // Ctrl+1 to Ctrl+3 jump to a tab.
                if event.key == Key::Character("[".into())
                    || event.key == Key::Character("]".into())
                {
                    let step = if event.key == Key::Character("[".into()) {
                        -1
                    } else {
                        1
                    };
                    let count = Tab::ALL.len() as isize;
                    let current = Tab::ALL
                        .iter()
                        .position(|tab| *tab == *nav.tab.peek())
                        .unwrap_or(0) as isize;
                    tab_state.set(Tab::ALL[(current + step).rem_euclid(count) as usize]);
                } else if let Key::Character(text) = &event.key
                    && let Some(index) = text.parse::<usize>().ok().and_then(|n| n.checked_sub(1))
                    && let Some(target) = Tab::ALL.get(index)
                {
                    tab_state.set(*target);
                }
            })
            .child(header)
            .child(rect().tw(&tokens, &[w_full(), h_flex()]).child(body))
            .into();
        root
    }
}

/// The Elements tab: the tree beside the selected node's properties.
fn elements(
    tokens: &Tokens,
    selected: State<Option<u64>>,
    expanded: State<HashSet<u64>>,
    wrappers: State<bool>,
    ratio: State<f32>,
    snap: Option<Rc<Snapshot>>,
    narrow: bool,
) -> Element {
    let Some(snap) = snap else {
        return status_page(Icon::Inspect, "Waiting for an application window")
            .description(
                "The tree appears as soon as the application has drawn. \
                 Open the inspector from an application with Ctrl+Alt+Shift+I.",
            )
            .child(spinner(SpinnerSize::Medium))
            .into();
    };
    let tree = rect()
        .tw(tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
        .child(DomTree {
            snap: Shot(snap.clone()),
            selected,
            expanded,
            wrappers,
        });
    let chosen = *selected.read();
    let details: Element = match (chosen, super::app_window()) {
        (Some(id), Some(window)) if snap.node(id).is_some() => DetailsPane {
            snap: snap.clone(),
            id,
            window,
        }
        .into(),
        _ => status_page(Icon::Inspect, "Select an element")
            .description(
                "Pick a row in the tree, or press the inspect button and click the application.",
            )
            .into(),
    };
    let pane = split_pane(tree, details, ratio);
    let pane = if narrow { pane.vertical() } else { pane };
    rect()
        .tw(tokens, &[w_full(), h_full(), flex_row(), grow_children()])
        .child(pane)
        .into()
}
