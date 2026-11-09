//! The batch action bar: what a table offers for the rows that are selected.

use std::hash::Hash;
use std::rc::Rc;

use freya::prelude::*;

use super::ctx::Latest;
use crate::component::ComponentName;
use crate::controls::button::{action_button, icon_button};
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::EMPHASIS;

/// One action of the batch bar. Build it with [`batch_action`].
pub struct BatchAction<K> {
    label: String,
    icon: Option<Icon>,
    destructive: bool,
    on_press: Rc<dyn Fn(Vec<K>)>,
}

impl<K> Clone for BatchAction<K> {
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            icon: self.icon,
            destructive: self.destructive,
            on_press: self.on_press.clone(),
        }
    }
}

impl<K> PartialEq for BatchAction<K> {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
            && self.icon == other.icon
            && self.destructive == other.destructive
            && Rc::ptr_eq(&self.on_press, &other.on_press)
    }
}

/// An action on the selected rows. `label` is a verb that names the result
/// ("Uninstall", "Move to Trash"); `on_press` is given the keys of the selected
/// rows, in the order they are shown.
pub fn batch_action<K>(
    label: impl Into<String>,
    on_press: impl Fn(Vec<K>) + 'static,
) -> BatchAction<K> {
    BatchAction {
        label: label.into(),
        icon: None,
        destructive: false,
        on_press: Rc::new(on_press),
    }
}

impl<K> BatchAction<K> {
    /// An icon before the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Draws the action as one that loses something.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

/// The bar: how many rows are selected, the actions, and a way to clear.
pub(super) struct BatchBar<T, K> {
    pub count: usize,
    pub actions: Vec<BatchAction<K>>,
    pub latest: Latest<T, K>,
}

impl<T, K> PartialEq for BatchBar<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count && self.actions == other.actions
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Component for BatchBar<T, K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let latest = self.latest.clone();
        let mut buttons = Vec::new();
        for action in &self.actions {
            let latest = latest.clone();
            let run = action.on_press.clone();
            let mut button = action_button(action.label.clone(), move |()| {
                run(latest.get().selected_keys());
            })
            .flat()
            .compact();
            if let Some(icon) = action.icon {
                button = button.icon(icon);
            }
            if action.destructive {
                button = button.destructive();
            }
            buttons.push(Element::from(button));
        }
        let clear = icon_button(Icon::WindowClose, {
            let latest = latest.clone();
            move |()| latest.get().clear_selection()
        })
        .tooltip("Clear selection");
        rect()
            .component("BatchBar")
            .a11y_role(AccessibilityRole::Toolbar)
            .a11y_alt("Actions for the selected rows")
            .tw(&tokens, &[w_full(), px(LIST_EDGE), py(S1)])
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            flex_row(),
                            grow_children(),
                            items(Align::Center),
                            gap(S1),
                            w_full(),
                            h(Dim::Button),
                            pl(S3),
                            pr(S1),
                            rounded(Radius::Control),
                            bg(Paint::AccentSoft),
                        ],
                    )
                    .child(
                        rect().tw(&tokens, &[w_flex()]).child(
                            EMPHASIS
                                .label(&tokens, format!("{} selected", self.count))
                                .max_lines(1),
                        ),
                    )
                    .children(buttons)
                    .child(clear),
            )
    }
}
