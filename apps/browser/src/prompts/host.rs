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
        // The question whose dialog this host put on the screen, if one is up.
        let mut opened = use_state(|| None::<(u64, u32)>);
        use_side_effect_with_deps(&id, move |id: &Option<(u64, u32)>| {
            // The first question changed. A dialog left over from an earlier
            // one is for a question that is gone (the engine ended it, or the
            // page went away): take it down, so that nothing on the screen
            // answers for a question that no longer waits. A dialog the person
            // has just answered is already closing, and this does nothing to it.
            let earlier = *opened.peek();
            if earlier.is_some() && earlier != *id && ctx.overlays.dialog_open() {
                ctx.overlays.dismiss_dialog();
            }
            let Some((key, _)) = *id else {
                opened.set(None);
                return;
            };
            let prompt = ctx.browser.peek().prompt(key).cloned();
            let mut on_screen = true;
            match prompt {
                Some(Prompt::Dialog(asked, request)) => {
                    DialogPrompt::show(ctx, key, asked, &request)
                }
                Some(Prompt::Auth(asked, request)) => AuthPrompt::show(ctx, key, asked, &request),
                Some(Prompt::Select(asked, request)) => {
                    SelectPrompt::show(ctx, key, asked, &request)
                }
                Some(Prompt::FilePicker(asked, request)) => {
                    PickerPrompt::show(ctx, key, asked, &request);
                    // The file chooser is the system's, not a dialog of the window.
                    on_screen = false;
                }
                _ => on_screen = false,
            }
            opened.set(on_screen.then_some(*id).flatten());
        });
        rect().width(Size::px(0.0)).height(Size::px(0.0))
    }
}
