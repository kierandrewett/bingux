//! The editing surface. See the [module](super) for what it is; this file
//! is how it is drawn and how it hears the keys and the pointer.
//!
//! The view is a grid of monospace cells. Each frame it walks from the row
//! at the top of the viewport down to the bottom, laying out only those
//! lines, and draws, back to front: the current-line band, find hits,
//! selections, matching brackets, the text of each row, then the carets.
//! Nothing below or above the viewport is laid out or drawn.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

use crate::component::ComponentName;

use bingux_text::layout::{LineLayout, Row, Segment, row_segments};
use bingux_text::{Editor, Matches, Range, TokenKind};

use super::keymap::{self, Effect, ZoomStep};
use super::palette::{SyntaxPalette, code_row, code_span};
use super::{EditorHandle, HighlightCache, ScrollPos};
use crate::dnd::{
    DRAG_SLOP, DragAction, DragActions, DragEnd, DragImage, DragInfo, DragRequest, DropTarget,
    Dropped, Format, Payload, use_drag, use_drop_target,
};
use crate::drag::{Point, is_touch, point_of};
use crate::shell::{MenuItem, Overlays, menu_item, menu_separator};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, CODE, Ink};

/// Line height as a multiple of the font size.
const LEADING: f32 = 1.5;
/// Cells to keep between the caret and the side when scrolling sideways.
const SIDE_MARGIN: f32 = 4.0;
/// Width of a caret in px.
const CARET: f32 = 2.0;
/// Width of the scroll bar's thumb.
const THUMB: f32 = 6.0;
/// The shortest thumb.
const THUMB_MIN: f32 = 28.0;
/// Lines the highlighter may parse in one frame.
const HIGHLIGHT_BUDGET: usize = 1500;
/// Documents larger than this are not highlighted.
pub const HIGHLIGHT_LIMIT: usize = 8 << 20;
/// Two presses this close (in time and in px) are one double or triple click.
const MULTI_CLICK: Duration = Duration::from_millis(400);
const MULTI_CLICK_SLOP: f64 = 5.0;
/// How long a caret stays solid after it moves, and its blink half period.
const BLINK_WAIT: Duration = Duration::from_millis(600);
const BLINK_HALF: Duration = Duration::from_millis(530);
/// Touchpad travel (px) that zooms one step with Ctrl held.
const ZOOM_TRAVEL: f64 = 60.0;

/// The editing surface. Build it with [`text_editor`].
#[derive(Clone, PartialEq)]
pub struct TextEditorView {
    handle: EditorHandle,
    line_numbers: bool,
    wrap: bool,
    show_whitespace: bool,
    highlight_line: bool,
    read_only: bool,
    highlight: bool,
    zoom: f32,
    on_zoom: Option<EventHandler<ZoomStep>>,
    matches: Option<(Rc<Matches>, Option<usize>)>,
    placeholder: Option<String>,
    label: String,
    a11y_id: Option<AccessibilityId>,
    menu_extra: Vec<MenuItem>,
    on_focus_change: Option<EventHandler<bool>>,
}

/// An editing surface for `handle`.
pub fn text_editor(handle: EditorHandle) -> TextEditorView {
    TextEditorView {
        handle,
        line_numbers: true,
        wrap: false,
        show_whitespace: false,
        highlight_line: true,
        read_only: false,
        highlight: true,
        zoom: 1.0,
        on_zoom: None,
        matches: None,
        placeholder: None,
        label: "Text".into(),
        a11y_id: None,
        menu_extra: Vec::new(),
        on_focus_change: None,
    }
}

impl TextEditorView {
    /// A gutter of line numbers on the leading side.
    pub fn line_numbers(mut self, on: bool) -> Self {
        self.line_numbers = on;
        self
    }

    /// Long lines wrap at the view's width instead of scrolling sideways.
    pub fn wrap(mut self, on: bool) -> Self {
        self.wrap = on;
        self
    }

    /// Spaces show as `·`, tabs as `→`.
    pub fn show_whitespace(mut self, on: bool) -> Self {
        self.show_whitespace = on;
        self
    }

    /// A faint band behind the caret's line.
    pub fn highlight_line(mut self, on: bool) -> Self {
        self.highlight_line = on;
        self
    }

    /// The text can be selected and copied but not changed.
    pub fn read_only(mut self, on: bool) -> Self {
        self.read_only = on;
        self
    }

    /// Syntax highlighting in the document's language. A document over
    /// [`HIGHLIGHT_LIMIT`] is never highlighted.
    pub fn syntax_highlighting(mut self, on: bool) -> Self {
        self.highlight = on;
        self
    }

    /// Text size as a factor of the body size. Ctrl+= and Ctrl+-, and Ctrl
    /// with the wheel, ask `on_zoom` to change it.
    pub fn zoom(mut self, zoom: f32, on_zoom: impl Into<EventHandler<ZoomStep>>) -> Self {
        self.zoom = zoom.clamp(0.5, 4.0);
        self.on_zoom = Some(on_zoom.into());
        self
    }

    /// Find hits to mark, and the index of the current one.
    pub fn matches(mut self, matches: Rc<Matches>, current: Option<usize>) -> Self {
        self.matches = Some((matches, current));
        self
    }

    /// What an empty document says, faint, where the text would be.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// The name assistive technology reads for the editor.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// The editor's accessibility id, so the window can focus it.
    pub fn a11y_id(mut self, id: AccessibilityId) -> Self {
        self.a11y_id = Some(id);
        self
    }

    /// Items added after Cut, Copy, Paste and Select All in the context menu.
    pub fn menu_extra(mut self, items: Vec<MenuItem>) -> Self {
        self.menu_extra = items;
        self
    }

    /// Told when the editor gains or loses the keyboard focus.
    pub fn on_focus_change(mut self, handler: impl Into<EventHandler<bool>>) -> Self {
        self.on_focus_change = Some(handler.into());
        self
    }
}

/// What a press in the text started: a drag that selects by characters,
/// words or lines from `anchor`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Text {
        unit: Unit,
        anchor: (usize, usize),
    },
    Gutter {
        anchor_line: usize,
    },
    Thumb {
        grab: f32,
    },
    /// A press inside the selection: a click that places the caret, unless
    /// the pointer travels far enough to drag the selected text.
    TextPending,
    /// The selected text is being dragged inside this window (no Wayland
    /// drag and drop to hand it to).
    TextInWindow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    Char,
    Word,
    Line,
}

/// Pointer state of one view.
#[derive(Default)]
struct PointerState {
    drag: Option<Drag>,
    last_press: Option<(Instant, (f64, f64), usize)>,
    /// The press that may become a drag of the selected text.
    grab: Option<TextGrab>,
}

/// A press inside the selection.
#[derive(Clone, Copy, Debug)]
struct TextGrab {
    /// Where, in window pixels.
    from: Point,
    /// The selected range it is in (chars).
    range: (usize, usize),
    /// The character the press was on, where a click puts the caret.
    pos: usize,
}

/// What a drag of selected text carries inside this program: where the text
/// came from, so a drop on the same document moves it as one undo step and
/// the source is not removed twice.
#[derive(Clone)]
struct TextDrag {
    handle: EditorHandle,
    range: (usize, usize),
    /// Set by the drop that moved the text itself.
    moved: Rc<std::cell::Cell<bool>>,
}

/// The sizes of the grid this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Metrics {
    font: f32,
    row: f32,
    cell: f32,
    gutter: f32,
    pad: f32,
    width: f32,
    height: f32,
    text_width: f32,
    wrap: Option<usize>,
}

impl Metrics {
    /// Whole rows that fit.
    fn fit(&self) -> usize {
        ((self.height / self.row).floor() as usize).max(1)
    }

    fn text_left(&self) -> f32 {
        self.gutter + self.pad
    }
}

/// One row on screen.
#[derive(Clone, Debug)]
struct ShownRow {
    line: usize,
    row: usize,
    y: f32,
}

impl Component for TextEditorView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let own_id = use_a11y();
        let a11y = self.a11y_id.unwrap_or(own_id);
        let focus = use_focus(a11y);
        let focused = focus().is_focused();
        let mut size = use_state(|| (0.0f32, 0.0f32));
        let mut measured_cell = use_state(|| 0.0f32);
        let mut origin = use_state(|| (0.0f32, 0.0f32));
        let pointer = use_hook(|| Rc::new(RefCell::new(PointerState::default())));
        let mut preedit = use_state(String::new);
        let mut moved_at = use_state(Instant::now);
        let highlight_pending = use_hook(|| Rc::new(std::cell::Cell::new(false)));
        let overlays = try_consume_context::<Overlays>();
        // The editor stops the keys and presses it uses, so the command scope
        // around it is told it holds the focus.
        let scope = crate::commands::use_scope_notifier();
        let key_scope = scope.clone();
        let mut was_focused = use_state(|| false);
        if *was_focused.peek() != focused {
            was_focused.set(focused);
            if let Some(handler) = &self.on_focus_change {
                handler.call(focused);
            }
            if !focused {
                self.handle.peek_mut_seal();
            }
        }

        let handle = self.handle.clone();
        let editor = handle.read();

        // The grid.
        let font = (TextSize::Md.px() * self.zoom * 4.0).round() / 4.0;
        let row_height = (font * LEADING).round().max(1.0);
        let cell = if *measured_cell.read() > 0.0 {
            *measured_cell.read() * self.zoom
        } else {
            font * 0.6
        };
        let (width, height) = *size.read();
        let lines = editor.len_lines();
        let digits = lines.to_string().len().max(2) as f32;
        let gutter = if self.line_numbers {
            digits * cell + S5.px() + S2.px()
        } else {
            0.0
        };
        let pad = S2.px();
        let text_width = (width - gutter - pad - THUMB * 2.0).max(cell * 8.0);
        let wrap = self
            .wrap
            .then(|| ((text_width / cell).floor() as usize).max(8));
        let metrics = Metrics {
            font,
            row: row_height,
            cell,
            gutter,
            pad,
            width,
            height,
            text_width,
            wrap,
        };

        // Text dragged in (from another program, or this document's own
        // selection) lands where the pointer lets go. A move inside the
        // document is one undo step.
        let dragger = use_drag();
        let drop_handle = handle.clone();
        let drop_read_only = self.read_only;
        let text_drop = use_drop_target(
            DropTarget::new()
                .accepts([Format::Text])
                .actions(DragActions::COPY_MOVE)
                .highlight(false)
                .enabled(!drop_read_only)
                // Files carry their paths as text too: they are the window's
                // to open, not text to type in.
                .accept_if(|info: DragInfo| !info.formats.contains(&Format::Uris))
                // Text dragged within this program moves, as in every text
                // field (Ctrl copies); text from other programs is copied
                // here, and its source decides whether it is then removed.
                .preferred(|info: DragInfo| {
                    if info.local
                        && info.source_actions.contains(DragAction::Move)
                        && !held().contains(Modifiers::CONTROL)
                    {
                        DragAction::Move
                    } else {
                        DragAction::Copy
                    }
                })
                .on_drop(move |dropped: Dropped| {
                    let Some(text) = dropped.data.text() else {
                        return;
                    };
                    let found = {
                        let editor = drop_handle.peek();
                        let scroll = drop_handle.memory().scroll;
                        position_at(
                            &editor,
                            &scroll,
                            &metrics,
                            dropped.position.0,
                            dropped.position.1,
                        )
                    };
                    let Some((pos, _)) = found else {
                        return;
                    };
                    let ours = dropped
                        .data
                        .local::<TextDrag>()
                        .filter(|drag| drag.handle.same(&drop_handle));
                    match ours {
                        Some(drag) if dropped.action == DragAction::Move => {
                            drop_handle.update(|editor| editor.move_text(drag.range, pos));
                            // The text is where it is going: the drag's end
                            // must not remove it from the source.
                            drag.moved.set(true);
                        }
                        _ => drop_handle.update(|editor| editor.drop_text(pos, &text)),
                    }
                    a11y.request_focus();
                }),
        );

        // Scroll to follow the caret when the text or the selection changed.
        let mut memory = handle.memory();
        let current = (editor.revision(), editor.selection().clone());
        if memory.followed.as_ref() != Some(&current) && height > 0.0 {
            memory.scroll = follow(&editor, memory.scroll, &metrics);
            memory.followed = Some(current);
        }
        memory.scroll = clamp_scroll(&editor, memory.scroll, &metrics);
        let scroll = memory.scroll;

        // The rows on screen.
        let mut shown: Vec<ShownRow> = Vec::new();
        let mut layouts: Vec<(usize, LineLayout)> = Vec::new();
        {
            let mut y = -scroll.offset;
            let mut line = scroll.line;
            let mut row = scroll.row;
            while line < lines && (y < height || shown.is_empty()) {
                let layout = editor.layout_wrapped(line, wrap);
                let count = layout.rows.len();
                while row < count && (y < height || shown.is_empty()) {
                    shown.push(ShownRow { line, row, y });
                    y += row_height;
                    row += 1;
                }
                layouts.push((line, layout));
                line += 1;
                row = 0;
            }
        }
        let first_line = shown.first().map_or(0, |row| row.line);
        let last_line = shown.last().map_or(0, |row| row.line);
        memory.shown = (first_line, last_line);

        // Highlighting, lazily, from the nearest checkpoint.
        let highlighting = self.highlight && editor.rope().len_bytes() <= HIGHLIGHT_LIMIT;
        let spans: Vec<Option<Rc<[bingux_text::highlight::Span]>>> = if highlighting {
            let language = editor.language().id.clone();
            let cache = memory.highlight.get_or_insert_with(|| HighlightCache {
                highlighter: bingux_text::Highlighter::new(editor.language()),
                language: language.clone(),
                revision: editor.revision(),
            });
            if cache.language != language {
                *cache = HighlightCache {
                    highlighter: bingux_text::Highlighter::new(editor.language()),
                    language,
                    revision: editor.revision(),
                };
            }
            if let Some(changed) = editor.changed_since(cache.revision) {
                cache.highlighter.invalidate_from(changed);
            }
            cache.revision = editor.revision();
            let result =
                cache
                    .highlighter
                    .spans(editor.rope(), first_line, last_line + 1, HIGHLIGHT_BUDGET);
            if !result.done && !highlight_pending.get() {
                // Parse on over the next frames until the screen is reached.
                highlight_pending.set(true);
                let pending = highlight_pending.clone();
                let handle = handle.clone();
                spawn(async move {
                    timer(Duration::from_millis(16)).await;
                    pending.set(false);
                    handle.refresh();
                });
            }
            result.lines
        } else {
            vec![None; last_line + 1 - first_line]
        };
        drop(memory);

        let palette = SyntaxPalette::new(&tokens);
        let text_left = metrics.text_left() - if wrap.is_none() { scroll.x } else { 0.0 };
        let layout_of = |line: usize| {
            layouts
                .iter()
                .find(|(at, _)| *at == line)
                .map(|(_, layout)| layout)
        };
        let selection = editor.selection().clone();
        let primary = selection.primary();
        let primary_line = editor.line_of(primary.head);
        let single_caret = !selection.is_multi() && primary.is_empty();

        // Back to front.
        let mut layers: Vec<Element> = Vec::new();

        // The current line.
        if self.highlight_line && single_caret {
            for shown_row in shown.iter().filter(|row| row.line == primary_line) {
                layers.push(band(
                    &tokens,
                    0.0,
                    shown_row.y,
                    width,
                    row_height,
                    Paint::Hover,
                ));
            }
        }

        // Find hits.
        if let Some((matches, current)) = &self.matches {
            let start = editor.line_start(first_line);
            let end = editor.line_end(last_line) + 1;
            let first_index = matches
                .ranges
                .partition_point(|(_, match_end)| *match_end < start);
            for (offset, (match_start, match_end)) in matches.within(start, end).iter().enumerate()
            {
                let paint = if *current == Some(first_index + offset) {
                    Paint::SearchCurrent
                } else {
                    Paint::SearchHit
                };
                for rect in range_rects(
                    &editor,
                    &shown,
                    &layout_of,
                    *match_start,
                    *match_end,
                    &metrics,
                    text_left,
                    false,
                ) {
                    layers.push(band(&tokens, rect.0, rect.1, rect.2, row_height, paint));
                }
            }
        }

        // Selections.
        let visible_start = editor.line_start(first_line);
        let visible_end = editor.line_end(last_line);
        for range in selection.ranges().iter().filter(|range| !range.is_empty()) {
            if range.end() < visible_start || range.start() > visible_end {
                continue;
            }
            let paint = if focused {
                Paint::Selection
            } else {
                Paint::Hover
            };
            for rect in range_rects(
                &editor,
                &shown,
                &layout_of,
                range.start(),
                range.end(),
                &metrics,
                text_left,
                true,
            ) {
                layers.push(band(&tokens, rect.0, rect.1, rect.2, row_height, paint));
            }
        }

        // Where dragged text would land.
        if let Some(over) = text_drop.over()
            && let Some((pos, false)) =
                position_at(&editor, &scroll, &metrics, over.position.0, over.position.1)
            && let Some((x, y)) = caret_point(&editor, &shown, &layout_of, pos, &metrics, text_left)
        {
            layers.push(
                rect()
                    .position(Position::new_absolute().left(x).top(y))
                    .width(Size::px(CARET))
                    .height(Size::px(row_height))
                    .tw(&tokens, &[bg(Paint::Accent)])
                    .into(),
            );
        }

        // The bracket at the caret and its match.
        if single_caret && let Some((here, there)) = editor.matching_bracket(primary.head) {
            for at in [here, there] {
                if let Some((x, y)) =
                    caret_point(&editor, &shown, &layout_of, at, &metrics, text_left)
                {
                    layers.push(
                        rect()
                            .position(Position::new_absolute().left(x).top(y))
                            .width(Size::px(cell))
                            .height(Size::px(row_height))
                            .tw(&tokens, &[bg(Paint::AccentSoft), rounded(Radius::Mark)])
                            .into(),
                    );
                }
            }
        }

        // The text.
        let empty = editor.len_chars() == 0;
        for shown_row in shown.iter() {
            let Some(layout) = layout_of(shown_row.line) else {
                continue;
            };
            let line_spans = spans.get(shown_row.line - first_line).cloned().flatten();
            let segments = row_text(
                &editor,
                shown_row.line,
                layout,
                shown_row.row,
                line_spans.as_deref(),
                self.show_whitespace,
                wrap.is_none(),
                scroll.x,
                &metrics,
            );
            let row_left = if layout.is_long() && wrap.is_none() {
                text_left + segments.1 as f32 * cell
            } else {
                text_left
                    + layout.rows[shown_row.row]
                        .start_col
                        .saturating_sub(layout.rows[shown_row.row].start_col)
                        as f32
                        * cell
            };
            let paragraph =
                segments
                    .0
                    .into_iter()
                    .fold(code_row(font, row_height), |paragraph, segment| {
                        let ink = match (segment.faint, segment.kind) {
                            (true, _) => super::palette::Ink {
                                colour: palette.faint,
                                bold: false,
                                italic: false,
                            },
                            (false, Some(kind)) => palette.ink(kind),
                            (false, None) => palette.ink(TokenKind::Operator),
                        };
                        paragraph.span(code_span(segment.text, ink, font))
                    });
            layers.push(
                rect()
                    .position(Position::new_absolute().left(row_left).top(shown_row.y))
                    .height(Size::px(row_height))
                    .child(paragraph)
                    .into(),
            );
        }
        if empty && let Some(placeholder) = &self.placeholder {
            layers.push(
                rect()
                    .position(Position::new_absolute().left(metrics.text_left()).top(0.0))
                    .height(Size::px(row_height))
                    .tw(&tokens, &[flex_row(), items(Align::Center)])
                    .child(CODE.in_ink(Ink::Muted).label_scaled(
                        &tokens,
                        placeholder.clone(),
                        font / CODE.size.px(),
                    ))
                    .into(),
            );
        }

        // The IME's text being composed, over the caret.
        let composing = preedit.read().clone();
        let caret_points: Vec<(f32, f32)> = selection
            .ranges()
            .iter()
            .filter_map(|range| {
                caret_point(&editor, &shown, &layout_of, range.head, &metrics, text_left)
            })
            .collect();
        if !composing.is_empty()
            && let Some((x, y)) = caret_points.first().copied()
        {
            layers.push(
                rect()
                    .position(Position::new_absolute().left(x).top(y))
                    .height(Size::px(row_height))
                    .tw(&tokens, &[bg(Paint::Bg), flex_row(), items(Align::Center)])
                    .child(
                        code_row(font, row_height).span(
                            code_span(composing.clone(), palette.ink(TokenKind::Link), font)
                                .text_decoration(TextDecoration::Underline),
                        ),
                    )
                    .into(),
            );
        }

        // The carets, in a component of their own so only they redraw as
        // they blink.
        let caret_rows: Vec<(f32, f32)> = caret_points.clone();
        layers.push(
            Carets {
                points: caret_rows,
                height: row_height,
                visible: focused && !self.read_only || focused && selection.is_multi(),
                moved: *moved_at.read(),
            }
            .into_element(),
        );

        // The focus target: a caret-sized node where the IME places its
        // window, holding the editor's role and name.
        let (target_x, target_y) = caret_points
            .first()
            .copied()
            .unwrap_or((metrics.text_left(), 0.0));
        let line_text: String = editor.line(primary_line).chars().take(2000).collect();
        let label = self.label.clone();
        // Freya sends a key to the focused node and the nodes above it that
        // listen, so the key handler is on this node (see `on_key_down` below).
        let focus_target = rect()
            .position(Position::new_absolute().left(target_x).top(target_y))
            .width(Size::px(CARET))
            .height(Size::px(row_height))
            .a11y_id(a11y)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::MultilineTextInput)
            .a11y_alt(label)
            .a11y_builder(move |node| node.set_value(line_text.clone()))
            .on_ime_preedit(move |event: Event<ImePreeditEventData>| {
                preedit.set(event.data().text.clone());
            });

        // The gutter.
        if self.line_numbers {
            let mut numbers: Vec<Element> = Vec::new();
            // Numbers a little smaller than the text, right-aligned before the gap.
            let number_width = gutter - S2.px();
            let number_scale = font * 0.92 / CODE.size.px();
            for shown_row in shown.iter().filter(|row| row.row == 0) {
                let ink = if shown_row.line == primary_line {
                    Ink::Text
                } else {
                    Ink::Muted
                };
                numbers.push(
                    rect()
                        .key(shown_row.line)
                        .position(Position::new_absolute().left(0.0).top(shown_row.y))
                        .width(Size::px(number_width))
                        .height(Size::px(row_height))
                        .tw(
                            &tokens,
                            &[flex_row(), items(Align::Center), justify(Align::End)],
                        )
                        .child(
                            CODE.in_ink(ink)
                                .label_scaled(
                                    &tokens,
                                    (shown_row.line + 1).to_string(),
                                    number_scale,
                                )
                                .font_tabular(),
                        )
                        .into(),
                );
            }
            layers.push(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::px(gutter))
                    .height(Size::px(height))
                    .tw(&tokens, &[bg(Paint::Bg)])
                    .children(numbers)
                    .into(),
            );
        }

        // The scroll bar.
        let thumb = thumb_geometry(&editor, &scroll, &metrics, shown.len());
        if let Some((top, length)) = thumb {
            layers.push(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(width - THUMB - S1.px())
                            .top(top),
                    )
                    .width(Size::px(THUMB))
                    .height(Size::px(length))
                    .tw(&tokens, &[bg(Paint::MutedFaint), rounded(Radius::Pill)])
                    .into(),
            );
        }

        // The cell width, measured once in the face and size drawn.
        let measure = rect()
            .position(Position::new_absolute().left(-10_000.0).top(0.0))
            .on_sized(move |event: Event<SizedEventData>| {
                let width = event.area.width() / 64.0;
                if width > 0.0 && (width - *measured_cell.peek()).abs() > 0.01 {
                    measured_cell.set(width);
                }
            })
            .child(
                code_row(TextSize::Md.px(), TextSize::Md.px() * LEADING)
                    .span(Span::new("0".repeat(64))),
            );

        drop(editor);

        // Input.
        let read_only = self.read_only;
        let on_zoom = self.on_zoom.clone();
        let key_handle = handle.clone();
        let on_key_down = move |event: Event<KeyboardEventData>| {
            if !a11y.is_focused() {
                return;
            }
            let Some(command) = keymap::command_for(&event.key, event.modifiers) else {
                return;
            };
            if !preedit.peek().is_empty() {
                return;
            }
            key_scope.focused();
            let page = metrics.fit().saturating_sub(1);
            let effect = key_handle.update(|editor| {
                editor.set_wrap(metrics.wrap);
                keymap::run(editor, &command, page, read_only)
            });
            match effect {
                Effect::Unhandled => return,
                Effect::Copy(text) => {
                    let _ = Clipboard::set(text);
                }
                Effect::Paste => {
                    if let Ok(text) = Clipboard::get() {
                        key_handle.update(|editor| editor.paste(&text));
                    }
                }
                Effect::Zoom(step) => {
                    if let Some(on_zoom) = &on_zoom {
                        on_zoom.call(step);
                    }
                }
                Effect::None => {}
            }
            moved_at.set(Instant::now());
            event.stop_propagation();
            event.prevent_default();
        };
        // First among the children, so the layers before it (bands that come
        // and go with brackets, hits and selections) never shift it: a node
        // that moves among its siblings is made again and loses the focus.
        let focus_node = focus_target.on_key_down(on_key_down);

        let wheel_handle = handle.clone();
        let wheel_zoom = self.on_zoom.clone();
        let zoom_travel = use_hook(|| Rc::new(std::cell::Cell::new(0.0f64)));
        let on_wheel = move |event: Event<WheelEventData>| {
            if held().contains(Modifiers::CONTROL) {
                // A notch is a step; a touchpad's pixels add up to one.
                let travel = zoom_travel.get() + event.delta_y;
                let step = if event.source == WheelSource::Line || travel.abs() >= ZOOM_TRAVEL {
                    travel
                } else {
                    0.0
                };
                zoom_travel.set(if step == 0.0 { travel } else { 0.0 });
                if step != 0.0
                    && let Some(on_zoom) = &wheel_zoom
                {
                    on_zoom.call(if step > 0.0 {
                        ZoomStep::In
                    } else {
                        ZoomStep::Out
                    });
                }
                event.stop_propagation();
                return;
            }
            let shift = held().contains(Modifiers::SHIFT);
            let (dx, dy) = if shift {
                (-event.delta_y as f32, 0.0)
            } else {
                (-event.delta_x as f32, -event.delta_y as f32)
            };
            {
                let editor = wheel_handle.peek();
                let mut memory = wheel_handle.memory();
                let mut scroll = scroll_by(&editor, memory.scroll, dy, &metrics);
                if metrics.wrap.is_none() {
                    scroll.x = (scroll.x + dx).max(0.0);
                }
                memory.scroll = clamp_scroll(&editor, scroll, &metrics);
            }
            wheel_handle.refresh();
            event.stop_propagation();
        };

        let press_handle = handle.clone();
        let press_pointer = pointer.clone();
        let press_overlays = overlays;
        let menu_extra = self.menu_extra.clone();
        let on_pointer_down = move |event: Event<PointerEventData>| {
            let at = event.element_location();
            let global = event.global_location();
            let button = event.button();
            a11y.request_focus();
            scope.focused();
            let editor_pos = {
                let editor = press_handle.peek();
                let scroll = press_handle.memory().scroll;
                position_at(&editor, &scroll, &metrics, at.x as f32, at.y as f32)
            };
            let Some((pos, in_gutter)) = editor_pos else {
                return;
            };
            moved_at.set(Instant::now());
            if button == Some(MouseButton::Right) {
                let outside = !press_handle
                    .peek()
                    .selection()
                    .ranges()
                    .iter()
                    .any(|range| range.start() <= pos && pos <= range.end() && !range.is_empty());
                if outside {
                    press_handle.update(|editor| editor.set_cursor(pos, false));
                }
                if let Some(overlays) = press_overlays {
                    let items = context_menu(press_handle.clone(), read_only, menu_extra.clone());
                    overlays.show_menu((global.x as f32, global.y as f32), items);
                }
                event.stop_propagation();
                return;
            }
            if button == Some(MouseButton::Middle) {
                return;
            }
            // The scroll bar's track.
            if at.x as f32 >= metrics.width - THUMB * 3.0 {
                let scroll = press_handle.memory().scroll;
                let editor = press_handle.peek();
                if let Some((top, length)) =
                    thumb_geometry(&editor, &scroll, &metrics, metrics.fit())
                {
                    let y = at.y as f32;
                    let grab = if y >= top && y <= top + length {
                        y - top
                    } else {
                        length / 2.0
                    };
                    drop(editor);
                    press_pointer.borrow_mut().drag = Some(Drag::Thumb { grab });
                    drag_thumb(&press_handle, &metrics, y - grab);
                    event.stop_propagation();
                    return;
                }
            }
            let modifiers = held();
            let shift = modifiers.contains(Modifiers::SHIFT);
            let alt = modifiers.contains(Modifiers::ALT);
            if in_gutter {
                let line = press_handle.peek().line_of(pos);
                press_handle.update(|editor| editor.select_line_at(editor.line_start(line)));
                press_pointer.borrow_mut().drag = Some(Drag::Gutter { anchor_line: line });
                event.stop_propagation();
                return;
            }
            let mut state = press_pointer.borrow_mut();
            let now = Instant::now();
            let count = match state.last_press {
                Some((when, (x, y), count))
                    if now.duration_since(when) < MULTI_CLICK
                        && (x - global.x).abs() < MULTI_CLICK_SLOP
                        && (y - global.y).abs() < MULTI_CLICK_SLOP =>
                {
                    count % 3 + 1
                }
                _ => 1,
            };
            state.last_press = Some((now, (global.x, global.y), count));
            // A press on selected text may be the start of dragging it away.
            if count == 1
                && !alt
                && !shift
                && button == Some(MouseButton::Left)
                && !is_touch(&event)
            {
                let inside = press_handle
                    .peek()
                    .selection()
                    .ranges()
                    .iter()
                    .find(|range| !range.is_empty() && range.start() <= pos && pos < range.end())
                    .map(|range| (range.start(), range.end()));
                if let Some(range) = inside {
                    state.grab = Some(TextGrab {
                        from: point_of(&event),
                        range,
                        pos,
                    });
                    state.drag = Some(Drag::TextPending);
                    event.stop_propagation();
                    return;
                }
            }
            let unit = match count {
                2 => Unit::Word,
                3 => Unit::Line,
                _ => Unit::Char,
            };
            let anchor = press_handle.update(|editor| {
                if alt && count == 1 {
                    editor.toggle_cursor_at(pos);
                    return (pos, pos);
                }
                let span = unit_span(editor, pos, unit);
                if shift && count == 1 {
                    editor.set_cursor(pos, true);
                    let anchor = editor.selection().primary().anchor;
                    return (anchor, anchor);
                }
                editor.set_selection(bingux_text::Selection::single(Range::new(span.0, span.1)));
                span
            });
            if !(alt && count == 1) {
                state.drag = Some(Drag::Text { unit, anchor });
            }
            event.stop_propagation();
        };

        let move_handle = handle.clone();
        let move_pointer = pointer.clone();
        let move_dragger = dragger.clone();
        let on_global_pointer_move = move |event: Event<PointerEventData>| {
            let Some(drag) = move_pointer.borrow().drag else {
                return;
            };
            match drag {
                Drag::TextPending => {
                    start_text_drag(&move_handle, &move_pointer, &move_dragger, point_of(&event));
                    return;
                }
                Drag::TextInWindow => {
                    move_dragger.moved(point_of(&event));
                    return;
                }
                _ => {}
            }
            let global = event.global_location();
            let (ox, oy) = *origin.peek();
            let x = global.x as f32 - ox;
            let y = global.y as f32 - oy;
            if let Drag::Thumb { grab } = drag {
                drag_thumb(&move_handle, &metrics, y - grab);
                return;
            }
            // Past the top or bottom, the view scrolls toward the pointer.
            let overshoot = if y < 0.0 {
                y
            } else if y > metrics.height {
                y - metrics.height
            } else {
                0.0
            };
            if overshoot != 0.0 {
                let editor = move_handle.peek();
                let mut memory = move_handle.memory();
                let scroll = scroll_by(&editor, memory.scroll, overshoot / 2.0, &metrics);
                memory.scroll = clamp_scroll(&editor, scroll, &metrics);
                // Keep following the pointer, not the caret.
                memory.followed = None;
            }
            let found = {
                let editor = move_handle.peek();
                let scroll = move_handle.memory().scroll;
                position_at(
                    &editor,
                    &scroll,
                    &metrics,
                    x.max(metrics.gutter + 1.0),
                    y.clamp(0.0, metrics.height - 1.0),
                )
            };
            let Some((pos, _)) = found else { return };
            move_handle.update(|editor| match drag {
                Drag::Text { unit, anchor } => {
                    let span = unit_span(editor, pos, unit);
                    let range = if pos < anchor.0 {
                        Range::new(anchor.1, span.0)
                    } else {
                        Range::new(anchor.0, span.1.max(anchor.1))
                    };
                    let mut ranges: Vec<Range> = editor.selection().ranges().to_vec();
                    let primary = editor.selection().primary_index();
                    ranges[primary] = range;
                    editor.set_selection(bingux_text::Selection::new(ranges, primary));
                }
                Drag::Gutter { anchor_line } => {
                    let line = editor.line_of(pos);
                    let (first, last) = (anchor_line.min(line), anchor_line.max(line));
                    let end = if last + 1 < editor.len_lines() {
                        editor.line_start(last + 1)
                    } else {
                        editor.len_chars()
                    };
                    let range = if line < anchor_line {
                        Range::new(end, editor.line_start(first))
                    } else {
                        Range::new(editor.line_start(first), end)
                    };
                    editor.set_selection(bingux_text::Selection::single(range));
                }
                Drag::Thumb { .. } | Drag::TextPending | Drag::TextInWindow => {}
            });
            moved_at.set(Instant::now());
        };
        let up_pointer = pointer.clone();
        let up_handle = handle.clone();
        let up_dragger = dragger;
        let on_global_pointer_up = move |event: Event<PointerEventData>| {
            let (drag, grab) = {
                let mut state = up_pointer.borrow_mut();
                (state.drag.take(), state.grab.take())
            };
            match (drag, grab) {
                // A click inside the selection: the caret goes there.
                (Some(Drag::TextPending), Some(grab)) => {
                    up_handle.update(|editor| editor.set_cursor(grab.pos, false));
                }
                (Some(Drag::TextInWindow), _) => up_dragger.released(point_of(&event)),
                _ => {}
            }
        };

        rect()
            .component("TextEditor")
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg), clip()])
            .cursor(CursorIcon::Text)
            .on_sized(move |event: Event<SizedEventData>| {
                let next = (event.area.width(), event.area.height());
                if *size.peek() != next {
                    size.set(next);
                }
                text_drop.set_area(event.visible_area);
                let corner = (event.area.min_x(), event.area.min_y());
                if *origin.peek() != corner {
                    origin.set(corner);
                }
            })
            .on_wheel(on_wheel)
            .on_pointer_down(on_pointer_down)
            .on_global_pointer_move(on_global_pointer_move)
            .on_global_pointer_up(on_global_pointer_up)
            .child(focus_node)
            .child(measure)
            .children(layers)
    }
}

impl EditorHandle {
    /// The next edit starts a new undo step: the editor lost focus.
    pub(crate) fn peek_mut_seal(&self) {
        self.editor.borrow_mut().seal();
    }
}

/// A filled band in window-local px.
fn band(
    tokens: &crate::theme::Tokens,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    paint: Paint,
) -> Element {
    rect()
        .position(Position::new_absolute().left(x).top(y))
        .width(Size::px(width.max(0.0)))
        .height(Size::px(height))
        .tw(tokens, &[bg(paint)])
        .into()
}

/// The text of one row as styled segments, and for a long line without
/// wrap the first char drawn (only the part on screen is drawn).
#[allow(clippy::too_many_arguments)]
fn row_text(
    editor: &Editor,
    line: usize,
    layout: &LineLayout,
    row: usize,
    spans: Option<&[bingux_text::highlight::Span]>,
    show_whitespace: bool,
    no_wrap: bool,
    scroll_x: f32,
    metrics: &Metrics,
) -> (Vec<Segment<TokenKind>>, usize) {
    if layout.is_long() {
        let Row { start, end, .. } = layout.rows[row];
        let (from, to) = if no_wrap {
            let first = (scroll_x / metrics.cell).floor() as usize;
            let count = (metrics.width / metrics.cell).ceil() as usize + 2;
            (first.min(end), (first + count).min(end))
        } else {
            (start, end)
        };
        let line_start = editor.line_start(line);
        let text: String = editor
            .rope()
            .slice(line_start + from..line_start + to)
            .chars()
            .map(|ch| if ch.is_control() { '\u{fffd}' } else { ch })
            .collect();
        let offset = if no_wrap { from } else { 0 };
        return (
            vec![Segment {
                text,
                kind: None,
                faint: false,
            }],
            offset,
        );
    }
    let text = editor.line(line);
    (
        row_segments(
            &text,
            layout,
            row,
            spans.unwrap_or(&[]),
            editor.settings.tab_width,
            show_whitespace,
        ),
        0,
    )
}

/// Where the caret before char `pos` is drawn, if its row is on screen.
fn caret_point<'a>(
    editor: &Editor,
    shown: &[ShownRow],
    layout_of: &impl Fn(usize) -> Option<&'a LineLayout>,
    pos: usize,
    metrics: &Metrics,
    text_left: f32,
) -> Option<(f32, f32)> {
    let line = editor.line_of(pos);
    let layout = layout_of(line)?;
    let (row, col) = layout.col_in_row(pos - editor.line_start(line));
    let shown_row = shown
        .iter()
        .find(|shown| shown.line == line && shown.row == row)?;
    Some((text_left + col as f32 * metrics.cell, shown_row.y))
}

/// The bands that cover chars `start..end`: `(x, y, width)` for each row on
/// screen. `newline` adds a cell where a selection runs over a line break.
#[allow(clippy::too_many_arguments)]
fn range_rects<'a>(
    editor: &Editor,
    shown: &[ShownRow],
    layout_of: &impl Fn(usize) -> Option<&'a LineLayout>,
    start: usize,
    end: usize,
    metrics: &Metrics,
    text_left: f32,
    newline: bool,
) -> Vec<(f32, f32, f32)> {
    let mut out = Vec::new();
    for shown_row in shown {
        let Some(layout) = layout_of(shown_row.line) else {
            continue;
        };
        let line_start = editor.line_start(shown_row.line);
        let row = layout.rows[shown_row.row];
        let row_start = line_start + row.start;
        let row_end = line_start + row.end;
        let last_row = shown_row.row + 1 == layout.rows.len();
        if end <= row_start && !(end == row_start && start < end && row.start == row.end) {
            continue;
        }
        if start > row_end || (start == row_end && !last_row) {
            continue;
        }
        let from = start.max(row_start) - line_start;
        let to = end.min(row_end) - line_start;
        let col_from = layout.col(from) - row.start_col;
        let col_to = layout.col(to) - row.start_col;
        let mut cells = col_to.saturating_sub(col_from) as f32;
        if newline && last_row && end > row_end && shown_row.line + 1 < editor.len_lines() {
            cells += 1.0;
        }
        if cells > 0.0 {
            out.push((
                text_left + col_from as f32 * metrics.cell,
                shown_row.y,
                cells * metrics.cell,
            ));
        }
    }
    out
}

/// The char under a point of the view (view-local px), and whether the
/// point is in the gutter.
fn position_at(
    editor: &Editor,
    scroll: &ScrollPos,
    metrics: &Metrics,
    x: f32,
    y: f32,
) -> Option<(usize, bool)> {
    let rows_down = ((y + scroll.offset) / metrics.row).floor().max(0.0) as usize;
    let (mut line, mut row) = (scroll.line, scroll.row);
    let mut left = rows_down;
    let lines = editor.len_lines();
    loop {
        let layout = editor.layout_wrapped(line, metrics.wrap);
        let available = layout.rows.len() - row;
        if left < available {
            row += left;
            let scroll_x = if metrics.wrap.is_none() {
                scroll.x
            } else {
                0.0
            };
            let col = ((x - metrics.text_left() + scroll_x) / metrics.cell)
                .round()
                .max(0.0) as usize;
            let index = layout.index_at(row, col);
            return Some((editor.line_start(line) + index, x < metrics.gutter));
        }
        left -= available;
        if line + 1 >= lines {
            // Below the last row: the end of the text.
            return Some((editor.len_chars(), x < metrics.gutter));
        }
        line += 1;
        row = 0;
    }
}

/// The span a press selects at `pos`: nothing, the word, or the line.
fn unit_span(editor: &Editor, pos: usize, unit: Unit) -> (usize, usize) {
    match unit {
        Unit::Char => (pos, pos),
        Unit::Word => editor.word_at(pos),
        Unit::Line => {
            let line = editor.line_of(pos);
            let end = if line + 1 < editor.len_lines() {
                editor.line_start(line + 1)
            } else {
                editor.len_chars()
            };
            (editor.line_start(line), end)
        }
    }
}

/// `scroll` moved down by `delta` px (up when negative), row by row.
fn scroll_by(editor: &Editor, scroll: ScrollPos, delta: f32, metrics: &Metrics) -> ScrollPos {
    let mut scroll = scroll;
    scroll.offset += delta;
    let lines = editor.len_lines();
    while scroll.offset >= metrics.row {
        let rows = editor.layout_wrapped(scroll.line, metrics.wrap).rows.len();
        if scroll.row + 1 < rows {
            scroll.row += 1;
        } else if scroll.line + 1 < lines {
            scroll.line += 1;
            scroll.row = 0;
        } else {
            scroll.offset = 0.0;
            break;
        }
        scroll.offset -= metrics.row;
    }
    while scroll.offset < 0.0 {
        if scroll.row > 0 {
            scroll.row -= 1;
        } else if scroll.line > 0 {
            scroll.line -= 1;
            scroll.row = editor.layout_wrapped(scroll.line, metrics.wrap).rows.len() - 1;
        } else {
            scroll.offset = 0.0;
            break;
        }
        scroll.offset += metrics.row;
    }
    scroll
}

/// Keeps the top row inside the text, and the last line no higher than
/// the middle of the view (a little room past the end, as other editors
/// leave).
fn clamp_scroll(editor: &Editor, scroll: ScrollPos, metrics: &Metrics) -> ScrollPos {
    let mut scroll = scroll;
    let lines = editor.len_lines();
    if scroll.line >= lines {
        scroll.line = lines.saturating_sub(1);
        scroll.row = 0;
        scroll.offset = 0.0;
    }
    let rows = editor.layout_wrapped(scroll.line, metrics.wrap).rows.len();
    if scroll.row >= rows {
        scroll.row = rows - 1;
    }
    // How many rows are there from the top to the end?
    let limit = metrics.fit() / 2 + 1;
    let mut below = rows - scroll.row;
    let mut line = scroll.line + 1;
    while below < limit && line < lines {
        below += editor.layout_wrapped(line, metrics.wrap).rows.len();
        line += 1;
    }
    if below < limit && line >= lines {
        let back = (limit - below) as f32 * metrics.row;
        let total_rows_above = scroll.line + scroll.row;
        if total_rows_above > 0 {
            scroll = scroll_by(editor, scroll, -back, metrics);
        } else {
            scroll.offset = 0.0;
        }
    }
    scroll
}

/// The scroll position that shows the primary caret, keeping `scroll` when
/// it already does.
fn follow(editor: &Editor, scroll: ScrollPos, metrics: &Metrics) -> ScrollPos {
    let head = editor.selection().primary().head;
    let line = editor.line_of(head);
    let layout = editor.layout_wrapped(line, metrics.wrap);
    let (row, col) = layout.col_in_row(head - editor.line_start(line));
    let mut out = scroll;
    if metrics.wrap.is_none() {
        let x = col as f32 * metrics.cell;
        let margin = SIDE_MARGIN * metrics.cell;
        if x < out.x + margin {
            out.x = (x - margin).max(0.0);
        } else if x > out.x + metrics.text_width - margin {
            out.x = x - metrics.text_width + margin;
        }
    }
    // Above the top row?
    if (line, row) < (out.line, out.row) || ((line, row) == (out.line, out.row) && out.offset > 0.0)
    {
        out.line = line;
        out.row = row;
        out.offset = 0.0;
        return out;
    }
    // Below the last whole row?
    let fit = metrics.fit();
    let mut counted = 0usize;
    let (mut at_line, mut at_row) = (out.line, out.row);
    while (at_line, at_row) != (line, row) && counted <= fit {
        let rows = editor.layout_wrapped(at_line, metrics.wrap).rows.len();
        if at_row + 1 < rows {
            at_row += 1;
        } else {
            at_line += 1;
            at_row = 0;
        }
        counted += 1;
    }
    let bottom = (counted + 1) as f32 * metrics.row - out.offset;
    if counted > fit || bottom > metrics.height {
        // Far away: centre it. Just below: bring it to the last row.
        let back = if counted > fit + fit / 2 {
            fit / 2
        } else {
            fit.saturating_sub(1)
        };
        let mut top = ScrollPos {
            line,
            row,
            offset: 0.0,
            x: out.x,
        };
        top = scroll_by(editor, top, -(back as f32) * metrics.row, metrics);
        top.offset = 0.0;
        return top;
    }
    out
}

/// Where the scroll bar's thumb is (its top and length in px), or `None`
/// when everything fits.
fn thumb_geometry(
    editor: &Editor,
    scroll: &ScrollPos,
    metrics: &Metrics,
    shown_rows: usize,
) -> Option<(f32, f32)> {
    let lines = editor.len_lines();
    if metrics.height <= 0.0
        || (scroll.line == 0 && scroll.row == 0 && lines <= shown_rows.min(metrics.fit()))
    {
        return None;
    }
    let track = metrics.height - S2.px();
    let length = (track * metrics.fit() as f32 / lines.max(1) as f32).clamp(THUMB_MIN, track);
    let fraction = scroll.line as f32 / (lines.saturating_sub(1)).max(1) as f32;
    Some((S1.px() + fraction * (track - length), length))
}

/// The pointer moved with the button down inside the selection: once it is
/// past the drag slop, the selected text is dragged. It goes to the system
/// (other programs take it as text, and a drop on this document moves it);
/// without drag and drop it stays in the window.
fn start_text_drag(
    handle: &EditorHandle,
    pointer: &Rc<RefCell<PointerState>>,
    dragger: &crate::dnd::Dragger,
    at: Point,
) {
    let Some(grab) = pointer.borrow().grab else {
        return;
    };
    if (at.0 - grab.from.0).hypot(at.1 - grab.from.1) <= DRAG_SLOP {
        return;
    }
    let text = handle
        .peek()
        .rope()
        .slice(grab.range.0..grab.range.1)
        .to_string();
    let moved = Rc::new(std::cell::Cell::new(false));
    let source = handle.clone();
    let removal = text.clone();
    let flag = moved.clone();
    let payload = Payload::new().text(text).local(TextDrag {
        handle: handle.clone(),
        range: grab.range,
        moved,
    });
    let request = DragRequest::new(payload, grab.from)
        .actions(DragActions::COPY_MOVE)
        .image(DragImage::None)
        .on_end(move |end: DragEnd| {
            // Another program took the text as a move: it is gone from here.
            if matches!(end, DragEnd::Dropped(DragAction::Move)) && !flag.get() {
                source.update(|editor| editor.remove_if_unchanged(grab.range, &removal));
            }
        });
    let system = dragger.start(request);
    {
        let mut state = pointer.borrow_mut();
        state.grab = None;
        state.drag = (!system).then_some(Drag::TextInWindow);
    }
    if !system {
        dragger.moved(at);
    }
}

/// Moves the view so the thumb's top is at `top` px.
fn drag_thumb(handle: &EditorHandle, metrics: &Metrics, top: f32) {
    {
        let editor = handle.peek();
        let lines = editor.len_lines();
        let track = metrics.height - S2.px();
        let length = (track * metrics.fit() as f32 / lines.max(1) as f32).clamp(THUMB_MIN, track);
        let fraction = ((top - S1.px()) / (track - length).max(1.0)).clamp(0.0, 1.0);
        let line = (fraction * lines.saturating_sub(1) as f32).round() as usize;
        let mut memory = handle.memory();
        memory.scroll = ScrollPos {
            line,
            row: 0,
            offset: 0.0,
            x: memory.scroll.x,
        };
        memory.scroll = clamp_scroll(&editor, memory.scroll, metrics);
        memory.followed = Some((editor.revision(), editor.selection().clone()));
    }
    handle.refresh();
}

/// Cut, Copy, Paste, Select All and the caller's items.
fn context_menu(handle: EditorHandle, read_only: bool, extra: Vec<MenuItem>) -> Vec<MenuItem> {
    let has_selection = handle.peek().selection().has_selection();
    let cut = handle.clone();
    let copy = handle.clone();
    let paste = handle.clone();
    let all = handle.clone();
    let mut items = vec![
        {
            let item = menu_item("Cut", move |_| {
                if let Some(copied) = cut.update(|editor| editor.cut()) {
                    let _ = Clipboard::set(copied.text);
                }
            })
            .shortcut("Ctrl+X");
            if read_only || !has_selection {
                item.disabled()
            } else {
                item
            }
        },
        {
            let item = menu_item("Copy", move |_| {
                if let Some(copied) = copy.update(|editor| editor.copy()) {
                    let _ = Clipboard::set(copied.text);
                }
            })
            .shortcut("Ctrl+C");
            if has_selection { item } else { item.disabled() }
        },
        {
            let item = menu_item("Paste", move |_| {
                if let Ok(text) = Clipboard::get() {
                    paste.update(|editor| editor.paste(&text));
                }
            })
            .shortcut("Ctrl+V");
            if read_only { item.disabled() } else { item }
        },
        menu_separator(),
        menu_item("Select all", move |_| {
            all.update(|editor| editor.select_all())
        })
        .shortcut("Ctrl+A"),
    ];
    if !extra.is_empty() {
        items.push(menu_separator());
        items.extend(extra);
    }
    items
}

/// The carets. A component of its own so its blink redraws only it.
#[derive(Clone, PartialEq)]
struct Carets {
    points: Vec<(f32, f32)>,
    height: f32,
    visible: bool,
    /// When the caret last moved: it stays solid for a moment after.
    moved: Instant,
}

impl Component for Carets {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut lit = use_state(|| true);
        let reduced = tokens.reduced_motion;
        let moved = self.moved;
        let visible = self.visible;
        // One task blinks while the editor has focus; a move restarts the
        // wait with the caret solid.
        use_side_effect_with_deps(&(moved, visible), move |(moved, visible)| {
            lit.set(true);
            if !*visible || reduced {
                return;
            }
            let moved = *moved;
            spawn(async move {
                timer(BLINK_WAIT).await;
                loop {
                    timer(BLINK_HALF).await;
                    if *lit.peek() && Instant::now().duration_since(moved) < BLINK_WAIT {
                        continue;
                    }
                    let next = !*lit.peek();
                    lit.set(next);
                }
            });
        });
        let show = self.visible && *lit.read();
        rect()
            .position(Position::new_absolute().left(0.0).top(0.0))
            .children(self.points.iter().filter(|_| show).map(|(x, y)| {
                rect()
                    .position(Position::new_absolute().left(*x - CARET / 2.0).top(*y))
                    .width(Size::px(CARET))
                    .height(Size::px(self.height))
                    .tw(&tokens, &[bg(Paint::Accent), rounded(Radius::Mark)])
                    .into_element()
            }))
    }
}

/// The caption a status bar shows for a selection: `Line 3, Column 14`, or
/// how much is selected.
pub fn caret_caption(editor: &Editor) -> String {
    let selection = editor.selection();
    if selection.is_multi() {
        return format!("{} selections", selection.len());
    }
    let range = selection.primary();
    if !range.is_empty() {
        let chars = range.len();
        // A selection that ends at the start of a line (a whole line picked
        // from the gutter or with Ctrl+L) does not reach into that line.
        let mut last = editor.line_of(range.end());
        let first = editor.line_of(range.start());
        if last > first && editor.line_col(range.end()).1 == 0 {
            last -= 1;
        }
        let lines = last - first + 1;
        let characters = if chars == 1 {
            "1 character".to_string()
        } else {
            format!("{chars} characters")
        };
        return if lines > 1 {
            format!("{characters}, {lines} lines selected")
        } else {
            format!("{characters} selected")
        };
    }
    let (line, col) = editor.line_col(range.head);
    let layout = editor.layout_wrapped(line, None);
    format!("Line {}, Column {}", line + 1, layout.col(col) + 1)
}

/// A muted caption in the editor's face, for a status bar.
pub fn status_caption(tokens: &crate::theme::Tokens, text: impl Into<String>) -> Label {
    CAPTION
        .in_ink(Ink::Muted)
        .label(tokens, text)
        .font_tabular()
}

/// The modifiers held right now: a wheel or pointer event carries none.
fn held() -> Modifiers {
    crate::input::held_modifiers()
}
