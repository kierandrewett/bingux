//! The zoom ladder, and zoom as the browser applies it to a tab.

use bingux_ui::web_view::TabEvent;
use bingux_ui::web_view::test_double::Call;

use super::*;
use crate::browser::testing::{browser, go};
use crate::model::shortcuts::Command;

#[test]
fn the_keys_walk_the_ladder_both_ways() {
    assert_eq!(Zoom(1.0).stepped_in(), Zoom(1.1));
    assert_eq!(Zoom(1.1).stepped_in(), Zoom(1.2));
    assert_eq!(Zoom(1.0).stepped_out(), Zoom(0.9));
    assert_eq!(Zoom(0.67).stepped_out(), Zoom(0.5));
}

#[test]
fn a_level_between_steps_goes_to_the_next_one() {
    assert_eq!(Zoom(1.05).stepped_in(), Zoom(1.1));
    assert_eq!(Zoom(1.05).stepped_out(), Zoom(1.0));
}

#[test]
fn the_ladder_ends_stay_put() {
    assert_eq!(Zoom(5.0).stepped_in(), Zoom(5.0));
    assert_eq!(Zoom(0.3).stepped_out(), Zoom(0.3));
}

#[test]
fn percent_rounds() {
    assert_eq!(Zoom(1.1).percent(), 110);
    assert_eq!(Zoom(0.6667).percent(), 67);
    assert!(Zoom(1.004).is_default());
    assert!(!Zoom(1.1).is_default());
}

#[test]
fn zoom_steps_along_the_ladder_and_resets() {
    let (mut browser, double) = browser();
    let key = go(&mut browser, "example.com");
    browser.run(Command::ZoomIn);
    browser.run(Command::ZoomIn);
    assert_eq!(browser.tabs.get(key).unwrap().zoom.level(), 1.2);
    let session = browser.session_of(key).unwrap();
    assert!(double.calls().contains(&Call::SetZoom(session, 1.2)));
    browser.run(Command::ZoomReset);
    assert_eq!(browser.tabs.get(key).unwrap().zoom.level(), 1.0);
    browser.apply(bingux_ui::web_view::BrowserEvent::tab(
        session,
        TabEvent::Zoom(1.5),
    ));
    assert_eq!(
        browser.tabs.get(key).unwrap().zoom.level(),
        1.5,
        "the page's own Ctrl+wheel is followed"
    );
}
