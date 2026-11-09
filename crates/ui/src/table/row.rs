//! One row of a table: a keyed component with its own hover and press, built
//! only while it is in view.

use std::hash::Hash;

use freya::prelude::*;

use super::ctx::{Columns, Latest};
use super::model::Plan;
use super::rows::Rows;
use crate::component::ComponentName;
use crate::controls::press::{point_of, use_press};
use crate::controls::text_field::text_field;
use crate::icons::{self, Icon};
use crate::shell::panel::PanelTrigger;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink, NUMERIC};

/// The width of the column of selection marks, at the start of a row.
pub(super) const CHECK_CELL: f32 = 36.0;
/// The space a row keeps clear of the table's side, so its rounded fill shows.
pub(super) fn side() -> f32 {
    LIST_EDGE.px() + FILE_ROW_INSET.px()
}

/// A row, with everything it draws. Equal rows are not drawn again.
pub(super) struct TableRow<T, K> {
    pub latest: Latest<T, K>,
    pub display: usize,
    /// The place in the rows of the row shown here: it changes with a sort.
    pub source: usize,
    pub rows: Rows<T>,
    pub columns: Columns<T>,
    pub plan: Plan,
    pub selected: bool,
    /// The plan column the keyboard cursor is on in this row, while the table
    /// has the keyboard.
    pub cursor: Option<usize>,
    /// The column index being edited in this row.
    pub editing: Option<usize>,
    pub height: f32,
    pub compact: bool,
    pub marks: bool,
    pub striped: bool,
    /// Selected rows are being picked: a phone row shows its mark.
    pub picking: bool,
    pub activates: bool,
}

impl<T, K> PartialEq for TableRow<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.latest == other.latest
            && self.display == other.display
            && self.source == other.source
            && self.rows == other.rows
            && self.columns == other.columns
            && self.plan == other.plan
            && self.selected == other.selected
            && self.cursor == other.cursor
            && self.editing == other.editing
            && self.height == other.height
            && self.compact == other.compact
            && self.marks == other.marks
            && self.striped == other.striped
            && self.picking == other.picking
            && self.activates == other.activates
    }
}

/// The mark that shows a row is selected: a rounded box, ticked when selected.
/// It is drawn and never focused; the row takes the press.
pub(crate) fn selection_mark(tokens: &crate::theme::Tokens, selected: bool) -> Element {
    rect()
        .tw(
            tokens,
            &[
                w(Dim::Box5),
                h(Dim::Box5),
                center(),
                rounded(Radius::Mark),
                if selected {
                    bg(Paint::Accent)
                } else {
                    bg(Paint::Transparent)
                },
                if selected {
                    border(Line::Stroke(Dim::Hairline, Paint::Accent))
                } else {
                    border(Line::Outline)
                },
            ],
        )
        .maybe_child(
            selected.then(|| icons::icon(Icon::Check, IconSize::Mark, Ink::OnAccent.color(tokens))),
        )
        .into()
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Component for TableRow<T, K> {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.display as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let press = use_press();
        let ctx = self.latest.get();
        let display = self.display;
        let Some(row) = self.rows.get(self.source) else {
            return rect().into_element();
        };
        let selected = self.selected;
        let touch_pick = self.compact && (self.picking || selected);
        let label = self
            .columns
            .0
            .iter()
            .find_map(|column| column.text_of(row))
            .unwrap_or_default();

        let mut line = rect().tw(
            &tokens,
            &[
                flex_row(),
                items(Align::Center),
                grow_children(),
                w_full(),
                h_full(),
                px(FILE_ROW_INSET),
                gap(S0),
                rounded(Radius::Control),
                surface(
                    if self.striped && display % 2 == 1 {
                        Paint::Hover
                    } else {
                        Paint::Transparent
                    },
                    hovering(),
                    pressing(),
                    selected,
                ),
            ],
        );

        if self.compact {
            line = line.child(self.compact_cells(row, &tokens, touch_pick, selected));
        } else {
            if self.marks {
                line = line.child(
                    rect()
                        .width(Size::px(CHECK_CELL))
                        .tw(&tokens, &[flex_row(), center(), h_full()])
                        .child(selection_mark(&tokens, selected)),
                );
            }
            for (at, placed) in self.plan.columns.iter().enumerate() {
                let column = &self.columns.0[placed.index];
                let content: Element = if self.editing == Some(placed.index) {
                    let key = (ctx.key)(row);
                    CellEditor {
                        initial: column.text_of(row).unwrap_or_default(),
                        latest: self.latest.clone(),
                        key,
                        column: placed.index,
                    }
                    .into()
                } else if let Some(cell) = &column.cell {
                    cell(row)
                } else {
                    let text = column.text_of(row).unwrap_or_default();
                    let style = if column.spec.right { NUMERIC } else { BODY };
                    let ink = if column.muted { Ink::Muted } else { Ink::Text };
                    let label = style
                        .in_ink(ink)
                        .label(&tokens, text)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis);
                    with_icon(&tokens, column, row, label)
                };
                let ring = self.cursor == Some(at);
                line = line.child(
                    rect()
                        .a11y_id(ctx.cells.get(display, at))
                        .a11y_role(AccessibilityRole::Cell)
                        .a11y_builder(move |node| node.set_column_index(at))
                        .width(Size::px(placed.width))
                        .tw(
                            &tokens,
                            &[
                                flex_col(),
                                justify(Align::Center),
                                items(if column.spec.right {
                                    Align::End
                                } else {
                                    Align::Start
                                }),
                                px(S2),
                                h_full(),
                                rounded(Radius::Inner),
                            ],
                        )
                        .maybe(ring, |cell| {
                            cell.tw(
                                &tokens,
                                &[border(Line::Stroke(Dim::Ring, Paint::FocusRing))],
                            )
                        })
                        .child(content),
                );
            }
        }

        let menu = ctx.menu.is_some();
        rect()
            .component("TableRow")
            .height(Size::px(self.height))
            .tw(&tokens, &[flex_col(), w_full(), px(LIST_EDGE)])
            .a11y_role(AccessibilityRole::Row)
            .a11y_alt(label)
            .a11y_builder(move |node| {
                node.set_row_index(display + 1);
                node.set_selected(selected);
            })
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down({
                let latest = self.latest.clone();
                move |event: Event<PointerEventData>| {
                    let ctx = latest.get();
                    match event.data() {
                        PointerEventData::Touch(_) => {
                            pressing.set(true);
                            press.begin(
                                &event,
                                Some(EventHandler::new({
                                    let latest = latest.clone();
                                    move |at: (f32, f32)| latest.get().long_press(display, at)
                                })),
                            );
                        }
                        PointerEventData::Mouse(mouse) => match mouse.button {
                            Some(MouseButton::Left) => {
                                pressing.set(true);
                                let col = column_at(&ctx, mouse.element_location.x as f32);
                                ctx.press_cell(display, col, freya_winit::modifiers::live());
                            }
                            Some(MouseButton::Right) if menu => {
                                event.stop_propagation();
                                ctx.open_menu(
                                    display,
                                    PanelTrigger::Pointer(point_of(&event)),
                                    false,
                                );
                            }
                            _ => {}
                        },
                    }
                }
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                press.moved(&event);
                // A touch that became a scroll is no longer a press.
                if !press.pending() && crate::controls::press::is_touch(&event) {
                    pressing.set_if_modified(false);
                }
            })
            .on_global_pointer_up(move |_| {
                press.release();
                pressing.set_if_modified(false);
            })
            .on_press({
                let latest = self.latest.clone();
                move |event: Event<PressEventData>| {
                    event.stop_propagation();
                    let ctx = latest.get();
                    match event.data() {
                        PressEventData::Touch(_) => {
                            if press.tap(&event) {
                                ctx.tap(display);
                            }
                        }
                        PressEventData::Mouse(_) => {
                            ctx.press_done(display, freya_winit::modifiers::live());
                        }
                        _ => {}
                    }
                }
            })
            .child(line)
            .into_element()
    }
}

/// The plan column under window-relative `x` within a row, so a press puts the
/// cursor there.
fn column_at<T: 'static, K: Clone + Eq + Hash + 'static>(
    ctx: &super::ctx::Ctx<T, K>,
    x: f32,
) -> usize {
    let mut left = side() - FILE_ROW_INSET.px();
    if ctx.has_marks() {
        left += CHECK_CELL;
    }
    let mut at = 0;
    for (index, placed) in ctx.plan.columns.iter().enumerate() {
        if x < left + placed.width {
            return index;
        }
        left += placed.width;
        at = index;
    }
    at
}

/// The cell's `label`, after the column's symbol when it has one.
fn with_icon<T: 'static>(
    tokens: &crate::theme::Tokens,
    column: &super::column::Column<T>,
    row: &T,
    label: freya::prelude::Label,
) -> Element {
    match column.icon.as_ref() {
        Some(icon) => rect()
            .tw(
                tokens,
                &[flex_row(), items(Align::Center), gap(S2), w_full()],
            )
            .child(icons::icon(
                icon(row),
                IconSize::Row,
                Ink::Muted.color(tokens),
            ))
            .child(label.width(Size::flex(1.0)))
            .into(),
        None => label.into(),
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> TableRow<T, K> {
    /// A phone row: the first column's cell as the title, the other columns'
    /// text in one muted line under it, a mark while rows are being picked and
    /// a chevron when the row opens something.
    fn compact_cells(
        &self,
        row: &T,
        tokens: &crate::theme::Tokens,
        pick: bool,
        selected: bool,
    ) -> Element {
        let mut cells = Vec::new();
        let mut title = None;
        let mut caption: Vec<String> = Vec::new();
        for (at, placed) in self.plan.columns.iter().enumerate() {
            let column = &self.columns.0[placed.index];
            if at == 0 {
                title = Some(match &column.cell {
                    Some(cell) => cell(row),
                    None => with_icon(
                        tokens,
                        column,
                        row,
                        BODY.label(tokens, column.text_of(row).unwrap_or_default())
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
                });
            } else if column.phone
                && let Some(text) = column.text_of(row).filter(|text| !text.is_empty())
            {
                caption.push(text);
            }
        }
        if pick {
            cells.push(
                rect()
                    .tw(tokens, &[flex_row(), center(), mr(S3)])
                    .child(selection_mark(tokens, selected))
                    .into_element(),
            );
        }
        let text = rect()
            .tw(
                tokens,
                &[flex_col(), w_flex(), justify(Align::Center), gap(S0_5)],
            )
            .maybe_child(title)
            .maybe_child((!caption.is_empty()).then(|| {
                CAPTION
                    .label(tokens, caption.join(" \u{b7} "))
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
            }));
        cells.push(text.into_element());
        if self.activates && !pick {
            cells.push(
                icons::icon(Icon::PanEnd, IconSize::Mark, Ink::Muted.color(tokens)).into_element(),
            );
        }
        rect()
            .tw(
                tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    grow_children(),
                    w_full(),
                    h_full(),
                    px(S2),
                ],
            )
            .children(cells)
            .into_element()
    }
}

/// A cell being edited: a field that starts with the cell's text selected.
/// Enter saves, Escape drops the edit.
struct CellEditor<T, K> {
    initial: String,
    latest: Latest<T, K>,
    key: K,
    column: usize,
}

impl<T, K: PartialEq> PartialEq for CellEditor<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.initial == other.initial && self.key == other.key && self.column == other.column
    }
}

impl<T: 'static, K: Clone + Eq + Hash + 'static> Component for CellEditor<T, K> {
    fn render(&self) -> impl IntoElement {
        let draft = use_state(|| self.initial.clone());
        let latest = self.latest.clone();
        let key = self.key.clone();
        let column = self.column;
        let escape = self.latest.clone();
        text_field(draft, move |text: String| {
            latest.get().commit_edit(key.clone(), column, text);
        })
        .fill_width()
        .flat()
        .dense()
        .select_all_on_open()
        .autofocus()
        .on_escape(move |()| escape.get().stop_edit())
    }
}
