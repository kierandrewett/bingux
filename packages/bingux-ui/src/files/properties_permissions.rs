//! The permissions of a file as a grid: who may read, write and execute it,
//! with the numeric mode and "allow executing as program".
//!
//! The grid is always laid out. While the file is being read its boxes are
//! off and disabled, and a file the person does not own keeps them disabled,
//! so nothing moves when the facts arrive. The pure parts (which bit is
//! which, what a toggle does to the mode) are tested without a window.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::checkbox;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink};

/// Who a permission is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Who {
    Owner,
    Group,
    Others,
}

/// What a permission allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Can {
    Read,
    Write,
    Execute,
}

const WHO: [(Who, &str); 3] = [
    (Who::Owner, "Owner"),
    (Who::Group, "Group"),
    (Who::Others, "Others"),
];
const CAN: [Can; 3] = [Can::Read, Can::Write, Can::Execute];

/// The mode bit for `can` by `who`: `0o400` for the owner reading.
pub(super) fn bit(who: Who, can: Can) -> u32 {
    let class = match who {
        Who::Owner => 6,
        Who::Group => 3,
        Who::Others => 0,
    };
    let kind = match can {
        Can::Read => 4,
        Can::Write => 2,
        Can::Execute => 1,
    };
    kind << class
}

/// `mode` with the permission `who` `can` switched on or off. The other
/// bits, the setuid, setgid and sticky ones included, are left alone.
pub(super) fn toggled(mode: u32, who: Who, can: Can, on: bool) -> u32 {
    let bit = bit(who, can);
    if on { mode | bit } else { mode & !bit }
}

/// Whether anyone may execute the file.
pub(super) fn is_executable(mode: u32) -> bool {
    mode & 0o111 != 0
}

/// "Allow executing as program" switched on or off. On adds the execute bit
/// for each class that may read the file, as a file manager does, and for
/// the owner when nobody may read it. Off clears every execute bit.
pub(super) fn with_execute(mode: u32, on: bool) -> u32 {
    if !on {
        return mode & !0o111;
    }
    let readers = [Who::Owner, Who::Group, Who::Others]
        .into_iter()
        .filter(|who| mode & bit(*who, Can::Read) != 0)
        .collect::<Vec<_>>();
    if readers.is_empty() {
        return mode | bit(Who::Owner, Can::Execute);
    }
    readers
        .into_iter()
        .fold(mode, |mode, who| mode | bit(who, Can::Execute))
}

/// `rw-r--r--` for the permission bits of `mode`, with `s` or `t` where the
/// setuid, setgid or sticky bit takes the place of an execute bit.
pub(super) fn symbolic(mode: u32) -> String {
    let letter = |bit: u32, text: char| if mode & bit != 0 { text } else { '-' };
    // The special bit shows over the execute position: lower case when the
    // execute bit is also set, upper case when it is not.
    let special = |special: u32, execute: u32, lower: char| {
        match (mode & special != 0, mode & execute != 0) {
            (true, true) => lower,
            (true, false) => lower.to_ascii_uppercase(),
            (false, true) => 'x',
            (false, false) => '-',
        }
    };
    [
        letter(0o400, 'r'),
        letter(0o200, 'w'),
        special(0o4000, 0o100, 's'),
        letter(0o040, 'r'),
        letter(0o020, 'w'),
        special(0o2000, 0o010, 's'),
        letter(0o004, 'r'),
        letter(0o002, 'w'),
        special(0o1000, 0o001, 't'),
    ]
    .iter()
    .collect()
}

/// "0644 (rw-r--r--)".
pub(super) fn mode_line(mode: u32) -> String {
    format!("{:04o} ({})", mode & 0o7777, symbolic(mode))
}

/// The grid of boxes: a row each for the owner, the group and everyone else,
/// and a column each for reading, writing and executing. `mode` is `None`
/// until it is read. `execute` is what the third column is called.
#[derive(PartialEq)]
pub(super) struct PermissionGrid {
    pub mode: Option<u32>,
    pub editable: bool,
    pub execute: &'static str,
    pub on_change: EventHandler<u32>,
}

impl Component for PermissionGrid {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mode = self.mode;
        let enabled = self.editable && mode.is_some();
        let label_width = Size::px(crate::style::Dim::Label.px());
        let heads = ["Read", "Write", self.execute];

        let head = rect()
            .tw(
                &tokens,
                &[flex_row(), grow_children(), items(Align::Center), w_full(), h(Dim::Box7)],
            )
            .child(rect().width(label_width.clone()))
            .children(heads.into_iter().map(|name| {
                rect()
                    .width(Size::flex(1.0))
                    .tw(&tokens, &[flex_row(), center()])
                    .child(CAPTION.in_ink(Ink::Muted).label(&tokens, name))
                    .into_element()
            }));

        let lines = WHO.into_iter().map(|(who, name)| {
            let boxes = CAN.into_iter().map(move |can| {
                let on = mode.is_some_and(|mode| mode & bit(who, can) != 0);
                let on_change = self.on_change.clone();
                rect()
                    .width(Size::flex(1.0))
                    .tw(&tokens, &[flex_row(), center()])
                    .child(
                        checkbox(on, move |next: bool| {
                            if let Some(mode) = mode {
                                on_change.call(toggled(mode, who, can, next));
                            }
                        })
                        .enabled(enabled),
                    )
                    .into_element()
            });
            rect()
                .tw(
                    &tokens,
                    &[flex_row(), grow_children(), items(Align::Center), w_full(), h(Dim::Box9)],
                )
                .child(
                    rect()
                        .width(label_width.clone())
                        .child(BODY.label(&tokens, name)),
                )
                .children(boxes)
                .into_element()
        });

        rect()
            .component("PermissionGrid")
            .tw(&tokens, &[flex_col(), w_full()])
            .child(head)
            .children(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_permission_has_its_own_bit() {
        assert_eq!(bit(Who::Owner, Can::Read), 0o400);
        assert_eq!(bit(Who::Owner, Can::Write), 0o200);
        assert_eq!(bit(Who::Group, Can::Execute), 0o010);
        assert_eq!(bit(Who::Others, Can::Read), 0o004);
    }

    #[test]
    fn toggling_one_box_changes_one_bit_and_keeps_the_special_ones() {
        assert_eq!(toggled(0o644, Who::Group, Can::Write, true), 0o664);
        assert_eq!(toggled(0o664, Who::Group, Can::Write, false), 0o644);
        assert_eq!(toggled(0o644, Who::Group, Can::Write, false), 0o644);
        assert_eq!(toggled(0o4755, Who::Others, Can::Execute, false), 0o4754);
    }

    #[test]
    fn allowing_execution_follows_who_can_read() {
        assert_eq!(with_execute(0o644, true), 0o755);
        assert_eq!(with_execute(0o600, true), 0o700);
        assert_eq!(with_execute(0o640, true), 0o750);
        // Nobody can read it: the owner gets the bit and no one else.
        assert_eq!(with_execute(0o000, true), 0o100);
        assert_eq!(with_execute(0o755, false), 0o644);
        assert_eq!(with_execute(0o4755, false), 0o4644);
        assert!(is_executable(0o100) && !is_executable(0o644));
    }

    #[test]
    fn the_symbolic_form_shows_special_bits() {
        assert_eq!(symbolic(0o644), "rw-r--r--");
        assert_eq!(symbolic(0o755), "rwxr-xr-x");
        assert_eq!(symbolic(0o000), "---------");
        assert_eq!(symbolic(0o4755), "rwsr-xr-x");
        assert_eq!(symbolic(0o4644), "rwSr--r--");
        assert_eq!(symbolic(0o1777), "rwxrwxrwt");
        assert_eq!(symbolic(0o1776), "rwxrwxrwT");
        assert_eq!(mode_line(0o640), "0640 (rw-r-----)");
        assert_eq!(mode_line(0o2755), "2755 (rwxr-sr-x)");
    }
}
