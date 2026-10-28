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
    flash: f32,
    on_sized: EventHandler<Event<SizedEventData>>,
}

impl Anchor {
    /// 0 to 1: how strongly the row is lit as the target of the location. It
    /// rises when the page has scrolled to the row and fades over about a
    /// second.
    pub fn flash(&self) -> f32 {
        self.flash
    }

    /// Put this on the row's root: it tells the anchor where the row is.
    pub fn on_sized(&self) -> EventHandler<Event<SizedEventData>> {
        self.on_sized.clone()
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
    let flash = use_tween(if flashing() { 1.0 } else { 0.0 }, if flashing() { 160 } else { 1100 });
    let done = use_hook(|| Rc::new(Cell::new(false)));
    let last_y = use_hook(|| Rc::new(Cell::new(None::<f32>)));
    if !targeted {
        done.set(false);
    }
    // Fully lit: start fading.
    let peaked = flashing() && flash >= 0.98;
    use_side_effect_with_deps(&peaked, move |peaked| {
        if *peaked {
            flashing.set(false);
        }
    });

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
    Anchor { flash, on_sized }
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
