use freya::prelude::*;

use crate::ui::{Status, page_title, status};

#[derive(PartialEq)]
pub struct DesktopPage;

impl Component for DesktopPage {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .child(page_title("Desktop"))
            .child(status(Status::Info("This page is being built.".into())))
    }
}
