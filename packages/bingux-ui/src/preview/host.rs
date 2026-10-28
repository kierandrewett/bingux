//! The preview host: one content, three frames.
//!
//! [`PreviewHost`] is the content of a preview in every frame: the header
//! (file, position, buttons), the viewer the registry picked, the Info panel
//! and the footer line. A popup, a docked pane and a full-screen window put
//! the same component in different places; the frame decides only where it
//! sits and which keys reach it.
//!
//! Stepping to another file keeps the header and the frame and remounts the
//! [`Subject`] below them, keyed by the file, so every load starts afresh,
//! the last file's is dropped, and nothing flickers: until the new file has
//! been read, its icon stands where its content will be.

use std::sync::Arc;

use bingux_preview_core::{PreviewSource, ViewerId};
use freya::prelude::*;

use super::chrome::{InfoPanel, PreviewNav, footer, header, icon_of};
use super::info;
use super::keys::{self, Command};
use super::mode::Mode;
use super::subject::{Resolved, resolve};
use super::viewers::{self, Placeholder};
use crate::component::ComponentName;
use crate::motion::{MOVE_MS, use_tween};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::{Resource, use_resource};

/// A preview of one source in one frame. `source` is `None` when there is
/// nothing to preview (nothing selected).
#[derive(Clone, PartialEq)]
pub struct PreviewHost {
    source: Option<PreviewSource>,
    mode: Mode,
    nav: PreviewNav,
}

impl PreviewHost {
    pub fn new(source: Option<PreviewSource>, mode: Mode, nav: PreviewNav) -> Self {
        PreviewHost { source, mode, nav }
    }
}

impl Component for PreviewHost {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let info = use_state(|| false);
        let compact = crate::style::is_compact();
        let header = header(&tokens, self.source.as_ref(), self.mode, &self.nav, info, compact);
        // The keyboard belongs to the content, not to the first button of the
        // header: a popup takes it when it opens, so Space, the arrows and the
        // viewer's own keys work at once and no button shows a ring nobody
        // asked for. A docked pane leaves it with the list.
        let a11y_id = use_a11y();
        let mode = self.mode;
        use_hook(move || {
            if mode != Mode::Pane {
                a11y_id.request_focus();
            }
        });
        rect()
            .component("PreviewHost")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .child(header)
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(match &self.source {
                Some(source) => Element::from(Subject {
                    source: source.clone(),
                    mode: self.mode,
                    nav: self.nav.clone(),
                    info,
                }),
                None => Element::from(rect().tw(&tokens, &[w_full(), h_flex()])),
            })
    }
}

/// One source on the stage, with its Info panel and footer. Keyed by the
/// source, so another file starts every load afresh.
#[derive(PartialEq)]
struct Subject {
    source: PreviewSource,
    mode: Mode,
    nav: PreviewNav,
    /// Whether the Info panel is open; the host owns it, so it survives a step.
    info: State<bool>,
}

/// The facts of what a subject shows: what the footer and the Info panel say.
fn facts_of(state: &Option<Result<Arc<Resolved>, String>>) -> (String, Vec<info::InfoRow>) {
    match state {
        Some(Ok(resolved)) => (info::summary(resolved), info::rows(resolved)),
        _ => (String::new(), Vec::new()),
    }
}

impl Component for Subject {
    fn render_key(&self) -> DiffKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.source, &mut hasher);
        DiffKey::U64(std::hash::Hasher::finish(&hasher))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let loaded = use_resource(self.source.clone(), resolve);
        let state: Option<Result<Arc<Resolved>, String>> = loaded.with(|loaded| match loaded {
            Resource::Ready(resolved) | Resource::Refreshing(resolved) => Some(Ok(resolved.clone())),
            Resource::Error(message) => Some(Err(message.clone())),
            Resource::Loading => None,
        });
        let viewer = match &state {
            Some(Ok(resolved)) => resolved.viewer,
            _ => ViewerId::Card,
        };
        let (line, rows) = facts_of(&state);

        // A pane has no room for the Info panel beside the viewer: it takes
        // the viewer's place while it is open.
        let info_instead = self.mode == Mode::Pane && *self.info.read();
        let stage = match &state {
            Some(Ok(resolved)) => viewers::viewer(resolved.clone(), &self.nav, self.mode),
            Some(Err(message)) => Element::from(Placeholder::new(icon_of(&self.source), self.source.name(), Some(message.clone()))),
            None => Element::from(Placeholder::new(icon_of(&self.source), self.source.name(), None)),
        };

        // The Info panel slides in from the right edge of the content. Its
        // content keeps its own width while the box grows, so nothing re-flows.
        let show_info = *self.info.read() && !info_instead;
        let slide = use_tween(if show_info { 1.0 } else { 0.0 }, tokens.motion(MOVE_MS));
        let panel_width = Dim::MenuWide.px();
        let panel = (slide > 0.001).then(|| {
            Element::from(
                rect()
                    .width(Size::px(panel_width * slide))
                    .height(Size::fill())
                    .tw(&tokens, &[flex_row(), clip()])
                    .child(rect().tw(&tokens, &[w(Dim::Hairline), h_full(), bg(Paint::Separator)]))
                    .child(rect().width(Size::px(panel_width)).height(Size::fill()).child(InfoPanel {
                        rows: rows.clone(),
                        on_properties: self.nav.on_properties.clone(),
                    })),
            )
        });

        let nav = self.nav.clone();
        let mode = self.mode;
        let info = self.info;
        // A docked pane leaves the keyboard with the list: the list's own
        // arrows already step the selection the pane follows.
        let on_key = move |event: Event<KeyboardEventData>| {
            if mode == Mode::Pane {
                return;
            }
            let Some(command) = keys::route(&event.key, event.modifiers, viewer, nav.grid) else {
                return;
            };
            event.stop_propagation();
            run(&nav, mode, info, command);
        };

        rect()
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_flex()])
            .on_global_key_down(on_key)
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), grow_children(), w_full(), h_flex()])
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .height(Size::fill())
                            .min_width(Size::px(0.0))
                            .tw(&tokens, &[bg(Paint::Card), clip()])
                            .child(if info_instead {
                                Element::from(InfoPanel {
                                    rows: rows.clone(),
                                    on_properties: self.nav.on_properties.clone(),
                                })
                            } else {
                                stage
                            }),
                    )
                    .maybe_child(panel),
            )
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(footer(&tokens, line))
    }
}

/// Do what `command` says in a preview framed as `mode`.
fn run(nav: &PreviewNav, mode: Mode, info: State<bool>, command: Command) {
    match command {
        Command::Close => {
            // Escape leaves full screen before it closes.
            match (mode, &nav.on_mode) {
                (Mode::Fullscreen, Some(switch)) => switch.call(Mode::Popup),
                _ => nav.on_close.call(()),
            }
        }
        Command::Step(step) => {
            if let Some(handler) = &nav.on_step {
                handler.call(step);
            }
        }
        Command::Open => {
            if mode != Mode::Pane {
                nav.on_close.call(());
            }
            if let Some(open) = &nav.on_open {
                open.call(());
            }
        }
        Command::Properties => {
            if mode != Mode::Pane {
                nav.on_close.call(());
            }
            if let Some(show) = &nav.on_properties {
                show.call(());
            }
        }
        Command::Info => {
            let mut info = info;
            let shown = *info.peek();
            info.set(!shown);
        }
        Command::Fullscreen => {
            if let Some(switch) = &nav.on_mode {
                switch.call(if mode == Mode::Fullscreen { Mode::Popup } else { Mode::Fullscreen });
            }
        }
        Command::SwitchMode => {
            if let Some(switch) = &nav.on_mode {
                switch.call(mode.switched());
            }
        }
    }
}
