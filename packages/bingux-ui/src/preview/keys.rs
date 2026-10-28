//! Which keys do what in an open preview: pure, so the map is tested.
//!
//! The host routes the keys every viewer shares (close, step, open, info,
//! fullscreen, dock). A viewer routes its own (zoom, rotate, wrap, play,
//! seek, page) with its own map beside its component.

use bingux_preview_core::ViewerId;
use freya::prelude::*;

use super::step::Step;

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

/// The meaning of `key` in an open preview of a file shown by `viewer`.
/// `grid`: the files are laid out in rows of tiles, so Up and Down step a
/// line and not one file.
///
/// Space closes, except in the media viewer, where it plays and pauses and
/// the viewer routes it itself. Left and Right seek in the media viewer, which
/// routes them itself; Alt with an arrow steps the file from any viewer. Up
/// and Down scroll a document instead of stepping.
pub fn route(key: &Key, modifiers: Modifiers, viewer: ViewerId, grid: bool) -> Option<Command> {
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let alt = modifiers.contains(Modifiers::ALT);
    let meta = modifiers.contains(Modifiers::META);
    if meta {
        return None;
    }
    let media = viewer == ViewerId::Media;
    let document = viewer == ViewerId::Document;
    match key {
        Key::Named(NamedKey::Escape) => Some(Command::Close),
        Key::Named(NamedKey::Enter) if alt && !ctrl => Some(Command::Properties),
        Key::Named(NamedKey::Enter) if !ctrl && !alt => Some(Command::Open),
        Key::Named(NamedKey::F11) => Some(Command::Fullscreen),
        Key::Named(NamedKey::ArrowLeft) if !ctrl && (alt || !media) => Some(Command::Step(Step::Previous)),
        Key::Named(NamedKey::ArrowRight) if !ctrl && (alt || !media) => Some(Command::Step(Step::Next)),
        Key::Named(NamedKey::ArrowUp) if !ctrl && (alt || !document && !media) => {
            Some(Command::Step(if grid { Step::Up } else { Step::Previous }))
        }
        Key::Named(NamedKey::ArrowDown) if !ctrl && (alt || !document && !media) => {
            Some(Command::Step(if grid { Step::Down } else { Step::Next }))
        }
        Key::Character(text) => {
            let text = text.as_str();
            match text {
                " " if !ctrl && !alt && !media => Some(Command::Close),
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

    fn named(key: NamedKey, modifiers: Modifiers, viewer: ViewerId, grid: bool) -> Option<Command> {
        route(&Key::Named(key), modifiers, viewer, grid)
    }

    fn character(text: &str, modifiers: Modifiers, viewer: ViewerId) -> Option<Command> {
        route(&Key::Character(text.to_owned().into()), modifiers, viewer, false)
    }

    #[test]
    fn space_closes_everywhere_but_the_player() {
        for viewer in [ViewerId::Image, ViewerId::Text, ViewerId::Document, ViewerId::Folder, ViewerId::Card, ViewerId::Archive, ViewerId::Font] {
            assert_eq!(character(" ", Modifiers::empty(), viewer), Some(Command::Close), "{viewer:?}");
        }
        assert_eq!(character(" ", Modifiers::empty(), ViewerId::Media), None, "the player plays and pauses");
        // Ctrl+Space is the file view's own (toggle the cursor row).
        assert_eq!(character(" ", Modifiers::CONTROL, ViewerId::Image), None);
    }

    #[test]
    fn escape_always_closes() {
        for viewer in [ViewerId::Image, ViewerId::Media, ViewerId::Document] {
            assert_eq!(named(NamedKey::Escape, Modifiers::empty(), viewer, false), Some(Command::Close));
        }
    }

    #[test]
    fn arrows_step_files_and_a_grid_steps_lines() {
        let none = Modifiers::empty();
        assert_eq!(named(NamedKey::ArrowRight, none, ViewerId::Image, false), Some(Command::Step(Step::Next)));
        assert_eq!(named(NamedKey::ArrowLeft, none, ViewerId::Image, false), Some(Command::Step(Step::Previous)));
        assert_eq!(named(NamedKey::ArrowDown, none, ViewerId::Image, false), Some(Command::Step(Step::Next)));
        assert_eq!(named(NamedKey::ArrowDown, none, ViewerId::Image, true), Some(Command::Step(Step::Down)));
        assert_eq!(named(NamedKey::ArrowUp, none, ViewerId::Text, true), Some(Command::Step(Step::Up)));
    }

    #[test]
    fn the_player_and_the_document_keep_their_arrows_and_alt_steps_anyway() {
        let none = Modifiers::empty();
        assert_eq!(named(NamedKey::ArrowRight, none, ViewerId::Media, false), None, "seek");
        assert_eq!(named(NamedKey::ArrowDown, none, ViewerId::Document, false), None, "scroll");
        assert_eq!(named(NamedKey::ArrowRight, Modifiers::ALT, ViewerId::Media, false), Some(Command::Step(Step::Next)));
        assert_eq!(named(NamedKey::ArrowDown, Modifiers::ALT, ViewerId::Document, false), Some(Command::Step(Step::Next)));
        // A document still steps with Left and Right.
        assert_eq!(named(NamedKey::ArrowLeft, none, ViewerId::Document, false), Some(Command::Step(Step::Previous)));
    }

    #[test]
    fn enter_opens_and_alt_enter_or_ctrl_i_show_properties() {
        assert_eq!(named(NamedKey::Enter, Modifiers::empty(), ViewerId::Image, false), Some(Command::Open));
        assert_eq!(named(NamedKey::Enter, Modifiers::ALT, ViewerId::Image, false), Some(Command::Properties));
        assert_eq!(character("i", Modifiers::CONTROL, ViewerId::Image), Some(Command::Properties));
        assert_eq!(character("i", Modifiers::empty(), ViewerId::Image), Some(Command::Info));
    }

    #[test]
    fn letters_toggle_fullscreen_and_the_dock() {
        assert_eq!(character("f", Modifiers::empty(), ViewerId::Media), Some(Command::Fullscreen));
        assert_eq!(character("F", Modifiers::SHIFT, ViewerId::Media), Some(Command::Fullscreen));
        assert_eq!(named(NamedKey::F11, Modifiers::empty(), ViewerId::Image, false), Some(Command::Fullscreen));
        assert_eq!(character("p", Modifiers::ALT, ViewerId::Image), Some(Command::SwitchMode));
        assert_eq!(character("p", Modifiers::empty(), ViewerId::Image), None);
        assert_eq!(character("x", Modifiers::empty(), ViewerId::Image), None);
    }
}
