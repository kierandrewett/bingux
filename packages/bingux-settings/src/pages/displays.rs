use freya::prelude::*;

use crate::ui::{Status, page_title, status};

#[derive(PartialEq)]
pub struct DisplaysPage;

impl Component for DisplaysPage {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .child(page_title("Displays"))
            .child(status(Status::Info("This page is being built.".into())))
    }
}
