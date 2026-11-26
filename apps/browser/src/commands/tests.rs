//! Which key is which command, what a menu says beside a command, and what a
//! command the engine cannot do says.

use freya::prelude::{Key, Modifiers, NamedKey};

use super::*;
use crate::browser::testing::browser;

fn ch(text: &str) -> Key {
    Key::Character(text.to_string())
}

const CTRL: Modifiers = Modifiers::CONTROL;

#[test]
fn the_basic_ctrl_shortcuts() {
    let linux = |key: &str, modifiers| Platform::Linux.window_command(&ch(key), modifiers);
    assert_eq!(linux("t", CTRL), Some(Command::NewTab));
    assert_eq!(linux("w", CTRL), Some(Command::CloseTab));
    assert_eq!(linux("l", CTRL), Some(Command::FocusAddress));
    assert_eq!(linux("f", CTRL), Some(Command::Find));
    assert_eq!(linux("r", CTRL), Some(Command::Reload));
    assert_eq!(
        linux("T", CTRL | Modifiers::SHIFT),
        Some(Command::ReopenTab),
        "shift makes the letter a capital"
    );
    assert_eq!(
        linux("R", CTRL | Modifiers::SHIFT),
        Some(Command::ReloadBypassCache)
    );
    assert_eq!(linux("=", CTRL), Some(Command::ZoomIn));
    assert_eq!(
        linux("+", CTRL | Modifiers::SHIFT),
        Some(Command::ZoomIn),
        "plus is typed with Shift on most layouts"
    );
    assert_eq!(linux("-", CTRL), Some(Command::ZoomOut));
    assert_eq!(linux("0", CTRL), Some(Command::ZoomReset));
}

#[test]
fn extra_modifiers_make_it_another_key() {
    assert_eq!(
        Platform::Linux.window_command(&ch("t"), CTRL | Modifiers::ALT),
        None
    );
    assert_eq!(
        Platform::Linux.window_command(&ch("t"), Modifiers::default()),
        None,
        "a letter is a letter"
    );
    assert_eq!(
        Platform::Linux.window_command(&ch("t"), CTRL | Modifiers::CAPS_LOCK),
        Some(Command::NewTab),
        "locks do not count"
    );
    assert_eq!(
        Platform::Linux.window_command(&ch("t"), Modifiers::META),
        None,
        "Super is not Ctrl"
    );
}

#[test]
fn tabs_move_with_tab_and_the_page_keys() {
    let key =
        |named: NamedKey, modifiers| Platform::Linux.window_command(&Key::Named(named), modifiers);
    assert_eq!(key(NamedKey::Tab, CTRL), Some(Command::NextTab));
    assert_eq!(
        key(NamedKey::Tab, CTRL | Modifiers::SHIFT),
        Some(Command::PreviousTab)
    );
    assert_eq!(key(NamedKey::PageDown, CTRL), Some(Command::NextTab));
    assert_eq!(key(NamedKey::PageUp, CTRL), Some(Command::PreviousTab));
    assert_eq!(
        key(NamedKey::PageUp, CTRL | Modifiers::SHIFT),
        Some(Command::MoveTabLeft)
    );
    assert_eq!(
        key(NamedKey::PageDown, CTRL | Modifiers::SHIFT),
        Some(Command::MoveTabRight)
    );
}

#[test]
fn alt_digits_pick_a_tab_on_linux_and_ctrl_digits_elsewhere() {
    assert_eq!(
        Platform::Linux.window_command(&ch("3"), Modifiers::ALT),
        Some(Command::GoToTab(3))
    );
    assert_eq!(
        Platform::Linux.window_command(&ch("9"), Modifiers::ALT),
        Some(Command::LastTab)
    );
    assert_eq!(Platform::Linux.window_command(&ch("3"), CTRL), None);
    assert_eq!(
        Platform::Windows.window_command(&ch("3"), CTRL),
        Some(Command::GoToTab(3))
    );
    assert_eq!(
        Platform::Mac.window_command(&ch("1"), Modifiers::META),
        Some(Command::GoToTab(1))
    );
    assert_eq!(
        Platform::Linux.window_command(&ch("0"), Modifiers::ALT),
        None
    );
}

#[test]
fn history_keys_follow_the_platform() {
    let left = Key::Named(NamedKey::ArrowLeft);
    assert_eq!(
        Platform::Linux.window_command(&left, Modifiers::ALT),
        Some(Command::Back)
    );
    assert_eq!(
        Platform::Linux.window_command(&Key::Named(NamedKey::ArrowRight), Modifiers::ALT),
        Some(Command::Forward)
    );
    assert_eq!(
        Platform::Mac.window_command(&ch("["), Modifiers::META),
        Some(Command::Back)
    );
    assert_eq!(
        Platform::Mac.window_command(&ch("]"), Modifiers::META),
        Some(Command::Forward)
    );
    assert_eq!(
        Platform::Linux.window_command(&left, Modifiers::default()),
        None,
        "plain arrows are the page's"
    );
}

#[test]
fn the_mac_uses_command_and_not_control() {
    assert_eq!(
        Platform::Mac.window_command(&ch("t"), Modifiers::META),
        Some(Command::NewTab)
    );
    assert_eq!(Platform::Mac.window_command(&ch("t"), CTRL), None);
    assert_eq!(
        Platform::Mac.window_command(&ch("]"), Modifiers::META | Modifiers::SHIFT),
        Some(Command::NextTab)
    );
    assert_eq!(
        Platform::Mac.window_command(&ch(","), Modifiers::META),
        Some(Command::Settings)
    );
}

#[test]
fn function_keys_do_not_need_a_modifier() {
    let f =
        |named: NamedKey, modifiers| Platform::Linux.window_command(&Key::Named(named), modifiers);
    assert_eq!(f(NamedKey::F5, Modifiers::default()), Some(Command::Reload));
    assert_eq!(
        f(NamedKey::F5, Modifiers::SHIFT),
        Some(Command::ReloadBypassCache)
    );
    assert_eq!(f(NamedKey::F5, CTRL), Some(Command::ReloadBypassCache));
    assert_eq!(
        f(NamedKey::F3, Modifiers::default()),
        Some(Command::FindNext)
    );
    assert_eq!(
        f(NamedKey::F3, Modifiers::SHIFT),
        Some(Command::FindPrevious)
    );
    assert_eq!(
        f(NamedKey::F11, Modifiers::default()),
        Some(Command::Fullscreen)
    );
}

#[test]
fn the_page_keeps_every_key_that_is_not_a_shortcut() {
    let page = |key: &str, modifiers| Platform::Linux.page_command(&ch(key), modifiers);
    assert_eq!(page("c", CTRL), Some(Command::Edit(EditCommand::Copy)));
    assert_eq!(page("v", CTRL), Some(Command::Edit(EditCommand::Paste)));
    assert_eq!(
        page("V", CTRL | Modifiers::SHIFT),
        Some(Command::Edit(EditCommand::PastePlain))
    );
    assert_eq!(page("a", CTRL), Some(Command::Edit(EditCommand::SelectAll)));
    assert_eq!(
        page("Z", CTRL | Modifiers::SHIFT),
        Some(Command::Edit(EditCommand::Redo))
    );
    assert_eq!(
        page("t", CTRL),
        Some(Command::NewTab),
        "a page cannot take Ctrl+T"
    );
    assert_eq!(page("k", CTRL), None, "Ctrl+K is the page's");
    assert_eq!(page("a", Modifiers::default()), None);
    assert_eq!(
        Platform::Linux.page_command(&Key::Named(NamedKey::Enter), Modifiers::default()),
        None
    );
}

#[test]
fn the_window_does_not_take_the_editing_keys() {
    // In the address bar Ctrl+C is the field's, not a command for the page.
    assert_eq!(Platform::Linux.window_command(&ch("c"), CTRL), None);
    assert_eq!(Platform::Linux.window_command(&ch("a"), CTRL), None);
}

#[test]
fn print_and_unsupported_things_say_so() {
    let (mut browser, _) = browser();
    browser.run(Command::Print);
    let notices = browser.take_notices();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].error);
}
