//! The browser's catalogue, `locale/en-US/browser.ftl`.

use std::sync::Once;

/// Registers the catalogue, once: `main` before the first window, and each
/// test that reads a message.
pub fn register() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| bingux_ui::i18n::register(bingux_ui::catalogue!("browser")));
}

#[cfg(test)]
mod tests {
    /// Every `tr!` ID has a message with the variables it passes, and every
    /// message is used.
    #[test]
    fn the_catalogue_is_clean() {
        bingux_ui::i18n::check::crate_catalogue(env!("CARGO_MANIFEST_DIR"), "browser")
            .assert_clean();
    }
}
