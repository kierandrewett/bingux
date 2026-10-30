//! Anchors: a row names itself, and a location's fragment points at it.
//!
//! `bingux-settings://network/proxy#proxy` opens the proxy page, scrolls to the
//! row whose anchor is `proxy` and flashes it. Every [`crate::rows::Row`]
//! anchors itself by the [`slug`] of its title, so a page needs no code for it.

use std::cell::Cell;
use std::rc::Rc;

use freya::prelude::*;

use crate::motion::use_tween;
use crate::nav::router::{try_router, use_entry_id, use_fragment};
use crate::shell::navigation::{section_top, use_page_scroll};
use crate::shell::sections::SectionRegistry;
use crate::style::*;
use crate::surface::use_tokens;

/// A title as a fragment: its letters and digits in lower case, words joined
/// by `-` (`Use this connection` is `use-this-connection`).
pub fn slug(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut gap = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.extend(c.to_lowercase());
        } else {
            gap = true;
        }
    }
    out
}

/// What [`use_anchor`] gives a row.
#[derive(Clone)]
pub struct Anchor {
    flashing: State<bool>,
    /// The row is the target of the location, or its highlight is still running.
    lit: bool,
    on_sized: EventHandler<Event<SizedEventData>>,
}

impl Anchor {
    /// The highlight to lay over the row, as its last child, while the row is
    /// the target of the location: it rises when the page has scrolled to the
    /// row and fades over about a second. `None` for every other row, which
    /// therefore carries no tween: a page of forty rows has one highlight, and
    /// that one only while the location points at it.
    pub fn flash(&self) -> Option<Element> {
        self.lit.then(|| {
            AnchorFlash {
                flashing: self.flashing,
            }
            .into()
        })
    }

    /// Put this on the row's root: it tells the anchor where the row is.
    pub fn on_sized(&self) -> EventHandler<Event<SizedEventData>> {
        self.on_sized.clone()
    }
}

/// The highlight of the row a location points at: rises over 160 ms, then fades
/// over 1.1 s. While it is not lit it draws nothing.
#[derive(PartialEq)]
struct AnchorFlash {
    flashing: State<bool>,
}

impl Component for AnchorFlash {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut flashing = self.flashing;
        let on = flashing();
        let flash = use_tween(if on { 1.0 } else { 0.0 }, if on { 160 } else { 1100 });
        // Fully lit: start fading.
        let peaked = on && flash >= 0.98;
        use_side_effect_with_deps(&peaked, move |peaked| {
            if *peaked {
                flashing.set(false);
            }
        });
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .maybe(flash > 0.01, |lit| {
                lit.width(Size::fill())
                    .height(Size::fill())
                    .tw(&tokens, &[bg(Paint::Accent)])
                    .opacity(flash * 0.22)
            })
    }
}

/// Registers the calling row as the anchor `id`. When the window's location
/// has `#id` and the row is on the current page, the page scrolls to the row
/// once and [`Anchor::flash`] pulses. Without a router it does nothing.
pub fn use_anchor(id: &str) -> Anchor {
    let fragment = use_fragment();
    let entry = use_entry_id();
    let current = try_router().map(|router| router.current_id());
    let targeted = !id.is_empty()
        && fragment.as_deref() == Some(id)
        && entry.is_none_or(|entry| Some(entry) == current);
    let scroll = use_page_scroll();
    let registry = use_try_consume::<SectionRegistry>();
    let mut flashing = use_state(|| false);
    let done = use_hook(|| Rc::new(Cell::new(false)));
    let last_y = use_hook(|| Rc::new(Cell::new(None::<f32>)));
    if !targeted {
        done.set(false);
    }

    let mut activate = {
        let done = done.clone();
        let last_y = last_y.clone();
        move || {
            if done.get() {
                return;
            }
            let Some(y) = last_y.get() else { return };
            done.set(true);
            if let (Some(mut scroll), Some(origin)) = (scroll, registry.and_then(|r| r.origin())) {
                let height = Platform::get().current_window().root_size.read().height;
                let want = (height * 0.3).max(section_top());
                scroll.scroll_to_y(((want - (y - origin)).round() as i32).min(0));
            }
            flashing.set(true);
        }
    };
    let mut on_effect = activate.clone();
    use_side_effect_with_deps(&targeted, move |targeted| {
        if *targeted {
            on_effect();
        }
    });
    let on_sized: EventHandler<Event<SizedEventData>> = (move |event: Event<SizedEventData>| {
        last_y.set(Some(event.area.min_y()));
        if targeted {
            activate();
        }
    })
    .into();
    Anchor {
        flashing,
        lit: targeted || flashing(),
        on_sized,
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn a_slug_is_lower_case_words_joined_by_dashes() {
        assert_eq!(slug("Wi-Fi"), "wi-fi");
        assert_eq!(slug("Use this connection only"), "use-this-connection-only");
        assert_eq!(slug("  Time zone!  "), "time-zone");
        assert_eq!(slug("Éditeur d'images"), "éditeur-d-images");
    }
}
