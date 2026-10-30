//! Which keys do what in an open preview: pure, so the map is tested.
//!
//! The host routes the keys every viewer shares (close, step, open, info,
//! fullscreen, dock). A viewer routes its own (zoom, rotate, wrap, play,
//! seek, page) with its own map beside its component.
//!
//! The rule that matters most: **the arrow keys choose the file, never the
//! content.** Left, Right, Up and Down move the selection in the file view
//! (Up and Down a line in a grid) and the preview follows, in a popup, in a
//! docked pane and in every viewer. A document scrolls with the wheel, Page
//! Up and Page Down, Home and End; a film seeks with Shift and an arrow, J
//! and L. None of those is an arrow key.

use std::cell::Cell;
use std::rc::Rc;

use freya::prelude::*;

use super::step::Step;

/// Set by a viewer while it is taking typed text (a find field): the host then
/// leaves every key to it, so typing an `f` or pressing Enter in the field does
/// not go full screen or open the file. A viewer sets it at render, so it still
/// stands while the key that ends the typing (Escape) is routed.
#[derive(Clone, Default)]
pub(super) struct KeyClaim(Rc<Cell<bool>>);

impl KeyClaim {
    pub(super) fn set(&self, typing: bool) {
        self.0.set(typing);
    }

    pub(super) fn get(&self) -> bool {
        self.0.get()
    }
}

/// Whether this preview has the keyboard for its own keys (zoom, scroll,
/// play). A popup or a full-screen sheet always has it. A docked pane leaves
/// the keyboard with the file list, and has it only after the person clicked
/// into the pane; Escape, or a click in the list, gives it back. The arrows
/// step the file in both cases.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Keyboard(Option<State<bool>>);

impl Keyboard {
    /// The popup's and the sheet's: always held.
    pub(super) fn always() -> Self {
        Keyboard(None)
    }

    /// The pane's: held while `held` is true.
    pub(super) fn pane(held: State<bool>) -> Self {
        Keyboard(Some(held))
    }

    /// Whether the preview's own keys are live now.
    pub(super) fn held(self) -> bool {
        self.0.is_none_or(|state| *state.peek())
    }

    /// Whether this is a pane's.
    pub(super) fn is_pane(self) -> bool {
        self.0.is_some()
    }

    /// Take the keyboard (a click in the pane).
    pub(super) fn take(self) {
        if let Some(mut state) = self.0 {
            state.set_if_modified(true);
        }
    }

    /// Give the keyboard back to the list.
    pub(super) fn release(self) {
        if let Some(mut state) = self.0 {
            state.set_if_modified(false);
        }
    }
}

/// What the host does for a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Leave fullscreen, else close.
    Close,
    Step(Step),
    /// Close and open the file as a double click does.
    Open,
    /// Close and show the Properties dialog.
    Properties,
    /// Show or hide the Info panel.
    Info,
    Fullscreen,
    /// Dock a popup, or undock a pane.
    SwitchMode,
}

/// The meaning of `key` in an open preview. `grid`: the files are laid out in
/// rows of tiles, so Up and Down step a line and not one file.
///
/// Space and Escape close (in a pane, Space is the viewer's: a document
/// scrolls a page, a film plays and pauses, and Escape gives the keyboard
/// back to the list; the host decides that, not this map). The four arrows,
/// with or without Alt, step the file in every viewer. Shift and an arrow,
/// and Ctrl and an arrow, are the viewer's.
pub fn route(key: &Key, modifiers: Modifiers, grid: bool, direction: crate::controls::direction::Direction) -> Option<Command> {
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let alt = modifiers.contains(Modifiers::ALT);
    let shift = modifiers.contains(Modifiers::SHIFT);
    let meta = modifiers.contains(Modifiers::META);
    if meta {
        return None;
    }
    // An arrow chooses the file unless Ctrl or Shift (without Alt) made it the viewer's.
    let steps = !ctrl && (alt || !shift);
    // Left steps back along the reading direction: to the previous file, or in a
    // window that reads from the right, the one at the right.
    let key = &direction.reading_key(key);
    match key {
        Key::Named(NamedKey::Escape) => Some(Command::Close),
        Key::Named(NamedKey::Enter) if alt && !ctrl => Some(Command::Properties),
        Key::Named(NamedKey::Enter) if !ctrl && !alt => Some(Command::Open),
        Key::Named(NamedKey::F11) => Some(Command::Fullscreen),
        Key::Named(NamedKey::ArrowLeft) if steps => Some(Command::Step(Step::Previous)),
        Key::Named(NamedKey::ArrowRight) if steps => Some(Command::Step(Step::Next)),
        Key::Named(NamedKey::ArrowUp) if steps => Some(Command::Step(if grid { Step::Up } else { Step::Previous })),
        Key::Named(NamedKey::ArrowDown) if steps => Some(Command::Step(if grid { Step::Down } else { Step::Next })),
        Key::Character(text) => {
            let text = text.as_str();
            match text {
                " " if !ctrl && !alt && !shift => Some(Command::Close),
                _ if ctrl && !alt && text.eq_ignore_ascii_case("i") => Some(Command::Properties),
                _ if !ctrl && alt && text.eq_ignore_ascii_case("p") => Some(Command::SwitchMode),
                _ if !ctrl && !alt && text.eq_ignore_ascii_case("i") => Some(Command::Info),
                _ if !ctrl && !alt && text.eq_ignore_ascii_case("f") => Some(Command::Fullscreen),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(key: NamedKey, modifiers: Modifiers, grid: bool) -> Option<Command> {
        route(&Key::Named(key), modifiers, grid, crate::controls::direction::Direction::Ltr)
    }

    fn character(text: &str, modifiers: Modifiers) -> Option<Command> {
        route(&Key::Character(text.to_owned().into()), modifiers, false, crate::controls::direction::Direction::Ltr)
    }

    #[test]
    fn space_and_escape_close() {
        assert_eq!(character(" ", Modifiers::empty()), Some(Command::Close));
        // Ctrl+Space is the file view's own (toggle the cursor row).
        assert_eq!(character(" ", Modifiers::CONTROL), None);
        // Shift+Space is a document's page back.
        assert_eq!(character(" ", Modifiers::SHIFT), None);
        assert_eq!(named(NamedKey::Escape, Modifiers::empty(), false), Some(Command::Close));
    }

    #[test]
    fn the_four_arrows_step_the_file_and_a_grid_steps_lines() {
        let none = Modifiers::empty();
        assert_eq!(named(NamedKey::ArrowRight, none, false), Some(Command::Step(Step::Next)));
        assert_eq!(named(NamedKey::ArrowLeft, none, false), Some(Command::Step(Step::Previous)));
        assert_eq!(named(NamedKey::ArrowDown, none, false), Some(Command::Step(Step::Next)));
        assert_eq!(named(NamedKey::ArrowUp, none, false), Some(Command::Step(Step::Previous)));
        assert_eq!(named(NamedKey::ArrowDown, none, true), Some(Command::Step(Step::Down)));
        assert_eq!(named(NamedKey::ArrowUp, none, true), Some(Command::Step(Step::Up)));
        // Left and Right step one file in a grid too.
        assert_eq!(named(NamedKey::ArrowRight, none, true), Some(Command::Step(Step::Next)));
    }

    #[test]
    fn no_arrow_is_a_viewers_without_a_modifier() {
        // Every viewer, whatever it shows: the same four arrows. The map has no
        // viewer parameter, so a document or a film cannot take one.
        let none = Modifiers::empty();
        for key in [NamedKey::ArrowLeft, NamedKey::ArrowRight, NamedKey::ArrowUp, NamedKey::ArrowDown] {
            assert!(matches!(named(key, none, false), Some(Command::Step(_))), "{key:?}");
            assert!(matches!(named(key, Modifiers::ALT, false), Some(Command::Step(_))), "Alt+{key:?}");
        }
    }

    #[test]
    fn shift_and_control_with_an_arrow_are_the_viewers() {
        // The player seeks with Shift and an arrow.
        assert_eq!(named(NamedKey::ArrowRight, Modifiers::SHIFT, false), None);
        assert_eq!(named(NamedKey::ArrowUp, Modifiers::SHIFT, false), None);
        assert_eq!(named(NamedKey::ArrowLeft, Modifiers::CONTROL, false), None);
    }

    #[test]
    fn enter_opens_and_alt_enter_or_ctrl_i_show_properties() {
        assert_eq!(named(NamedKey::Enter, Modifiers::empty(), false), Some(Command::Open));
        assert_eq!(named(NamedKey::Enter, Modifiers::ALT, false), Some(Command::Properties));
        assert_eq!(character("i", Modifiers::CONTROL), Some(Command::Properties));
        assert_eq!(character("i", Modifiers::empty()), Some(Command::Info));
    }

    #[test]
    fn letters_toggle_fullscreen_and_the_dock() {
        assert_eq!(character("f", Modifiers::empty()), Some(Command::Fullscreen));
        assert_eq!(character("F", Modifiers::SHIFT), Some(Command::Fullscreen));
        assert_eq!(named(NamedKey::F11, Modifiers::empty(), false), Some(Command::Fullscreen));
        assert_eq!(character("p", Modifiers::ALT), Some(Command::SwitchMode));
        assert_eq!(character("p", Modifiers::empty()), None);
        assert_eq!(character("x", Modifiers::empty()), None);
    }
}
