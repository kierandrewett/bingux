//! The inspector window: the application's element tree on the left and the
//! selected node's properties on the right.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;

use super::dom_tree::{DomTree, Shot};
use super::nav::{Tab, provide_nav, use_nav};
use super::snapshot::Snapshot;
use super::styles::StylesPane;
use crate::controls::split_pane;
use crate::shell::overlays::OverlayHost;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{Tokens, provide_tokens};
use crate::typography::{CAPTION, styled_label};

/// The window's root. Installs the tokens and the body below them.
pub fn root() -> Element {
    InspectorWindow.into()
}

#[derive(PartialEq)]
struct InspectorWindow;

impl Component for InspectorWindow {
    fn render(&self) -> impl IntoElement {
        // The inspector is a fixed Regular, mouse-driven window. It has no
        // WindowHost, so this provides the density its tree reads, and a device
        // preview on the application window never reaches it.
        use_provide_context(|| WindowDensity::new(SizeClass::Regular, PointerKind::Mouse));
        let tokens = provide_tokens();
        provide_nav();
        super::set_tokens(tokens);
        rect()
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
            .child(OverlayHost::new(InspectorBody))
    }
}

#[derive(PartialEq)]
struct InspectorBody;

impl Component for InspectorBody {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let nav = use_nav();
        let expanded = use_state::<HashSet<u64>>(HashSet::new);
        let ratio = use_state(|| 0.58f32);
        let mut version = use_state(|| 0u64);
        let mut snapshot = use_state::<Option<Rc<Snapshot>>>(|| None);
        let mut inspect_on = use_state(super::picking);
        let mut paused_on = use_state(super::paused);

        // Re-read the snapshot whenever the plugin has taken a new one.
        use_hook(move || {
            spawn(async move {
                let mut nav = nav;
                let mut expanded = expanded;
                loop {
                    timer(Duration::from_millis(200)).await;
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
        let tab_state = nav.tab;
        let header = super::toolbar::toolbar(
            &tokens,
            tab,
            tab_state,
            *inspect_on.read(),
            *paused_on.read(),
        );
        let body = match tab {
            Tab::Elements => elements(
                &tokens,
                nav.selected,
                expanded,
                ratio,
                snapshot.read().clone(),
            ),
            Tab::Performance => super::perf::view(),
            Tab::Accessibility => super::a11y::view(&tokens, &super::issues(), nav),
        };
        let root: Element = rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), grow_children(), bg(Paint::Bg)])
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let control = event.modifiers == Modifiers::CONTROL;
                if event.key == Key::Named(NamedKey::F5) {
                    super::refresh();
                } else if control
                    && (event.key == Key::Character("[".into())
                        || event.key == Key::Character("]".into()))
                {
                    // Ctrl+[ steps back a tab, Ctrl+] forward, wrapping around.
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
                    let next = (current + step).rem_euclid(count) as usize;
                    let mut tab_state = nav.tab;
                    tab_state.set(Tab::ALL[next]);
                }
            })
            .child(header)
            .child(body)
            .into();
        root
    }
}

/// The Elements tab: the tree beside the selected node's properties.
fn elements(
    tokens: &Tokens,
    selected: State<Option<u64>>,
    expanded: State<HashSet<u64>>,
    ratio: State<f32>,
    snap: Option<Rc<Snapshot>>,
) -> Element {
    let Some(snap) = snap else {
        return centred(tokens, "Waiting for an application window");
    };
    let tree = rect()
        .tw(tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
        .child(DomTree {
            snap: Shot(snap.clone()),
            selected,
            expanded,
        });
    let chosen = *selected.read();
    let details: Element = match (chosen, super::app_window()) {
        (Some(id), Some(window)) if snap.node(id).is_some() => StylesPane {
            snap: snap.clone(),
            id,
            window,
        }
        .into(),
        _ => centred(tokens, "Select an element"),
    };
    // Two columns leave neither the tree nor the properties room below this
    // width, so the properties stack under the tree.
    let narrow = Platform::get().current_window().root_size.read().width < 640.0;
    let pane = split_pane(tree, details, ratio);
    let pane = if narrow { pane.vertical() } else { pane };
    rect()
        .tw(tokens, &[w_full(), h_flex(), flex_row(), grow_children()])
        .child(pane)
        .into()
}

fn centred(tokens: &Tokens, text: &str) -> Element {
    rect()
        .tw(
            tokens,
            &[
                w_full(),
                h_full(),
                flex_col(),
                items(Align::Center),
                justify(Align::Center),
            ],
        )
        .child(styled_label(CAPTION, text))
        .into()
}
