//! `<input type=file>`, through the file chooser every Bingux app has.

use bingux_ui::files::choose;
use bingux_ui::prelude::*;
use bingux_ui::web_view::RequestId;
use bingux_ui::web_view::requests::{PickerMode, PickerRequest};

use crate::ui::Ctx;

/// The file chooser a page asked for.
pub struct PickerPrompt;

impl PickerPrompt {
    /// Open the chooser for question `id` of tab `key`.
    pub fn show(ctx: Ctx, key: u64, id: RequestId, request: &PickerRequest) {
        let chooser = match request.mode {
            PickerMode::Open => ChooserRequest::open().title(tr!("browser-prompt-file-title")),
            PickerMode::OpenMany => ChooserRequest::open()
                .multiple(true)
                .title(tr!("browser-prompt-files-title")),
            PickerMode::Folder => ChooserRequest::select_folder(),
            PickerMode::Save => {
                ChooserRequest::save(request.suggested_name.clone().unwrap_or_default())
            }
        };
        choose(ctx.window, chooser, move |chosen| {
            let paths = chosen.map(|chosen| chosen.paths).unwrap_or_default();
            ctx.with(|browser| browser.answer_picker(key, id, paths));
        });
    }
}
