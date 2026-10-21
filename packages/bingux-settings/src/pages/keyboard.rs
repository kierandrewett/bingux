use freya::prelude::*;

use crate::ui::{Status, page_title, status};

#[derive(PartialEq)]
pub struct KeyboardPage;

impl Component for KeyboardPage {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .child(page_title("Keyboard"))
            .child(status(Status::Info("This page is being built.".into())))
    }
}
