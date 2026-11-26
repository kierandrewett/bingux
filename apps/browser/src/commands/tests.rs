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
    assert_eq!(
        f(NamedKey::F12, Modifiers::default()),
        Some(Command::DevTools)
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

// Failures the fullscreen keys guard against:
//   1. Escape swallowed by the browser while a page is fullscreen, so the page
//      stays fullscreen in the engine and the window leaves it alone;
//   2. Escape forwarded to the page during F11 fullscreen, so F11 cannot be
//      left with Escape;
//   3. a page entering fullscreen in a background tab taking the window.
#[test]
fn escape_is_the_browsers_for_f11_and_the_pages_for_page_fullscreen() {
    use bingux_ui::web_view::{BrowserEvent, TabEvent, TabId};
    let (mut browser, double) = browser();
    assert!(!browser.escape_leaves_window_fullscreen());
    browser.run(Command::Fullscreen);
    assert!(
        browser.escape_leaves_window_fullscreen(),
        "F11 is left with Escape"
    );
    browser.run(Command::Fullscreen);
    let tab = browser
        .tabs
        .active()
        .expect("tab")
        .session
        .expect("session");
    double.emit(BrowserEvent::tab(TabId(tab.0), TabEvent::Fullscreen(true)));
    browser.poll();
    assert!(browser.fullscreen, "the page's fullscreen fills the window");
    assert!(
        !browser.escape_leaves_window_fullscreen(),
        "Escape goes to the page, which ends it"
    );
    double.emit(BrowserEvent::tab(TabId(tab.0), TabEvent::Fullscreen(false)));
    browser.poll();
    assert!(
        !browser.fullscreen,
        "the engine ended it and the window follows"
    );
}

// Failures Print guards against:
//   1. a blank tab or Settings page "printed", writing an empty PDF;
//   2. the engine called with no tab, or on a hidden one;
//   3. a print the engine cannot do (no capability) saying nothing.
#[test]
fn print_on_a_page_says_where_the_pdf_went() {
    let (mut browser, _double) = browser();
    crate::browser::testing::go(&mut browser, "https://example.com/");
    browser.take_notices();
    browser.run(Command::Print);
    browser.poll();
    let notices = browser.out.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| !notice.error && notice.text.contains("Saved the page as a PDF")),
        "{notices:?}"
    );
}

#[test]
fn print_asks_the_engine_for_a_pdf_named_after_the_page() {
    use bingux_ui::web_view::test_double::Call;
    let (mut browser, double) = browser();
    browser.run(Command::Print);
    assert!(
        !double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::PrintPdf(..))),
        "a blank tab has nothing to print"
    );
    crate::browser::testing::go(&mut browser, "https://example.com/");
    double.clear_calls();
    browser.run(Command::Print);
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::PrintPdf(_, name) if name.ends_with(".pdf"))),
        "{:?}",
        double.calls()
    );
    browser.poll();
    let notices = browser.out.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| !notice.error && notice.text.contains("Saved the page as a PDF")),
        "{notices:?}"
    );
}

// Failures the developer tools and picture-in-picture commands guard against:
//   1. F12 doing nothing at all, so the person thinks the browser is broken;
//   2. a toast that claims the tools opened when they did not;
//   3. the toolbox opened in the background, or not as a tab of its own, or
//      for a private page (the engine refuses it);
//   4. picture-in-picture pretending to work when the engine has no window.
#[test]
fn picture_in_picture_says_the_engine_cannot() {
    let (mut browser, double) = browser();
    browser.run(Command::PictureInPicture);
    let notices = browser.out.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.error
                && notice.text.contains("Picture-in-picture is not available")),
        "{notices:?}"
    );
    assert!(double.calls().is_empty(), "nothing was asked of the engine");
}

#[test]
fn f12_opens_the_toolbox_of_a_page_in_a_tab_in_front_of_it() {
    use bingux_ui::web_view::test_double::Call;
    let (mut browser, double) = browser();
    browser.run(Command::DevTools);
    let notices = browser.out.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.error && notice.text.contains("no web page")),
        "a blank tab has nothing to inspect: {notices:?}"
    );
    let page = crate::browser::testing::go(&mut browser, "https://example.com/");
    let tabs_before = browser.tabs.all().len();
    double.clear_calls();
    browser.run(Command::DevTools);
    assert!(
        double
            .calls()
            .iter()
            .any(|call| matches!(call, Call::OpenDevtools(_)))
    );
    assert_eq!(browser.tabs.all().len(), tabs_before + 1);
    let toolbox = browser.tabs.active().expect("active");
    assert_ne!(toolbox.key, page, "the toolbox is in front");
    assert_eq!(toolbox.opener, Some(page));
}
