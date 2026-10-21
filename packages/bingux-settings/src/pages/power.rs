use freya::prelude::*;

use crate::ui::{Status, page_title, status};

#[derive(PartialEq)]
pub struct PowerPage;

impl Component for PowerPage {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .child(page_title("Power"))
            .child(status(Status::Info("This page is being built.".into())))
    }
}
