use freya::prelude::*;

use crate::ui::{Status, page_title, status};

#[derive(PartialEq)]
pub struct BluetoothPage;

impl Component for BluetoothPage {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .child(page_title("Bluetooth"))
            .child(status(Status::Info("This page is being built.".into())))
    }
}
