//! Recipes: the named boxes every page is built from, with the layout
//! rhythm in one table.
//!
//! The rhythm, outside in:
//!
//! | Between                                  | Space |
//! | ---------------------------------------- | ----- |
//! | Page title block and the first group     | 28    |
//! | One group and the next                   | 40    |
//! | A group's title block and its card       | 12    |
//! | A group title and its description        | 4     |
//! | Two cards in one group                   | 12    |
//! | Card edge and row content (left, right)  | 16    |
//! | Card edge and free content (all sides)   | 16    |
//! | Row top and bottom when it has two lines | 12    |
//! | Leading icon and row text                | 12    |
//! | Row title and subtitle                   | 2     |
//! | Row text and its trailing control        | 16    |
//!
//! A recipe used by one component only may live at the top of that
//! component's file, as a `const` of type [`Recipe`]; one used by two or
//! more lives here.

use super::*;

/// A named list of utilities.
pub type Recipe = &'static [Util];

/// The page column: groups 40 px apart.
pub const PAGE: Recipe = &[flex_col(), w_full(), gap(S10)];
/// The page title block over the page column: 28 px to the first group.
pub const PAGE_BODY: Recipe = &[flex_col(), w_full(), gap(S7)];
/// The page title and the sentence under it.
pub const PAGE_HEAD: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    gap(S4),
];
pub const PAGE_HEAD_TEXT: Recipe = &[flex_col(), w_flex(), gap(S1)];

/// A group: its title block, then its cards 12 px under it.
pub const GROUP: Recipe = &[flex_col(), w_full(), gap(S3)];
/// A group's title block: title and description at the left, an optional
/// action at the right.
pub const GROUP_HEAD: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::End),
    w_full(),
    gap(S4),
];
pub const GROUP_HEAD_TEXT: Recipe = &[flex_col(), w_flex(), gap(S1)];
/// A boxed surface: a group, a card, the action strip, the arrangement canvas.
/// Every box uses this one recipe, so they share fill, hairline and radius.
/// Rows sit flush in it.
pub const BOX: Recipe = &[
    flex_col(),
    w_full(),
    rounded(Radius::Card),
    bg(Paint::Card),
    border(Line::Hairline),
    clip(),
];
/// Free content inside a box (swatches, previews, a chart): 16 px from the
/// box edge on every side.
pub const BOX_BODY: Recipe = &[flex_col(), w_full(), p(S4), gap(S3)];

/// A one-line row: 56 px, content 16 px from the card edge.
pub const ROW: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    h(Dim::Row),
    px(GUTTER),
    gap(S3),
];
/// A row whose text has two or more lines: at least 56 px, 12 px above and
/// below the text.
pub const ROW_TALL: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    h_auto(),
    min_h(Dim::Row),
    px(GUTTER),
    py(S3),
    gap(S3),
];
/// A one-line row split into a body and a chevron zone: the zone reaches the
/// card edge and the full height, so the row has no padding of its own.
pub const ROW_SPLIT: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    h(Dim::Row),
];
/// [`ROW_SPLIT`] for a row whose text has two or more lines.
pub const ROW_TALL_SPLIT: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    h_auto(),
    min_h(Dim::Row),
];
/// The body of a split row: what [`ROW`] holds, less the chevron, ending one
/// gap before the divider.
pub const ROW_SPLIT_BODY: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    pl(GUTTER),
    pr(S3),
    gap(S3),
];
/// [`ROW_SPLIT_BODY`] of a tall row, 12 px above and below the text.
pub const ROW_TALL_SPLIT_BODY: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    pl(GUTTER),
    pr(S3),
    py(S3),
    gap(S3),
];
/// The chevron zone of a split row: one gap either side of the chevron, and
/// at least a control wide so a thumb can land on it.
pub const ROW_CHEVRON_ZONE: Recipe = &[flex_row(), center(), px(S3), min_w(Dim::Control)];
/// A row whose content stacks: a title line over a full-width control such
/// as a slider.
pub const ROW_STACKED: Recipe = &[flex_col(), w_full(), px(GUTTER), py(S3), gap(S2)];
/// The title and subtitle column of a row.
pub const ROW_TEXT: Recipe = &[flex_col(), w_flex(), gap(S0_5)];
/// The trailing controls of a row, 16 px after the text.
pub const ROW_TRAILING: Recipe = &[flex_row(), items(Align::Center), ml(S1), gap(S2)];
/// The hairline between two rows, inset by the gutter on both sides.
pub const SEPARATOR: Recipe = &[w_full(), h(Dim::Hairline), mx(GUTTER), bg(Paint::Separator)];

/// A control in a row: dropdown, field, stepper.
pub const CONTROL: Recipe = &[
    flex_row(),
    items(Align::Center),
    h(Dim::Control),
    px(S3),
    gap(S2),
    rounded(Radius::Control),
];
/// A standalone button.
pub const BUTTON: Recipe = &[
    flex_row(),
    center(),
    h(Dim::Button),
    px(S4),
    gap(S2),
    rounded(Radius::Control),
];
/// A button that sits in a row, at the control height.
pub const BUTTON_COMPACT: Recipe = &[
    flex_row(),
    center(),
    h(Dim::Control),
    px(S3),
    gap(S2),
    rounded(Radius::Control),
];
/// An icon-only button.
pub const ICON_BUTTON: Recipe = &[center(), w(Dim::Control), h(Dim::Control), rounded(Radius::Control)];
/// A small rounded count or tag after a title.
pub const BADGE_BOX: Recipe = &[
    flex_row(),
    center(),
    h(Dim::Box5),
    px(S2),
    rounded(Radius::Pill),
    bg(Paint::AccentSoft),
];

/// A menu, dropdown list or popover: items 8 px from its edge.
pub const MENU: Recipe = &[
    flex_col(),
    p(S2),
    rounded(Radius::Overlay),
    bg(Paint::Popover),
    border(Line::Hairline),
    shadow(Elevation::Raised),
    clip(),
];
/// One item in a menu or dropdown list.
pub const MENU_ITEM: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    h(Dim::MenuItem),
    px(S3),
    gap(S3),
    rounded(Radius::Control),
];
/// A dialog or popup card.
pub const DIALOG: Recipe = &[
    flex_col(),
    p(S6),
    gap(S4),
    rounded(Radius::Overlay),
    bg(Paint::Bg),
    border(Line::Hairline),
    shadow(Elevation::Floating),
];
/// A tooltip bubble.
pub const TOOLTIP: Recipe = &[
    px(S2),
    py(S1),
    rounded(Radius::Control),
    bg(Paint::Floating),
    border(Line::Hairline),
];

/// The header bar: back button, title, actions and window buttons.
pub const HEADER: Recipe = &[
    flex_row(),
    grow_children(),
    items(Align::Center),
    w_full(),
    h(Dim::Header),
    px(S2),
    gap(S1),
];
/// The sidebar column.
pub const SIDEBAR: Recipe = &[flex_col(), w(Dim::Sidebar), h_full()];
/// The list of pages in the sidebar.
pub const SIDEBAR_LIST: Recipe = &[flex_col(), w_full(), px(LIST_EDGE), pb(S4), gap(S0_5)];
/// The search band at the top of the sidebar, on the header line.
pub const SIDEBAR_SEARCH: Recipe = &[
    flex_row(),
    items(Align::Center),
    w_full(),
    h(Dim::Header),
    px(S2),
];
/// The shape of a selectable item in a list, written once for the sidebar
/// row, the places row and the file row: a flex row, its icon and text
/// `NAV_INSET` in from the ends of the rounded fill, `S2` between them. The
/// two recipes differ only in the height rule.
macro_rules! list_item {
    ($height:expr) => {
        &[
            flex_row(),
            items(Align::Center),
            w_full(),
            $height,
            px(NAV_INSET),
            gap(S2),
            rounded(Radius::Control),
        ]
    };
}
/// A row in the sidebar: a page, the profile or a search result. One height.
pub const NAV_ITEM: Recipe = list_item!(h(Dim::NavItem));
/// A row that is at least as tall as a sidebar row and grows with its
/// content: a place with a second line, a file row with a big icon. Set a
/// height with `Size::px` when the content decides it. Fill it with
/// `surface(Paint::Transparent, hover, press, selected)` as the sidebar does,
/// and keep the list's sides `LIST_EDGE` clear of its container.
pub const LIST_ITEM: Recipe = list_item!(min_h(Dim::NavItem));
/// The icon column of a sidebar row: a box of the column width with the icon centred.
pub const NAV_ICON: Recipe = &[
    flex_row(),
    items(Align::Center),
    center(),
    w(Dim::NavIcon),
    h(Dim::NavIcon),
];
/// The hairline between the sidebar and the page, full height.
pub const SPLIT_HAIRLINE: Recipe = &[w(Dim::Hairline), h_full(), bg(Paint::Separator)];
/// The name of a group of pages in the sidebar.
pub const NAV_LABEL: Recipe = &[w_full(), px(S2), pt(S4), pb(S1)];
