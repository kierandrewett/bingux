//! A `<select>` list the page wants an option of.

use bingux_ui::prelude::*;
use bingux_ui::web_view::RequestId;
use bingux_ui::web_view::requests::SelectRequest;
use freya::prelude::*;

use crate::ui::Ctx;

/// A dialog with the options in a dropdown.
pub struct SelectPrompt;

impl SelectPrompt {
    /// A `<select>` list, as a dialog with the options in a dropdown.
    pub fn show(ctx: Ctx, key: u64, id: RequestId, request: &SelectRequest) {
        let options: Vec<(String, String)> = request
            .options
            .iter()
            .enumerate()
            .map(|(at, option)| (at.to_string(), option.label.clone()))
            .collect();
        let first = request
            .options
            .iter()
            .position(|option| option.selected)
            .unwrap_or(0);
        let mut chosen = ctx.prompts.choice;
        chosen.set(first);
        ctx.overlays.show_dialog(
            dialog(tr!("browser-prompt-select-title"), "")
                .content(Element::from(SelectBody { ctx, options }))
                .action(
                    CANCEL_ID,
                    tr!("browser-prompt-cancel-button"),
                    ResponseKind::Default,
                    move |_: ()| {
                        ctx.with(|browser| browser.answer_select(key, id, None));
                    },
                )
                .response(
                    tr!("browser-prompt-choose-button"),
                    ResponseKind::Suggested,
                    move |_: ()| {
                        let index = *ctx.prompts.choice.peek();
                        ctx.with(|browser| browser.answer_select(key, id, Some(index)));
                    },
                )
                .default_action("choose")
                .cancel_action(CANCEL_ID),
        );
    }
}

/// The dropdown of a `<select>` dialog. It is a component of its own: it holds
/// a control, and a control has hooks.
#[derive(PartialEq)]
struct SelectBody {
    ctx: Ctx,
    options: Vec<(String, String)>,
}

impl Component for SelectBody {
    fn render(&self) -> impl IntoElement {
        let mut chosen = self.ctx.prompts.choice;
        dropdown(
            self.options.clone(),
            chosen.read().to_string(),
            // The options are numbered from 0 (`SelectPrompt::show`), so the
            // value of the dropdown is always a number.
            move |value: String| chosen.set(value.parse().unwrap_or(0)),
        )
    }
}
