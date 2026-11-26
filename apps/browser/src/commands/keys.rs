//! Which key is which command: the conventions of each platform.

use bingux_ui::web_view::input::EditCommand;
use freya::prelude::{Key, Modifiers, NamedKey};

use super::Command;

/// Which family of conventions applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
    Mac,
}

impl Platform {
    /// The platform this build runs on.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// The shortcut of the window: a key typed anywhere in it.
    pub fn window_command(self, key: &Key, modifiers: Modifiers) -> Option<Command> {
        let held = Held::of(modifiers);
        let plain = held.main(self, false, false);
        let shifted = held.main(self, true, false);
        let mac = self == Platform::Mac;

        // Back, Forward and the function keys.
        if held.only_alt() && !mac {
            match key {
                Key::Named(NamedKey::ArrowLeft) => return Some(Command::Back),
                Key::Named(NamedKey::ArrowRight) => return Some(Command::Forward),
                _ if Held::is(key, "d") => return Some(Command::FocusAddress),
                _ => {}
            }
        }
        if mac && held.main(self, false, false) {
            if Held::is(key, "[") {
                return Some(Command::Back);
            }
            if Held::is(key, "]") {
                return Some(Command::Forward);
            }
        }
        if held.none() {
            match key {
                Key::Named(NamedKey::F5) => return Some(Command::Reload),
                Key::Named(NamedKey::F3) => return Some(Command::FindNext),
                Key::Named(NamedKey::F11) => return Some(Command::Fullscreen),
                Key::Named(NamedKey::F12) => return Some(Command::DevTools),
                _ => {}
            }
        }
        if held.only_shift() {
            match key {
                Key::Named(NamedKey::F3) => return Some(Command::FindPrevious),
                Key::Named(NamedKey::F5) => return Some(Command::ReloadBypassCache),
                _ => {}
            }
        }
        if !mac && plain {
            if let Key::Named(named_key) = key {
                match named_key {
                    NamedKey::F5 => return Some(Command::ReloadBypassCache),
                    NamedKey::F4 => return Some(Command::CloseTab),
                    NamedKey::Tab => return Some(Command::NextTab),
                    NamedKey::PageDown => return Some(Command::NextTab),
                    NamedKey::PageUp => return Some(Command::PreviousTab),
                    _ => {}
                }
            }
        }
        if shifted {
            if let Key::Named(named_key) = key {
                match named_key {
                    NamedKey::Tab => return Some(Command::PreviousTab),
                    NamedKey::PageDown => return Some(Command::MoveTabRight),
                    NamedKey::PageUp => return Some(Command::MoveTabLeft),
                    _ => {}
                }
            }
            match key {
                _ if Held::is(key, "t") => return Some(Command::ReopenTab),
                _ if Held::is(key, "p") => return Some(Command::NewPrivateWindow),
                _ if Held::is(key, "r") => return Some(Command::ReloadBypassCache),
                _ if Held::is(key, "y") => return Some(Command::Downloads),
                _ if Held::is(key, "g") => return Some(Command::FindPrevious),
                // Plus is Shift and equals on most layouts.
                _ if Held::is(key, "+") => return Some(Command::ZoomIn),
                _ if Held::is(key, "]") && mac => return Some(Command::NextTab),
                _ if Held::is(key, "[") && mac => return Some(Command::PreviousTab),
                _ => {}
            }
        }
        if mac && held.main(self, false, true) {
            match key {
                Key::Named(NamedKey::ArrowRight) => return Some(Command::NextTab),
                Key::Named(NamedKey::ArrowLeft) => return Some(Command::PreviousTab),
                _ => {}
            }
        }
        if plain {
            match key {
                _ if Held::is(key, "t") => return Some(Command::NewTab),
                _ if Held::is(key, "w") => return Some(Command::CloseTab),
                _ if Held::is(key, "l") => return Some(Command::FocusAddress),
                _ if Held::is(key, "f") => return Some(Command::Find),
                _ if Held::is(key, "g") => return Some(Command::FindNext),
                _ if Held::is(key, "r") => return Some(Command::Reload),
                _ if Held::is(key, "i") => return Some(Command::PageInfo),
                _ if Held::is(key, "u") => return Some(Command::ViewSource),
                _ if Held::is(key, "p") => return Some(Command::Print),
                _ if Held::is(key, ",") => return Some(Command::Settings),
                _ if Held::is(key, "+") || Held::is(key, "=") => return Some(Command::ZoomIn),
                _ if Held::is(key, "-") => return Some(Command::ZoomOut),
                _ if Held::is(key, "0") => return Some(Command::ZoomReset),
                _ => {}
            }
            // Ctrl+1 to 8 is a tab on Windows and for everybody on macOS; 9 is the last.
            if self != Platform::Linux {
                if let Some(command) = Command::digit(key) {
                    return Some(command);
                }
            }
        }
        if self == Platform::Linux && held.only_alt() {
            if let Some(command) = Command::digit(key) {
                return Some(command);
            }
        }
        None
    }

    /// A shortcut for a key typed while the page has the focus: the window's, then
    /// the editing commands. Everything else belongs to the page.
    pub fn page_command(self, key: &Key, modifiers: Modifiers) -> Option<Command> {
        if let Some(command) = self.window_command(key, modifiers) {
            return Some(command);
        }
        let held = Held::of(modifiers);
        if held.main(self, false, false) {
            return match key {
                _ if Held::is(key, "x") => Some(Command::Edit(EditCommand::Cut)),
                _ if Held::is(key, "c") => Some(Command::Edit(EditCommand::Copy)),
                _ if Held::is(key, "v") => Some(Command::Edit(EditCommand::Paste)),
                _ if Held::is(key, "a") => Some(Command::Edit(EditCommand::SelectAll)),
                _ if Held::is(key, "z") => Some(Command::Edit(EditCommand::Undo)),
                _ if Held::is(key, "y") && self != Platform::Mac => {
                    Some(Command::Edit(EditCommand::Redo))
                }
                _ => None,
            };
        }
        if held.main(self, true, false) {
            return match key {
                _ if Held::is(key, "v") => Some(Command::Edit(EditCommand::PastePlain)),
                _ if Held::is(key, "z") => Some(Command::Edit(EditCommand::Redo)),
                _ => None,
            };
        }
        // Shift+Insert and Ctrl+Insert are the old paste and copy.
        if held.only_shift() && Held::named(key, NamedKey::Insert) {
            return Some(Command::Edit(EditCommand::Paste));
        }
        None
    }
}

/// The four modifiers a shortcut is made of. Lock keys do not matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Held {
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
}

impl Held {
    fn of(modifiers: Modifiers) -> Self {
        Self {
            ctrl: modifiers.contains(Modifiers::CONTROL),
            alt: modifiers.contains(Modifiers::ALT),
            shift: modifiers.contains(Modifiers::SHIFT),
            meta: modifiers.contains(Modifiers::META),
        }
    }

    /// Exactly the main modifier of `platform` and `extra`.
    fn main(self, platform: Platform, shift: bool, alt: bool) -> bool {
        let (main, other) = match platform {
            Platform::Mac => (self.meta, self.ctrl),
            Platform::Linux | Platform::Windows => (self.ctrl, self.meta),
        };
        main && !other && self.shift == shift && self.alt == alt
    }

    fn none(self) -> bool {
        !self.ctrl && !self.alt && !self.shift && !self.meta
    }

    fn only_alt(self) -> bool {
        self.alt && !self.ctrl && !self.shift && !self.meta
    }

    fn only_shift(self) -> bool {
        self.shift && !self.ctrl && !self.alt && !self.meta
    }

    /// Whether `key` is the character `text`, in either case.
    pub(super) fn is(key: &Key, text: &str) -> bool {
        matches!(key, Key::Character(typed) if typed.eq_ignore_ascii_case(text))
    }

    /// Whether `key` is the named key `name`.
    pub(super) fn named(key: &Key, name: NamedKey) -> bool {
        *key == Key::Named(name)
    }
}

impl Command {
    /// Tab `n` for a digit key; 9 is the last tab.
    fn digit(key: &Key) -> Option<Command> {
        let Key::Character(text) = key else {
            return None;
        };
        let digit = text.chars().next()?.to_digit(10)?;
        match digit {
            1..=8 => Some(Command::GoToTab(digit as usize)),
            9 => Some(Command::LastTab),
            _ => None,
        }
    }
}
