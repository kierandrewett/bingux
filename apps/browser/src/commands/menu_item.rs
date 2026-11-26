//! A command as an entry of a menu.

use bingux_ui::prelude::*;

use super::Command;
use crate::ui::Ctx;

impl Command {
    /// A menu entry that runs this command, with its shortcut beside it.
    pub fn menu_item(self, ctx: Ctx, label: &str) -> MenuItem {
        let item = menu_item(label, move |_: ()| ctx.with(|browser| browser.run(self)));
        // A phone has no keyboard to learn the keys from, and no room for them.
        match self.label(ctx.platform) {
            Some(keys) if !is_compact() => item.shortcut(keys),
            _ => item,
        }
    }
}
