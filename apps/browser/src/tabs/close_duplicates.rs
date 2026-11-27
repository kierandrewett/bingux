//! The question Close duplicate tabs asks before it closes anything: Firefox's
//! `confirmEx` of `warnAboutClosingTabs` (Tabbrowser.sys.mjs, FIREFOX_160_0_RELEASE).

use bingux_ui::prelude::*;

use crate::browser::DuplicatesAsk;
use crate::ui::Ctx;

/// Asks what `ask` says, and closes `keys` when the person answers Close tabs.
/// Cancel, Escape and a press outside close nothing.
pub struct CloseDuplicatesDialog;

impl CloseDuplicatesDialog {
    /// Show the question over the window.
    pub fn open(ctx: Ctx, keys: Vec<u64>, ask: DuplicatesAsk) {
        // The first time Firefox explains the command; after that it only counts
        // the tabs, in the title alone.
        let (title, text) = match ask {
            DuplicatesAsk::FirstTime => (
                tr!("browser-close-duplicates-title"),
                tr!("browser-close-duplicates-text"),
            ),
            DuplicatesAsk::Many(count) => (
                tr!("browser-close-tabs-title", count = count),
                String::new(),
            ),
        };
        const CLOSE_ID: &str = "close-tabs";
        // Close tabs is Firefox's first button, and its default one.
        ctx.overlays.show_dialog(
            dialog(title, text)
                .action(
                    CANCEL_ID,
                    tr!("browser-prompt-cancel-button"),
                    ResponseKind::Default,
                    |_: ()| {},
                )
                .action(
                    CLOSE_ID,
                    tr!("browser-close-tabs-button"),
                    ResponseKind::Suggested,
                    move |_: ()| ctx.with(|browser| browser.close_tabs_as_duplicates(keys.clone())),
                )
                .default_action(CLOSE_ID)
                .cancel_action(CANCEL_ID),
        );
    }
}
