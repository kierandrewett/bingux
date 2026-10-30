//! The "Add plugin" popup: every plugin found on the system, grouped by
//! category, with a search field.

use bingux_audio_effects::catalog::PluginInfo;
use bingux_audio_effects::preset::{Band, EQUALIZER, PluginSlot};
use bingux_audio_effects::{Catalog, Category};
use freya::prelude::*;

use bingux_ui::prelude::{PageHead, popup, search_field, use_window};
use bingux_ui::window::{PopupFlags, WindowLike, WindowParent};

use super::{Effect, Fx};
use crate::ui::{Group, Status, page_column, row, status};

pub(super) fn open(fx: Fx) {
    fx.window.open(popup("Add plugin", Picker { fx }).flags(PopupFlags::CANCEL | PopupFlags::GRABBER));
}

/// A new plugin of the kind `info` describes, with its default controls.
fn new_slot(id: String, info: &PluginInfo) -> PluginSlot {
    let mut slot = PluginSlot::new(id, info.id.clone());
    if info.id == EQUALIZER {
        slot.preamp = Some(0.0);
        slot.bands = [60.0, 250.0, 1000.0, 4000.0, 12000.0]
            .into_iter()
            .map(|frequency| Band::peaking(frequency, 0.0, 1.0))
            .collect();
    }
    slot
}

fn matches(info: &PluginInfo, query: &str) -> bool {
    info.id != bingux_audio_effects::graph::ANALYSER
        && (query.is_empty()
            || info.name.to_lowercase().contains(query)
            || info.maker.to_lowercase().contains(query)
            || info.label.to_lowercase().contains(query))
}

#[derive(PartialEq)]
struct Picker {
    fx: Fx,
}

impl Component for Picker {
    fn render(&self) -> impl IntoElement {
        let fx = self.fx;
        let window = use_window();
        let query = use_state(String::new);
        let needle = query.read().trim().to_lowercase();
        let catalog: Option<Catalog> = fx.catalog.read().clone();

        let mut page = page_column()
            .child(PageHead::new())
            .child(search_field(query, |_| {}).placeholder("Search plugins").fill_width());
        let Some(catalog) = catalog else {
            return page
                .child(Group::new(None::<String>).row(status(Status::Loading("Looking for plugins.".to_string()))));
        };
        let mut any = false;
        for category in Category::ALL {
            let found: Vec<&PluginInfo> = catalog
                .in_category(category)
                .into_iter()
                .filter(|info| matches(info, &needle))
                .collect();
            if found.is_empty() {
                continue;
            }
            any = true;
            let rows = found.into_iter().map(|info| {
                let problem = info.unusable();
                let kind = info.clone();
                let mut item = row(info.name.clone()).subtitle(problem.clone().unwrap_or_else(|| info.maker.clone()));
                if problem.is_none() {
                    item = item.on_press(move |_| {
                        let stem = if kind.label.is_empty() { kind.id.clone() } else { kind.label.clone() };
                        let id = fx.data.peek().preset.fresh_id(&stem);
                        let slot = new_slot(id.clone(), &kind);
                        fx.edit(move |preset| preset.plugins.push(slot), Effect::Restart);
                        fx.select(&id);
                        window.close();
                    });
                } else {
                    item = item.enabled(false);
                }
                Element::from(item)
            });
            page = page.child(Group::titled(category.title()).rows(rows));
        }
        if !any {
            page = page.child(Group::new(None::<String>).row(status(Status::Empty("No plugins match.".to_string()))));
        }
        page
    }
}
