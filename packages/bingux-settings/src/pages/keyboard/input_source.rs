//! One input source's own page: its details, and the actions that move it in
//! the list or remove it. The keyboard page's source rows open it.

use freya::prelude::*;

use super::sources::{self, Edit, Source};
use super::{SourceEdits, kind_label};
use crate::icons::Icon;
use crate::system;
use crate::ui::{
    Group, PageHead, Status, page_column, property_row, row, skeleton, status, use_overlays,
    use_window,
};

/// The page for `source`. It reads the list the keyboard page reads, so it
/// shows the same position and enables the actions the list would.
#[derive(PartialEq)]
pub struct InputSourcePage {
    pub source: Source,
}

impl Component for InputSourcePage {
    fn render(&self) -> impl IntoElement {
        let sources_state = system::use_cached_persistent("keyboard-sources", sources::read);
        let catalog_state = system::use_cached_saved("keyboard-catalog", sources::catalog);
        let overlays = use_overlays();
        let window = use_window();
        let busy = use_state(|| false);
        let edits = SourceEdits {
            state: sources_state,
            catalog: catalog_state,
            overlays,
            window,
            busy,
        };

        let config = match sources_state.read().clone() {
            None => return page_column().child(PageHead::new()).child(skeleton()),
            Some(Err(message)) => {
                return page_column()
                    .child(PageHead::new())
                    .child(status(Status::Error(message)));
            }
            Some(Ok(config)) => config,
        };
        let Some(index) = config.sources.iter().position(|s| *s == self.source) else {
            return page_column()
                .child(PageHead::new())
                .child(status(Status::Empty(
                    "This input source is no longer set up.".into(),
                )));
        };
        let count = config.sources.len();

        let busy_now = *busy.read();

        let details = Group::titled("Details")
            .row(property_row("Type", kind_label(&self.source.kind)))
            .row(property_row(
                "Position",
                format!("{} of {}", index + 1, count),
            ));

        let actions = Group::titled("Actions")
            .row(
                row("Move up")
                    .leading(Icon::PanUp)
                    .enabled(index > 0 && !busy_now)
                    .on_press(move |_| edits.run(Edit::MoveUp(index))),
            )
            .row(
                row("Move down")
                    .leading(Icon::PanDown)
                    .enabled(index + 1 < count && !busy_now)
                    .on_press(move |_| edits.run(Edit::MoveDown(index))),
            )
            .row(
                row("Remove")
                    .leading(Icon::Delete)
                    .destructive()
                    .enabled(count > 1 && !busy_now)
                    .on_press(move |_| edits.run(Edit::Remove(index))),
            );

        page_column()
            .child(PageHead::new())
            .child(details)
            .child(actions)
    }
}
