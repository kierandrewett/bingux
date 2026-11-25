//! Keyboard shortcuts, as data: a key and the modifiers held, and the command
//! it is. The same table serves the window (a key typed anywhere in it) and
//! the page (a key typed while the page has the focus, where the page keeps
//! everything that is not a browser shortcut).
//!
//! The conventions are the platform's: Ctrl on Linux and Windows, Command on
//! macOS; Alt+1 to 9 for a tab on Linux and Ctrl+1 to 9 elsewhere, as Firefox
//! does.

use bingux_ui::web_view::input::EditCommand;
use freya::prelude::{Key, Modifiers, NamedKey};

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

    /// The name of the main shortcut modifier, for menus.
    pub const fn main_key(self) -> &'static str {
        match self {
            Self::Mac => "\u{2318}",
            Self::Linux | Self::Windows => "Ctrl",
        }
    }
}

/// What a shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    NewTab,
    NewPrivateWindow,
    CloseTab,
    ReopenTab,
    NextTab,
    PreviousTab,
    /// Tab `n`, counted from 1.
    GoToTab(usize),
    LastTab,
    MoveTabLeft,
    MoveTabRight,
    FocusAddress,
    Find,
    FindNext,
    FindPrevious,
    Reload,
    ReloadBypassCache,
    Back,
    Forward,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fullscreen,
    Downloads,
    Settings,
    PageInfo,
    ViewSource,
    Print,
    MainMenu,
    /// An editing command for the page, only while the page has the focus.
    Edit(EditCommand),
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
}

fn is(key: &Key, text: &str) -> bool {
    matches!(key, Key::Character(typed) if typed.eq_ignore_ascii_case(text))
}

fn named(key: &Key, name: NamedKey) -> bool {
    *key == Key::Named(name)
}

/// The shortcut of the window: a key typed anywhere in it.
pub fn window_command(platform: Platform, key: &Key, modifiers: Modifiers) -> Option<Command> {
    let held = Held::of(modifiers);
    let plain = held.main(platform, false, false);
    let shifted = held.main(platform, true, false);
    let mac = platform == Platform::Mac;

    // Back, Forward and the function keys.
    if held.only_alt() && !mac {
        match key {
            Key::Named(NamedKey::ArrowLeft) => return Some(Command::Back),
            Key::Named(NamedKey::ArrowRight) => return Some(Command::Forward),
            _ if is(key, "d") => return Some(Command::FocusAddress),
            _ => {}
        }
    }
    if mac && held.main(platform, false, false) {
        if is(key, "[") {
            return Some(Command::Back);
        }
        if is(key, "]") {
            return Some(Command::Forward);
        }
    }
    if held.none() {
        match key {
            Key::Named(NamedKey::F5) => return Some(Command::Reload),
            Key::Named(NamedKey::F3) => return Some(Command::FindNext),
            Key::Named(NamedKey::F11) => return Some(Command::Fullscreen),
            Key::Named(NamedKey::F10) => return Some(Command::MainMenu),
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
            _ if is(key, "t") => return Some(Command::ReopenTab),
            _ if is(key, "p") => return Some(Command::NewPrivateWindow),
            _ if is(key, "r") => return Some(Command::ReloadBypassCache),
            _ if is(key, "y") => return Some(Command::Downloads),
            _ if is(key, "g") => return Some(Command::FindPrevious),
            // Plus is Shift and equals on most layouts.
            _ if is(key, "+") => return Some(Command::ZoomIn),
            _ if is(key, "]") && mac => return Some(Command::NextTab),
            _ if is(key, "[") && mac => return Some(Command::PreviousTab),
            _ => {}
        }
    }
    if mac && held.main(platform, false, true) {
        match key {
            Key::Named(NamedKey::ArrowRight) => return Some(Command::NextTab),
            Key::Named(NamedKey::ArrowLeft) => return Some(Command::PreviousTab),
            _ => {}
        }
    }
    if plain {
        match key {
            _ if is(key, "t") => return Some(Command::NewTab),
            _ if is(key, "w") => return Some(Command::CloseTab),
            _ if is(key, "l") => return Some(Command::FocusAddress),
            _ if is(key, "f") => return Some(Command::Find),
            _ if is(key, "g") => return Some(Command::FindNext),
            _ if is(key, "r") => return Some(Command::Reload),
            _ if is(key, "i") => return Some(Command::PageInfo),
            _ if is(key, "u") => return Some(Command::ViewSource),
            _ if is(key, "p") => return Some(Command::Print),
            _ if is(key, ",") => return Some(Command::Settings),
            _ if is(key, "+") || is(key, "=") => return Some(Command::ZoomIn),
            _ if is(key, "-") => return Some(Command::ZoomOut),
            _ if is(key, "0") => return Some(Command::ZoomReset),
            _ => {}
        }
        // Ctrl+1 to 8 is a tab on Windows and for everybody on macOS; 9 is the last.
        if platform != Platform::Linux {
            if let Some(command) = digit(key) {
                return Some(command);
            }
        }
    }
    if platform == Platform::Linux && held.only_alt() {
        if let Some(command) = digit(key) {
            return Some(command);
        }
    }
    None
}

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

/// A shortcut for a key typed while the page has the focus: the window's, then
/// the editing commands. Everything else belongs to the page.
pub fn page_command(platform: Platform, key: &Key, modifiers: Modifiers) -> Option<Command> {
    if let Some(command) = window_command(platform, key, modifiers) {
        return Some(command);
    }
    let held = Held::of(modifiers);
    if held.main(platform, false, false) {
        return match key {
            _ if is(key, "x") => Some(Command::Edit(EditCommand::Cut)),
            _ if is(key, "c") => Some(Command::Edit(EditCommand::Copy)),
            _ if is(key, "v") => Some(Command::Edit(EditCommand::Paste)),
            _ if is(key, "a") => Some(Command::Edit(EditCommand::SelectAll)),
            _ if is(key, "z") => Some(Command::Edit(EditCommand::Undo)),
            _ if is(key, "y") && platform != Platform::Mac => Some(Command::Edit(EditCommand::Redo)),
            _ => None,
        };
    }
    if held.main(platform, true, false) {
        return match key {
            _ if is(key, "v") => Some(Command::Edit(EditCommand::PastePlain)),
            _ if is(key, "z") => Some(Command::Edit(EditCommand::Redo)),
            _ => None,
        };
    }
    // Shift+Insert and Ctrl+Insert are the old paste and copy.
    if held.only_shift() && named(key, NamedKey::Insert) {
        return Some(Command::Edit(EditCommand::Paste));
    }
    None
}

/// The shortcut shown beside a menu entry.
pub fn label(platform: Platform, command: Command) -> Option<String> {
    let main = platform.main_key();
    let join = |keys: &str| {
        if platform == Platform::Mac {
            keys.replace('+', "")
        } else {
            keys.to_string()
        }
    };
    let text = match command {
        Command::NewTab => format!("{main}+T"),
        Command::NewPrivateWindow => format!("{main}+Shift+P"),
        Command::CloseTab => format!("{main}+W"),
        Command::ReopenTab => format!("{main}+Shift+T"),
        Command::FocusAddress => format!("{main}+L"),
        Command::Find => format!("{main}+F"),
        Command::FindNext => "F3".to_string(),
        Command::FindPrevious => "Shift+F3".to_string(),
        Command::Reload => format!("{main}+R"),
        Command::ReloadBypassCache => format!("{main}+Shift+R"),
        Command::Back => if platform == Platform::Mac { format!("{main}+[") } else { "Alt+Left".to_string() },
        Command::Forward => if platform == Platform::Mac { format!("{main}+]") } else { "Alt+Right".to_string() },
        Command::ZoomIn if platform == Platform::Mac => return Some("\u{2318}+".to_string()),
        Command::ZoomIn => format!("{main}++"),
        Command::ZoomOut => format!("{main}+-"),
        Command::ZoomReset => format!("{main}+0"),
        Command::Fullscreen => "F11".to_string(),
        Command::Downloads => format!("{main}+Shift+Y"),
        Command::Settings => format!("{main}+,"),
        Command::PageInfo => format!("{main}+I"),
        Command::ViewSource => format!("{main}+U"),
        Command::Print => format!("{main}+P"),
        Command::NextTab => format!("{main}+Tab"),
        Command::PreviousTab => format!("{main}+Shift+Tab"),
        _ => return None,
    };
    Some(join(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(text: &str) -> Key {
        Key::Character(text.to_string())
    }

    const CTRL: Modifiers = Modifiers::CONTROL;

    #[test]
    fn the_basic_ctrl_shortcuts() {
        let linux = |key: &str, modifiers| window_command(Platform::Linux, &ch(key), modifiers);
        assert_eq!(linux("t", CTRL), Some(Command::NewTab));
        assert_eq!(linux("w", CTRL), Some(Command::CloseTab));
        assert_eq!(linux("l", CTRL), Some(Command::FocusAddress));
        assert_eq!(linux("f", CTRL), Some(Command::Find));
        assert_eq!(linux("r", CTRL), Some(Command::Reload));
        assert_eq!(linux("T", CTRL | Modifiers::SHIFT), Some(Command::ReopenTab), "shift makes the letter a capital");
        assert_eq!(linux("R", CTRL | Modifiers::SHIFT), Some(Command::ReloadBypassCache));
        assert_eq!(linux("=", CTRL), Some(Command::ZoomIn));
        assert_eq!(linux("+", CTRL | Modifiers::SHIFT), Some(Command::ZoomIn), "plus is typed with Shift on most layouts");
        assert_eq!(linux("-", CTRL), Some(Command::ZoomOut));
        assert_eq!(linux("0", CTRL), Some(Command::ZoomReset));
    }

    #[test]
    fn extra_modifiers_make_it_another_key() {
        assert_eq!(window_command(Platform::Linux, &ch("t"), CTRL | Modifiers::ALT), None);
        assert_eq!(window_command(Platform::Linux, &ch("t"), Modifiers::default()), None, "a letter is a letter");
        assert_eq!(window_command(Platform::Linux, &ch("t"), CTRL | Modifiers::CAPS_LOCK), Some(Command::NewTab), "locks do not count");
        assert_eq!(window_command(Platform::Linux, &ch("t"), Modifiers::META), None, "Super is not Ctrl");
    }

    #[test]
    fn tabs_move_with_tab_and_the_page_keys() {
        let key = |named: NamedKey, modifiers| window_command(Platform::Linux, &Key::Named(named), modifiers);
        assert_eq!(key(NamedKey::Tab, CTRL), Some(Command::NextTab));
        assert_eq!(key(NamedKey::Tab, CTRL | Modifiers::SHIFT), Some(Command::PreviousTab));
        assert_eq!(key(NamedKey::PageDown, CTRL), Some(Command::NextTab));
        assert_eq!(key(NamedKey::PageUp, CTRL), Some(Command::PreviousTab));
        assert_eq!(key(NamedKey::PageUp, CTRL | Modifiers::SHIFT), Some(Command::MoveTabLeft));
        assert_eq!(key(NamedKey::PageDown, CTRL | Modifiers::SHIFT), Some(Command::MoveTabRight));
    }

    #[test]
    fn alt_digits_pick_a_tab_on_linux_and_ctrl_digits_elsewhere() {
        assert_eq!(window_command(Platform::Linux, &ch("3"), Modifiers::ALT), Some(Command::GoToTab(3)));
        assert_eq!(window_command(Platform::Linux, &ch("9"), Modifiers::ALT), Some(Command::LastTab));
        assert_eq!(window_command(Platform::Linux, &ch("3"), CTRL), None);
        assert_eq!(window_command(Platform::Windows, &ch("3"), CTRL), Some(Command::GoToTab(3)));
        assert_eq!(window_command(Platform::Mac, &ch("1"), Modifiers::META), Some(Command::GoToTab(1)));
        assert_eq!(window_command(Platform::Linux, &ch("0"), Modifiers::ALT), None);
    }

    #[test]
    fn history_keys_follow_the_platform() {
        let left = Key::Named(NamedKey::ArrowLeft);
        assert_eq!(window_command(Platform::Linux, &left, Modifiers::ALT), Some(Command::Back));
        assert_eq!(window_command(Platform::Linux, &Key::Named(NamedKey::ArrowRight), Modifiers::ALT), Some(Command::Forward));
        assert_eq!(window_command(Platform::Mac, &ch("["), Modifiers::META), Some(Command::Back));
        assert_eq!(window_command(Platform::Mac, &ch("]"), Modifiers::META), Some(Command::Forward));
        assert_eq!(window_command(Platform::Linux, &left, Modifiers::default()), None, "plain arrows are the page's");
    }

    #[test]
    fn the_mac_uses_command_and_not_control() {
        assert_eq!(window_command(Platform::Mac, &ch("t"), Modifiers::META), Some(Command::NewTab));
        assert_eq!(window_command(Platform::Mac, &ch("t"), CTRL), None);
        assert_eq!(window_command(Platform::Mac, &ch("]"), Modifiers::META | Modifiers::SHIFT), Some(Command::NextTab));
        assert_eq!(window_command(Platform::Mac, &ch(","), Modifiers::META), Some(Command::Settings));
    }

    #[test]
    fn function_keys_do_not_need_a_modifier() {
        let f = |named: NamedKey, modifiers| window_command(Platform::Linux, &Key::Named(named), modifiers);
        assert_eq!(f(NamedKey::F5, Modifiers::default()), Some(Command::Reload));
        assert_eq!(f(NamedKey::F5, Modifiers::SHIFT), Some(Command::ReloadBypassCache));
        assert_eq!(f(NamedKey::F5, CTRL), Some(Command::ReloadBypassCache));
        assert_eq!(f(NamedKey::F3, Modifiers::default()), Some(Command::FindNext));
        assert_eq!(f(NamedKey::F3, Modifiers::SHIFT), Some(Command::FindPrevious));
        assert_eq!(f(NamedKey::F11, Modifiers::default()), Some(Command::Fullscreen));
    }

    #[test]
    fn the_page_keeps_every_key_that_is_not_a_shortcut() {
        let page = |key: &str, modifiers| page_command(Platform::Linux, &ch(key), modifiers);
        assert_eq!(page("c", CTRL), Some(Command::Edit(EditCommand::Copy)));
        assert_eq!(page("v", CTRL), Some(Command::Edit(EditCommand::Paste)));
        assert_eq!(page("V", CTRL | Modifiers::SHIFT), Some(Command::Edit(EditCommand::PastePlain)));
        assert_eq!(page("a", CTRL), Some(Command::Edit(EditCommand::SelectAll)));
        assert_eq!(page("Z", CTRL | Modifiers::SHIFT), Some(Command::Edit(EditCommand::Redo)));
        assert_eq!(page("t", CTRL), Some(Command::NewTab), "a page cannot take Ctrl+T");
        assert_eq!(page("k", CTRL), None, "Ctrl+K is the page's");
        assert_eq!(page("a", Modifiers::default()), None);
        assert_eq!(
            page_command(Platform::Linux, &Key::Named(NamedKey::Enter), Modifiers::default()),
            None
        );
    }

    #[test]
    fn the_window_does_not_take_the_editing_keys() {
        // In the address bar Ctrl+C is the field's, not a command for the page.
        assert_eq!(window_command(Platform::Linux, &ch("c"), CTRL), None);
        assert_eq!(window_command(Platform::Linux, &ch("a"), CTRL), None);
    }

    #[test]
    fn menu_labels_follow_the_platform() {
        assert_eq!(label(Platform::Linux, Command::NewTab).as_deref(), Some("Ctrl+T"));
        assert_eq!(label(Platform::Linux, Command::Back).as_deref(), Some("Alt+Left"));
        assert_eq!(label(Platform::Mac, Command::NewTab).as_deref(), Some("\u{2318}T"));
        assert_eq!(label(Platform::Linux, Command::GoToTab(1)), None);
    }
}
