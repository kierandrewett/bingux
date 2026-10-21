use freya::prelude::*;

use crate::ui::{Status, page_title, status};

#[derive(PartialEq)]
pub struct MousePage;

impl Component for MousePage {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .child(page_title("Mouse"))
            .child(status(Status::Info("This page is being built.".into())))
    }
}
