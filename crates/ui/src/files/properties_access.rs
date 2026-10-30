//! The Permissions tab of the Properties dialog: the read, write and execute
//! grid, the numeric mode, ownership, and for a folder an explicit "apply to
//! enclosed items" action that asks first.
//!
//! The tab acts on every selected item. A box where the items differ is
//! mixed; pressing it sets the bit on all of them. The tab is laid out in
//! full from the first frame: what is not yet known shows "…" and the grid is
//! disabled, and the state of the person's right to change (owner, read-only
//! drive) is a row, so the text above the grid never changes.

use bingux_files::ops::{Permissions, TreeChange};
use freya::prelude::*;

use super::properties::Page;
use super::properties_permissions::{Can, ModeSet, PermissionGrid, Who};
use super::properties_words::{NONE, PENDING, named_id, plural};
use crate::controls::action_button;
use crate::group::Group;
use crate::rows::{property_row, row, switch_row};
use crate::style::*;

/// Where "apply to enclosed items" is.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum TreePhase {
    Idle,
    /// The person pressed Apply and has been asked.
    Confirming,
    Running,
    Done(TreeChange),
    Stopped,
    Failed(String),
}

/// What the tab shows and does, from the dialog that owns the state.
pub(super) struct Access<'a> {
    /// Each item's owner and group; `None` until read.
    pub perms: Option<&'a [Permissions]>,
    /// Why they could not be read.
    pub failed: Option<&'a str>,
    /// The modes on show, including the ones just set; `None` until read.
    pub modes: Option<ModeSet>,
    /// The person may change them: they own every item and the drive is
    /// writable.
    pub editable: bool,
    pub read_only: bool,
    /// Why the last change failed.
    pub error: Option<String>,
    /// The new mode of every item, in order.
    pub on_change: EventHandler<Vec<u32>>,
    pub tree: TreePhase,
    pub on_tree_ask: EventHandler<()>,
    pub on_tree_cancel: EventHandler<()>,
    pub on_tree_apply: EventHandler<()>,
}

/// What "Access" says about the person's right to change the modes.
fn access_line(access: &Access, several: bool) -> String {
    if let Some(error) = &access.error {
        return error.clone();
    }
    match (&access.perms, access.failed) {
        (None, Some(_)) => NONE.to_owned(),
        (None, None) => PENDING.to_owned(),
        (Some(_), _) if access.read_only => "Nobody, read-only drive".to_owned(),
        (Some(_), _) if access.editable => "You".to_owned(),
        (Some(_), _) if several => "Only the owners".to_owned(),
        (Some(_), _) => "Only the owner".to_owned(),
    }
}

/// `kieran (1000)`, or "Several owners" when the items differ.
fn shared_name(access: &Access, of: impl Fn(&Permissions) -> (u32, &str), noun: &str) -> String {
    match (access.perms, access.failed) {
        (Some(perms), _) => {
            let mut all = perms.iter().map(&of);
            let Some(first) = all.next() else {
                return NONE.to_owned();
            };
            if all.all(|other| other.0 == first.0) {
                named_id(first.1, first.0)
            } else {
                format!("Several {noun}")
            }
        }
        (None, Some(_)) => NONE.to_owned(),
        (None, None) => PENDING.to_owned(),
    }
}

pub(super) fn permissions_tab(page: &Page, access: &Access) -> Vec<Element> {
    let several = !page.single();
    let all_folders = page.entries.iter().all(|entry| entry.is_dir());
    let any_folder = page.entries.iter().any(|entry| entry.is_dir());

    let modes = access.modes.clone();
    let on_change = access.on_change.clone();
    let grid_modes = modes.clone();
    let on_press = EventHandler::new(move |(who, can, next): (Who, Can, bool)| {
        if let Some(modes) = &grid_modes {
            on_change.call(modes.toggled(who, can, next));
        }
    });

    let pending = |text: Option<String>| text.unwrap_or_else(|| PENDING.to_owned());
    let numeric_text = pending(modes.as_ref().map(ModeSet::numeric));
    let letters_text = pending(modes.as_ref().map(ModeSet::letters));
    let special_text = pending(modes.as_ref().map(ModeSet::special_line));
    let mut permissions = Group::titled("Permissions")
        .description(if several {
            "Who may read, write and run each of these."
        } else {
            "Who may read, write and run this."
        })
        .content(PermissionGrid {
            modes: modes.clone(),
            editable: access.editable,
            execute: if all_folders { "Access" } else { "Execute" },
            on_change: on_press,
        })
        .row(property_row("Who can change", access_line(access, several)))
        .row(row("Mode").mono_value(numeric_text))
        .row(row("In letters").mono_value(letters_text))
        .row(property_row("Special", special_text));
    if !any_folder {
        let toggle_modes = modes.clone();
        let on_change = access.on_change.clone();
        let on = modes
            .as_ref()
            .is_some_and(|modes| modes.executable() == crate::controls::CheckState::On);
        permissions = permissions.row(
            switch_row(
                "Allow executing as program",
                on,
                EventHandler::new(move |next: bool| {
                    if let Some(modes) = &toggle_modes {
                        on_change.call(modes.with_execute(next));
                    }
                }),
            )
            .enabled(access.editable && modes.is_some()),
        );
    }

    let ownership = Group::titled("Ownership")
        .row(property_row(
            "Owner",
            shared_name(access, |perms| (perms.uid, perms.owner.as_str()), "owners"),
        ))
        .row(property_row(
            "Group",
            shared_name(access, |perms| (perms.gid, perms.group.as_str()), "groups"),
        ));

    let mut groups: Vec<Element> = vec![permissions.into(), ownership.into()];
    if page.single() && page.is_folder() {
        groups.push(enclosed(page, access).into());
    }
    if let Some(security) = security(page) {
        groups.push(security.into());
    }
    groups
}

/// "Apply to enclosed items", which asks before it changes a whole tree.
fn enclosed(page: &Page, access: &Access) -> Group {
    let mode = access.modes.as_ref().and_then(ModeSet::common);
    let ready = access.editable && mode.is_some();
    let ask = access.on_tree_ask.clone();
    let cancel = access.on_tree_cancel.clone();
    let apply = access.on_tree_apply.clone();
    let buttons = rect().tw(&page.tokens, &[flex_row(), items(Align::Center), gap(S2)]);
    let (title, trailing) = match &access.tree {
        TreePhase::Idle => (
            "Apply to enclosed items".to_owned(),
            buttons
                .child(action_button("Apply…", move |_| ask.call(())).compact().enabled(ready))
                .into_element(),
        ),
        TreePhase::Confirming => (
            mode.map_or_else(
                || "Apply to everything inside?".to_owned(),
                |mode| format!("Apply {:04o} to all items inside?", mode & 0o7777),
            ),
            buttons
                .child(action_button("Cancel", move |_| cancel.call(())).compact())
                .child(action_button("Apply", move |_| apply.call(())).compact().destructive())
                .into_element(),
        ),
        TreePhase::Running => (
            "Applying…".to_owned(),
            buttons
                .child(action_button("Stop", move |_| cancel.call(())).compact())
                .into_element(),
        ),
        TreePhase::Done(change) => (
            done_line(*change),
            buttons
                .child(action_button("Apply…", move |_| ask.call(())).compact().enabled(ready))
                .into_element(),
        ),
        TreePhase::Stopped => (
            "Stopped. What was changed stays changed.".to_owned(),
            buttons
                .child(action_button("Apply…", move |_| ask.call(())).compact().enabled(ready))
                .into_element(),
        ),
        TreePhase::Failed(message) => (
            message.clone(),
            buttons
                .child(action_button("Apply…", move |_| ask.call(())).compact().enabled(ready))
                .into_element(),
        ),
    };
    Group::titled("Enclosed items").row(
        row(title)
            .subtitle("Files keep an execute bit only where they had one")
            .trailing(trailing)
            .trailing_control(),
    )
}

/// What a finished tree change did.
fn done_line(change: TreeChange) -> String {
    let changed = plural(change.changed, "item", "items");
    if change.failed == 0 {
        format!("Changed {changed}")
    } else {
        format!("Changed {changed}; {} could not be changed", change.failed)
    }
}

/// The SELinux context and the access control list, where the system keeps
/// them. Absent on a system without them, and added below the rest when read.
fn security(page: &Page) -> Option<Group> {
    if !page.single() {
        return None;
    }
    let details = page.loaded?.details.as_ref()?;
    if details.security_context.is_none() && !details.acl {
        return None;
    }
    let mut group = Group::titled("Security");
    if let Some(context) = &details.security_context {
        group = group.row(row("SELinux context").mono_value(context.clone()));
    }
    if details.acl {
        group = group.row(property_row("Access control list", "Has entries beyond owner, group and others"));
    }
    Some(group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_change_counts_what_it_did_and_what_it_could_not() {
        assert_eq!(done_line(TreeChange { changed: 1, failed: 0 }), "Changed 1 item");
        assert_eq!(done_line(TreeChange { changed: 12, failed: 0 }), "Changed 12 items");
        assert_eq!(
            done_line(TreeChange { changed: 12, failed: 2 }),
            "Changed 12 items; 2 could not be changed"
        );
    }
}
