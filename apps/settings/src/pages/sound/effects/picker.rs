//! The "Add effect" popup: every effect Bingux offers, grouped by what it does,
//! each with a one-line description and a search field. An effect whose plugin
//! is not installed is listed with the package that has it and cannot be added.

use bingux_audio_effects::effects::{self, Category};
use freya::prelude::*;

use bingux_ui::prelude::{PageHead, popup, search_field, use_window};
use bingux_ui::window::{PopupFlags, WindowLike, WindowParent};

use super::Fx;
use crate::ui::{Group, Status, page_column, row, status};

pub(super) fn open(fx: Fx) {
    fx.window
        .open(popup("Add effect", Picker { fx }).flags(PopupFlags::CANCEL | PopupFlags::GRABBER));
}

fn matches(effect: &effects::Effect, query: &str) -> bool {
    query.is_empty()
        || effect.name.to_lowercase().contains(query)
        || effect.summary.to_lowercase().contains(query)
        || effect.package.project().to_lowercase().contains(query)
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
        let catalog = fx.catalog.read().clone();

        let mut page = page_column().child(PageHead::new()).child(
            search_field(query, |_| {})
                .placeholder("Search effects")
                .fill_width(),
        );
        let Some(catalog) = catalog else {
            return page.child(
                Group::new(None::<String>)
                    .row(status(Status::Loading("Looking for plugins.".to_string()))),
            );
        };
        let mut any = false;
        for category in Category::ALL {
            let found: Vec<&effects::Effect> = effects::in_category(category)
                .filter(|effect| matches(effect, &needle))
                .collect();
            if found.is_empty() {
                continue;
            }
            any = true;
            let rows = found.into_iter().map(|effect| {
                let installed = catalog.plugin_of(effect).is_some();
                let kind = effect.key;
                let subtitle = if installed {
                    effect.summary.to_string()
                } else {
                    format!("Not installed. Install {}.", effect.package.fedora())
                };
                let mut item = row(effect.name).subtitle(subtitle);
                if installed {
                    item = item.on_press(move |_| {
                        let mut added = None;
                        fx.edit(|preset| added = preset.add(kind));
                        if let Some(id) = added {
                            fx.select(&id);
                        }
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
            page = page.child(
                Group::new(None::<String>)
                    .row(status(Status::Empty("No effects match.".to_string()))),
            );
        }
        page
    }
}
