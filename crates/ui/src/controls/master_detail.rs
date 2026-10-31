//! A master list and its detail page: the vertical tab stack.
//!
//! On a wide window the list stands at the left, a hairline divides it from
//! the page, and both show at once. On a phone one shows at a time: the list,
//! and the page pushed over it with a Back button when `show_detail` is set.
//! The caller owns which item is open; selecting an item on a phone sets it,
//! and `on_back` clears it. Use it inside a popup or a sheet; the navigation
//! split of a whole window is `NavigationSplitView`.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::button::icon_button;
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;

/// The width of the master list on a wide window.
pub const MASTER_WIDTH: f32 = 380.0;

#[derive(PartialEq)]
pub struct MasterDetail {
    master: Element,
    detail: Element,
    show_detail: bool,
    on_back: EventHandler<()>,
    back_label: String,
}

/// A list and its page. `show_detail` says which one a phone shows.
pub fn master_detail(
    master: impl Into<Element>,
    detail: impl Into<Element>,
    show_detail: bool,
    on_back: impl Into<EventHandler<()>>,
) -> MasterDetail {
    MasterDetail {
        master: master.into(),
        detail: detail.into(),
        show_detail,
        on_back: on_back.into(),
        back_label: "Back".to_string(),
    }
}

impl MasterDetail {
    /// What the Back button on a phone is called.
    pub fn back_label(mut self, label: impl Into<String>) -> Self {
        self.back_label = label.into();
        self
    }
}

impl Component for MasterDetail {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        if is_compact() {
            let on_back = self.on_back.clone();
            if !self.show_detail {
                return rect()
                    .component("MasterDetail")
                    .tw(&tokens, &[w_full(), h_full(), flex_col()])
                    .child(self.master.clone());
            }
            return rect()
                .component("MasterDetail")
                .tw(&tokens, &[w_full(), h_full(), flex_col()])
                .child(
                    rect()
                        .tw(
                            &tokens,
                            &[w_full(), flex_row(), items(Align::Center), px(S2), py(S2)],
                        )
                        .child(
                            icon_button(Icon::GoPrevious, move |_| on_back.call(()))
                                .tooltip(self.back_label.as_str()),
                        ),
                )
                .child(self.detail.clone());
        }
        rect()
            .component("MasterDetail")
            .tw(&tokens, &[w_full(), h_full(), flex_row()])
            .child(
                rect()
                    .width(Size::px(MASTER_WIDTH))
                    .height(Size::fill())
                    .child(self.master.clone()),
            )
            .child(rect().tw(&tokens, &[w(Dim::Hairline), h_full(), bg(Paint::Separator)]))
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .child(self.detail.clone()),
            )
    }
}

#[cfg(all(test, feature = "testing"))]
#[path = "master_detail_tests.rs"]
mod tests;
