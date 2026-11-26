//! The question Clear history asks before it forgets anything.

use bingux_ui::prelude::*;

use crate::ui::Ctx;

/// Asks whether to forget the history, and forgets it when the person says
/// so. The toast that follows says what was and was not reached.
pub struct ClearDialog;

impl ClearDialog {
    /// Show the question over the window.
    pub fn open(ctx: Ctx) {
        let pages = ctx.browser.peek().history_len();
        ctx.overlays.show_dialog(
            dialog(
                tr!("browser-clear-history-title"),
                tr!("browser-clear-history-description", count = pages),
            )
            .action(
                CANCEL_ID,
                tr!("browser-prompt-cancel-button"),
                ResponseKind::Default,
                |_: ()| {},
            )
            .response(
                tr!("browser-settings-clear-button"),
                ResponseKind::Destructive,
                move |_: ()| ctx.with(|browser| browser.clear_history()),
            )
            .default_action(CANCEL_ID)
            .cancel_action(CANCEL_ID),
        );
    }
}
