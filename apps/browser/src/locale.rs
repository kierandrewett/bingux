//! The browser's catalogue, `locale/en-US/browser.ftl`.

use std::sync::Once;

/// Registers the catalogue, once: `main` before the first window, and each
/// test that reads a message.
pub fn register() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| bingux_ui::i18n::register(bingux_ui::catalogue!("browser")));
}
