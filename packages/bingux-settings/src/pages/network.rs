use freya::prelude::*;

use crate::ui::{Status, page_title, status};

#[derive(PartialEq)]
pub struct NetworkPage;

impl Component for NetworkPage {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .child(page_title("Network"))
            .child(status(Status::Info("This page is being built.".into())))
    }
}
