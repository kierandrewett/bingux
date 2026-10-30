//! The Settings header's icon buttons (Back, the main menu) show no fill at
//! rest, at desktop and phone width. The same check for the Files toolbar is in
//! `bingux-ui` (`shell::rest_fill_tests`).

use std::time::Duration;

use bingux_ui::snapshot::Snapshot;

use crate::pages::PageId;

fn settled(size: (f32, f32)) -> Snapshot {
    let mut shot = Snapshot::new(crate::snapshot_app(PageId::Power.location()), size);
    shot.settle(Duration::from_millis(1200));
    shot
}

#[test]
fn no_icon_button_of_the_settings_header_has_a_fill_at_rest() {
    for (size, label) in [((1100.0, 760.0), "desktop"), ((390.0, 800.0), "phone")] {
        let mut shot = settled(size);
        let buttons = shot.icon_buttons();
        assert!(
            buttons.iter().any(|button| button.component == "IconMenuButton"),
            "{label}: the main menu button is in the header: {buttons:?}"
        );
        let filled: Vec<String> = buttons
            .iter()
            .filter(|button| button.fill.is_some())
            .map(|button| format!("{} ({})", button.name, button.component))
            .collect();
        assert!(filled.is_empty(), "{label}: icon buttons that fill at rest: {filled:?}");
    }
}
