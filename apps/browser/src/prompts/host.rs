//! Shows the dialog of the first prompt of the tab on show, once for each.

use freya::prelude::*;

use super::{AuthPrompt, DialogPrompt, PickerPrompt, Prompt, SelectPrompt};
use crate::ui::Ctx;

/// Shows the dialog of the first prompt of the tab on show, once for each.
/// It draws nothing of its own.
#[derive(PartialEq)]
pub struct PromptHost {
    pub ctx: Ctx,
}

impl PromptHost {
    /// What names a prompt so that a new one is a new dialog.
    fn id(ctx: Ctx) -> Option<(u64, u32)> {
        let browser = ctx.browser.read();
        let key = browser.tabs.active_key();
        browser.prompt(key).and_then(|prompt| match prompt {
            Prompt::Permission(..) => None,
            other => Some((key, other.id().0)),
        })
    }
}

impl Component for PromptHost {
    fn render(&self) -> impl IntoElement {
        let ctx = self.ctx;
        let id = Self::id(ctx);
        use_side_effect_with_deps(&id, move |id: &Option<(u64, u32)>| {
            let Some((key, _)) = *id else {
                return;
            };
            let prompt = ctx.browser.peek().prompt(key).cloned();
            match prompt {
                Some(Prompt::Dialog(_, request)) => DialogPrompt::show(ctx, key, &request),
                Some(Prompt::Auth(_, request)) => AuthPrompt::show(ctx, key, &request),
                Some(Prompt::Select(_, request)) => SelectPrompt::show(ctx, key, &request),
                Some(Prompt::FilePicker(_, request)) => PickerPrompt::show(ctx, key, &request),
                _ => {}
            }
        });
        rect().width(Size::px(0.0)).height(Size::px(0.0))
    }
}
