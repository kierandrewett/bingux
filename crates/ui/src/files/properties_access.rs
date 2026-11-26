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
use super::properties_words::{NONE, PENDING, named_id};
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
        (Some(_), _) if access.read_only => crate::tr!("ui-files-props-access-nobody-read-only"),
        (Some(_), _) if access.editable => crate::tr!("ui-files-props-access-you"),
        (Some(_), _) if several => crate::tr!("ui-files-props-access-owners"),
        (Some(_), _) => crate::tr!("ui-files-props-access-owner"),
    }
}

/// `kieran (1000)`, or "Several owners" when the items differ.
fn shared_name(
    access: &Access,
    of: impl Fn(&Permissions) -> (u32, &str),
    several: String,
) -> String {
    match (access.perms, access.failed) {
        (Some(perms), _) => {
            let mut all = perms.iter().map(&of);
            let Some(first) = all.next() else {
                return NONE.to_owned();
            };
            if all.all(|other| other.0 == first.0) {
                named_id(first.1, first.0)
            } else {
                several
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
    let mut permissions = Group::titled(&crate::tr!("ui-files-props-permissions"))
        .description(if several {
            crate::tr!("ui-files-props-permissions-several")
        } else {
            crate::tr!("ui-files-props-permissions-single")
        })
        .content(PermissionGrid {
            modes: modes.clone(),
            editable: access.editable,
            execute: if all_folders {
                crate::i18n_id!("ui-files-props-can-access")
            } else {
                crate::i18n_id!("ui-files-props-can-execute")
            },
            on_change: on_press,
        })
        .row(property_row(
            crate::tr!("ui-files-props-who-can-change"),
            access_line(access, several),
        ))
        .row(row(crate::tr!("ui-files-props-mode")).mono_value(numeric_text))
        .row(row(crate::tr!("ui-files-props-in-letters")).mono_value(letters_text))
        .row(property_row(
            crate::tr!("ui-files-props-special"),
            special_text,
        ));
    if !any_folder {
        let toggle_modes = modes.clone();
        let on_change = access.on_change.clone();
        let on = modes
            .as_ref()
            .is_some_and(|modes| modes.executable() == crate::controls::CheckState::On);
        permissions = permissions.row(
            switch_row(
                crate::tr!("ui-files-props-allow-executing"),
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

    let ownership = Group::titled(&crate::tr!("ui-files-props-ownership"))
        .row(property_row(
            crate::tr!("ui-files-props-owner"),
            shared_name(
                access,
                |perms| (perms.uid, perms.owner.as_str()),
                crate::tr!("ui-files-props-several-owners"),
            ),
        ))
        .row(property_row(
            crate::tr!("ui-files-props-group"),
            shared_name(
                access,
                |perms| (perms.gid, perms.group.as_str()),
                crate::tr!("ui-files-props-several-groups"),
            ),
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
            crate::tr!("ui-files-props-apply-enclosed"),
            buttons
                .child(
                    action_button(crate::tr!("ui-files-props-apply-ellipsis"), move |_| {
                        ask.call(())
                    })
                    .compact()
                    .enabled(ready),
                )
                .into_element(),
        ),
        TreePhase::Confirming => (
            mode.map_or_else(
                || crate::tr!("ui-files-props-apply-everything-inside"),
                |mode| {
                    crate::tr!(
                        "ui-files-props-apply-mode-inside",
                        mode = format!("{:04o}", mode & 0o7777)
                    )
                },
            ),
            buttons
                .child(
                    action_button(crate::tr!("ui-files-props-cancel"), move |_| {
                        cancel.call(())
                    })
                    .compact(),
                )
                .child(
                    action_button(crate::tr!("ui-files-props-apply"), move |_| apply.call(()))
                        .compact()
                        .destructive(),
                )
                .into_element(),
        ),
        TreePhase::Running => (
            crate::tr!("ui-files-props-applying"),
            buttons
                .child(
                    action_button(crate::tr!("ui-files-props-stop"), move |_| cancel.call(()))
                        .compact(),
                )
                .into_element(),
        ),
        TreePhase::Done(change) => (
            done_line(*change),
            buttons
                .child(
                    action_button(crate::tr!("ui-files-props-apply-ellipsis"), move |_| {
                        ask.call(())
                    })
                    .compact()
                    .enabled(ready),
                )
                .into_element(),
        ),
        TreePhase::Stopped => (
            crate::tr!("ui-files-props-apply-stopped"),
            buttons
                .child(
                    action_button(crate::tr!("ui-files-props-apply-ellipsis"), move |_| {
                        ask.call(())
                    })
                    .compact()
                    .enabled(ready),
                )
                .into_element(),
        ),
        TreePhase::Failed(message) => (
            message.clone(),
            buttons
                .child(
                    action_button(crate::tr!("ui-files-props-apply-ellipsis"), move |_| {
                        ask.call(())
                    })
                    .compact()
                    .enabled(ready),
                )
                .into_element(),
        ),
    };
    Group::titled(&crate::tr!("ui-files-props-enclosed-items")).row(
        row(title)
            .subtitle(crate::tr!("ui-files-props-enclosed-hint"))
            .trailing(trailing)
            .trailing_control(),
    )
}

/// What a finished tree change did.
fn done_line(change: TreeChange) -> String {
    if change.failed == 0 {
        crate::tr!("ui-files-props-changed", count = change.changed)
    } else {
        crate::tr!(
            "ui-files-props-changed-failed",
            count = change.changed,
            failed = change.failed
        )
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
    let mut group = Group::titled(&crate::tr!("ui-files-props-security"));
    if let Some(context) = &details.security_context {
        group = group
            .row(row(crate::tr!("ui-files-props-selinux-context")).mono_value(context.clone()));
    }
    if details.acl {
        group = group.row(property_row(
            crate::tr!("ui-files-props-acl"),
            crate::tr!("ui-files-props-acl-entries"),
        ));
    }
    Some(group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_change_counts_what_it_did_and_what_it_could_not() {
        assert_eq!(
            done_line(TreeChange {
                changed: 1,
                failed: 0
            }),
            "Changed 1 item"
        );
        assert_eq!(
            done_line(TreeChange {
                changed: 12,
                failed: 0
            }),
            "Changed 12 items"
        );
        assert_eq!(
            done_line(TreeChange {
                changed: 12,
                failed: 2
            }),
            "Changed 12 items; 2 could not be changed"
        );
    }
}
