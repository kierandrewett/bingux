use freya::prelude::*;

use crate::ui::{Status, page_title, status};

#[derive(PartialEq)]
pub struct UsersPage;

impl Component for UsersPage {
    fn render(&self) -> impl IntoElement {
        rect()
            .width(Size::fill())
            .child(page_title("Users"))
            .child(status(Status::Info("This page is being built.".into())))
    }
}
