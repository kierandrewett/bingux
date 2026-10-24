//! The search result blocks of the open page, listed in the sidebar while the
//! search has text. A search block registers itself once it is laid out, and
//! the sidebar lists the pages with results. Pressing one scrolls to it.

use std::sync::atomic::{AtomicU64, Ordering};

use freya::prelude::*;

use crate::icons::Icon;

/// The split view provides this. Search blocks register into it, and the
/// sidebar reads it.
#[derive(Clone, Copy, PartialEq)]
pub struct SectionRegistry(pub State<SectionList>);

#[derive(Clone, Default, PartialEq)]
pub struct SectionList {
    /// The top of the page's scrolled content, once it is laid out.
    origin: Option<f32>,
    /// The search results' page blocks that are showing.
    blocks: Vec<Block>,
}

#[derive(Clone, PartialEq)]
struct Block {
    id: u64,
    title: String,
    icon: Icon,
    /// Where the block starts, in layout pixels.
    y: f32,
}

/// A search result the sidebar lists: a page with matches, and how far below
/// the top of the page its block starts.
#[derive(Clone, PartialEq)]
pub struct SearchHit {
    pub title: String,
    pub icon: Icon,
    pub offset: i32,
}

/// Provides an empty registry to the sections below. The split view calls it
/// once per render.
pub fn provide() -> SectionRegistry {
    use_provide_context(|| SectionRegistry(State::create(SectionList::default())))
}

/// The search result blocks that are showing, in page order, as the sidebar
/// lists them. Empty until the page's origin is known.
pub fn hits(list: &SectionList) -> Vec<SearchHit> {
    let Some(origin) = list.origin else {
        return Vec::new();
    };
    let mut blocks = list.blocks.clone();
    blocks.sort_by(|a, b| a.y.total_cmp(&b.y));
    blocks
        .into_iter()
        .map(|block| SearchHit {
            title: block.title,
            icon: block.icon,
            offset: (block.y - origin).round() as i32,
        })
        .collect()
}

/// Registers the calling search result block, a page's part of the results,
/// at wherever it is laid out while `shown`. Put the returned handler on the
/// block's card with `on_sized`. Hidden blocks are not listed.
pub fn use_search_block(
    title: &str,
    icon: Icon,
    shown: bool,
) -> EventHandler<Event<SizedEventData>> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = use_hook(|| NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let registry = use_try_consume::<SectionRegistry>();
    let title = title.to_string();
    use_side_effect_with_deps(&shown, move |shown: &bool| {
        if let Some(mut registry) = registry
            && !*shown
        {
            registry.0.write().blocks.retain(|block| block.id != id);
        }
    });
    use_drop(move || {
        if let Some(mut registry) = registry {
            registry.0.write().blocks.retain(|block| block.id != id);
        }
    });
    (move |event: Event<SizedEventData>| {
        let Some(mut registry) = registry else {
            return;
        };
        if !shown {
            return;
        }
        let y = event.area.min_y();
        let listed = registry
            .0
            .peek()
            .blocks
            .iter()
            .any(|block| block.id == id && block.title == title && block.y == y);
        if listed {
            return;
        }
        let mut list = registry.0.write();
        list.blocks.retain(|block| block.id != id);
        list.blocks.push(Block {
            id,
            title: title.clone(),
            icon,
            y,
        });
    })
    .into()
}

/// Records where the page's scrolled content starts. Put it on the element
/// that wraps the page content inside the scroll view.
pub fn origin_handler(registry: SectionRegistry) -> EventHandler<Event<SizedEventData>> {
    (move |event: Event<SizedEventData>| {
        let mut registry = registry;
        let y = event.area.min_y();
        if registry.0.peek().origin != Some(y) {
            registry.0.write().origin = Some(y);
        }
    })
    .into()
}
