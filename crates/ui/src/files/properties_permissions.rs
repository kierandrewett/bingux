//! The permissions of one or several items as a grid: who may read, write
//! and execute them, with the numeric mode and "allow executing as program".
//!
//! The grid is always laid out. While the items are being read its boxes are
//! off and disabled, and items the person does not own keep them disabled,
//! so nothing moves when the facts arrive. Several items with different
//! modes show a box as mixed (a dash) where they differ. The pure parts
//! (which bit is which, what a press does to each mode) are tested without
//! a window.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::{CheckState, checkbox};
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

/// The permission bits of every item the Permissions tab acts on, one mode
/// each. A box is on when every item has the bit, off when none has it and
/// mixed when only some do; pressing it sets the bit on every item, or, if
/// it was on, clears it on every item. Each item keeps the bits that were
/// not pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModeSet {
    modes: Vec<u32>,
}

impl ModeSet {
    pub(super) fn new(modes: Vec<u32>) -> Self {
        Self { modes }
    }

    pub(super) fn modes(&self) -> &[u32] {
        &self.modes
    }

    /// The mode every item has, or `None` when they differ.
    pub(super) fn common(&self) -> Option<u32> {
        let first = *self.modes.first()?;
        self.modes.iter().all(|mode| *mode == first).then_some(first)
    }

    /// Whether `has` holds for all, none or some of the items.
    fn tally(&self, has: impl Fn(u32) -> bool) -> CheckState {
        let count = self.modes.iter().filter(|mode| has(**mode)).count();
        if count == 0 {
            CheckState::Off
        } else if count == self.modes.len() {
            CheckState::On
        } else {
            CheckState::Mixed
        }
    }

    /// The state of the box for `who` and `can`.
    pub(super) fn state(&self, who: Who, can: Can) -> CheckState {
        self.tally(|mode| mode & bit(who, can) != 0)
    }

    /// Every item's mode with the box for `who` and `can` pressed to `on`.
    pub(super) fn toggled(&self, who: Who, can: Can, on: bool) -> Vec<u32> {
        self.modes.iter().map(|mode| toggled(*mode, who, can, on)).collect()
    }

    /// Whether the items may be executed: on when each can be by someone.
    pub(super) fn executable(&self) -> CheckState {
        self.tally(is_executable)
    }

    /// Every item's mode after "Allow executing as program" is set to `on`.
    pub(super) fn with_execute(&self, on: bool) -> Vec<u32> {
        self.modes.iter().map(|mode| with_execute(*mode, on)).collect()
    }

    /// The numeric mode (`0644`), or "Mixed" when the items differ.
    pub(super) fn numeric(&self) -> String {
        self.common()
            .map_or_else(|| "Mixed".to_owned(), |mode| format!("{:04o}", mode & 0o7777))
    }

    /// The mode in letters (`rw-r--r--`), or "Mixed" when the items differ.
    pub(super) fn letters(&self) -> String {
        self.common().map_or_else(|| "Mixed".to_owned(), symbolic)
    }

    /// Which of the set user ID, set group ID and sticky bits are on.
    pub(super) fn special_line(&self) -> String {
        let kinds = [(0o4000, "Set user ID"), (0o2000, "Set group ID"), (0o1000, "Sticky")];
        let found: Vec<String> = kinds
            .into_iter()
            .filter_map(|(bit, name)| match self.tally(|mode| mode & bit != 0) {
                CheckState::On => Some(name.to_owned()),
                CheckState::Mixed => Some(format!("{name} (some)")),
                CheckState::Off => None,
            })
            .collect();
        if found.is_empty() { "None".to_owned() } else { found.join(", ") }
    }
}

/// The grid of boxes: a row each for the owner, the group and everyone else,
/// and a column each for reading, writing and executing. `modes` is `None`
/// until they are read. `execute` is what the third column is called. A press
/// reports who, what and the box's next value.
#[derive(PartialEq)]
pub(super) struct PermissionGrid {
    pub modes: Option<ModeSet>,
    pub editable: bool,
    pub execute: &'static str,
    pub on_change: EventHandler<(Who, Can, bool)>,
}

impl Component for PermissionGrid {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let modes = self.modes.clone();
        let enabled = self.editable && modes.is_some();
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
            let boxes = CAN.into_iter().map(|can| {
                let state = modes
                    .as_ref()
                    .map_or(CheckState::Off, |modes| modes.state(who, can));
                let on_change = self.on_change.clone();
                rect()
                    .width(Size::flex(1.0))
                    .tw(&tokens, &[flex_row(), center()])
                    .child(
                        checkbox(state, move |next: bool| on_change.call((who, can, next)))
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
    }

    #[test]
    fn a_box_is_mixed_where_the_items_differ() {
        let set = ModeSet::new(vec![0o644, 0o664, 0o600]);
        assert_eq!(set.state(Who::Owner, Can::Read), CheckState::On);
        assert_eq!(set.state(Who::Owner, Can::Execute), CheckState::Off);
        assert_eq!(set.state(Who::Group, Can::Write), CheckState::Mixed);
        assert_eq!(set.state(Who::Group, Can::Read), CheckState::Mixed);
        assert_eq!(set.state(Who::Others, Can::Write), CheckState::Off);
        assert_eq!(set.common(), None);
        assert_eq!(set.numeric(), "Mixed");
        assert_eq!(set.letters(), "Mixed");
        let same = ModeSet::new(vec![0o640, 0o640]);
        assert_eq!((same.numeric().as_str(), same.letters().as_str()), ("0640", "rw-r-----"));
    }

    #[test]
    fn pressing_a_mixed_box_turns_it_on_for_every_item_and_keeps_the_rest() {
        let set = ModeSet::new(vec![0o644, 0o664, 0o4600]);
        // A mixed box reports `true` as its next value.
        assert_eq!(set.toggled(Who::Group, Can::Write, true), [0o664, 0o664, 0o4620]);
        assert_eq!(set.toggled(Who::Owner, Can::Write, false), [0o444, 0o464, 0o4400]);
        // A box that is on is pressed off everywhere.
        assert_eq!(set.toggled(Who::Owner, Can::Read, false), [0o244, 0o264, 0o4200]);
    }

    #[test]
    fn allowing_execution_is_decided_per_item() {
        let set = ModeSet::new(vec![0o644, 0o600, 0o755]);
        assert_eq!(set.executable(), CheckState::Mixed);
        assert_eq!(set.with_execute(true), [0o755, 0o700, 0o755]);
        assert_eq!(set.with_execute(false), [0o644, 0o600, 0o644]);
        assert_eq!(ModeSet::new(vec![0o755, 0o700]).executable(), CheckState::On);
        assert_eq!(ModeSet::new(vec![0o644]).executable(), CheckState::Off);
    }

    #[test]
    fn the_special_bits_are_named_and_mixed_ones_say_some() {
        assert_eq!(ModeSet::new(vec![0o644]).special_line(), "None");
        assert_eq!(ModeSet::new(vec![0o4755, 0o6755]).special_line(), "Set user ID, Set group ID (some)");
        assert_eq!(ModeSet::new(vec![0o1777]).special_line(), "Sticky");
    }
}
