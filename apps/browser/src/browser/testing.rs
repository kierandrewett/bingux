//! The fixtures the tests of the browser start from.

use std::sync::Arc;

use bingux_ui::web_view::test_double::TestDoubleBackend;

use super::{Browser, Options};
use crate::model::tabs::Place;

/// A browser over the test double, with one blank tab open and the engine ready.
pub(crate) fn browser() -> (Browser, Arc<TestDoubleBackend>) {
    let double = TestDoubleBackend::new();
    let mut browser = Browser::new(double.clone(), Options::default());
    browser.open_tab("", Place::End, true);
    browser.poll();
    double.clear_calls();
    (browser, double)
}

/// Navigate the tab on show to `url` and take in what the engine says.
pub(crate) fn go(browser: &mut Browser, url: &str) -> u64 {
    let key = browser.tabs.active_key();
    browser.navigate(key, url);
    browser.poll();
    key
}
