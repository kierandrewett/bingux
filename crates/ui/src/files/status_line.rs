//! The pill over a file view: how many items there are, how many are
//! selected, and what the view is doing.
//!
//! It floats over the bottom-right corner of the list, sized to its text, like
//! a browser's link preview, so it takes no row from the list. The surface,
//! border and padding are the tooltip's, and the fill is opaque, so a row under
//! it never shows through. It says as little as it can: `95 items`, or
//! `1 of 95 selected (19 kB)` once something is selected. What the view hides (dot files, the file type filter)
//! is not its business: the view menu and the empty state say that.
//!
//! It keeps out of the pointer's way. The pointer is followed on the whole
//! window, not only over the pill, because the list swallows pointer events and
//! a pill under a row that is being pressed would never see one. When the
//! pointer comes within a small margin of the pill it slides to the opposite
//! bottom corner; it returns to the default corner when the pointer leaves the
//! list. A running operation replaces the text with its label, a progress bar
//! and a cancel button, and stays put so the button can be pressed. Nothing
//! shows when there is nothing to say. Figures use tabular digits, so a count
//! that ticks up while a big folder loads does not shimmer.

use bingux_files::display;
use freya::prelude::*;

use super::actions::Ui;
use crate::component::ComponentName;
use crate::controls::{icon_button, progress_bar};
use crate::icons::Icon;
use crate::motion::use_tween;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink, TextStyle};

/// The status line's text: a caption with digits of one width.
const STATUS: TextStyle = TextStyle {
    tabular: true,
    ..CAPTION
};

/// How long the pill takes to slide to the other corner.
const SLIDE_MS: u64 = 150;

/// What the line reports about the folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct StatusInfo {
    /// Entries on show.
    pub shown: usize,
    /// Entries read so far, for the count while a folder loads.
    pub total_listed: usize,
    pub selected: usize,
    /// The size of the selected files; zero when none or only folders.
    pub selected_bytes: u64,
    /// The folder is still being read.
    pub loading: bool,
    /// The whole folder has been read.
    pub complete: bool,
    /// The drive has not answered for a while.
    pub unresponsive: bool,
    /// The folder on show is in the Trash: the line says so.
    pub trash: bool,
    /// Where the one selected item leads, when it is a link.
    pub link: Option<String>,
    /// The read of the folder was stopped (Stop, Escape): what is shown is
    /// what had been read.
    pub stopped: bool,
}

/// `1204` as `1,204`.
fn grouped(number: usize) -> String {
    let digits = number.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

fn item_count(count: usize) -> String {
    format!(
        "{} {}",
        grouped(count),
        if count == 1 { "item" } else { "items" }
    )
}

fn folder_count(count: usize) -> String {
    format!(
        "{} {}",
        grouped(count),
        if count == 1 { "folder" } else { "folders" }
    )
}

/// The text of the line while nothing else is going on.
pub(super) fn status_text(info: &StatusInfo) -> String {
    if info.unresponsive {
        return "Waiting for the drive to answer…".to_owned();
    }
    if info.stopped {
        return format!("Stopped · {} read so far", item_count(info.total_listed));
    }
    if info.loading && !info.complete {
        return format!("Loading… {}", item_count(info.total_listed));
    }
    let mut size = String::new();
    if info.selected_bytes > 0 {
        size = format!(" ({})", display::size(info.selected_bytes));
    }
    match () {
        _ if info.selected > 0 => format!(
            "{}{} of {} selected{size}{}",
            if info.trash { "Trash · " } else { "" },
            grouped(info.selected),
            grouped(info.shown),
            info.link
                .as_ref()
                .map_or_else(String::new, |target| format!(" · Link to {target}"))
        ),
        _ if info.trash => format!("Trash · {}", item_count(info.shown)),
        _ => item_count(info.shown),
    }
}

/// The status line.
pub(super) fn status_line(info: StatusInfo, ui: Ui) -> Element {
    StatusLine { info, ui }.into()
}

/// Whether the pill has something to say. An empty folder says nothing: its
/// empty state already does.
fn has_news(info: &StatusInfo) -> bool {
    info.loading || info.unresponsive || info.stopped || info.shown > 0 || info.selected > 0
}

/// The bottom corner of the view the pill sits in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Corner {
    Left,
    Right,
}

/// A rectangle in window coordinates: `(left, top, right, bottom)`.
type Bounds = (f32, f32, f32, f32);

/// Where the pill of `size` sits in `corner` of the strip `area`.
fn pill_bounds(area: Bounds, size: (f32, f32), corner: Corner, edge: f32) -> Bounds {
    let (left, _, right, bottom) = area;
    let x = match corner {
        Corner::Left => left + edge,
        Corner::Right => right - edge - size.0,
    };
    let y = bottom - edge - size.1;
    (x, y, x + size.0, y + size.1)
}

fn near(bounds: Bounds, point: (f32, f32), margin: f32) -> bool {
    point.0 >= bounds.0 - margin
        && point.0 <= bounds.2 + margin
        && point.1 >= bounds.1 - margin
        && point.1 <= bounds.3 + margin
}

/// The corner the pill should be in once the pointer is at `point`. It leaves
/// a corner the pointer is approaching, unless the pointer is as close to the
/// other one; it goes back to the default corner when the pointer is outside
/// the list's width or below it.
fn corner_for(
    now: Corner,
    area: Bounds,
    size: (f32, f32),
    point: (f32, f32),
    edge: f32,
    margin: f32,
) -> Corner {
    let inside = point.0 >= area.0 && point.0 <= area.2 && point.1 <= area.3 + edge;
    if !inside {
        return Corner::Right;
    }
    let other = match now {
        Corner::Left => Corner::Right,
        Corner::Right => Corner::Left,
    };
    if near(pill_bounds(area, size, now, edge), point, margin)
        && !near(pill_bounds(area, size, other, edge), point, margin)
    {
        return other;
    }
    now
}

#[derive(PartialEq)]
struct StatusLine {
    info: StatusInfo,
    ui: Ui,
}

impl Component for StatusLine {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut corner = use_state(|| Corner::Right);
        let mut strip = use_state(Area::default);
        let mut size = use_state(|| (0.0_f32, 0.0_f32));
        let slide = use_tween(
            if corner() == Corner::Right { 1.0 } else { 0.0 },
            tokens.motion(SLIDE_MS),
        );
        let busy = self.ui.busy.read().clone();
        let notice = self.ui.notice.read().clone();
        if busy.is_none() && notice.is_none() && !has_news(&self.info) {
            return rect().interactive(false).into_element();
        }
        let edge = S2.px();
        let surface = |pill: Rect| {
            pill.component("StatusLine")
                // Above the list's rows, their icons and their hover fills.
                .layer(crate::layers::STATUS_PILL)
                .a11y_role(AccessibilityRole::Status)
                .tw(
                    &tokens,
                    &[flex_row(), grow_children(), items(Align::Center), gap(S2)],
                )
                .tw(&tokens, TOOLTIP)
                .tw(
                    &tokens,
                    &[border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline))],
                )
        };
        if let Some(busy) = busy {
            // An operation in progress has a button to press: the pill stays
            // where it is, and takes pointer input.
            let cancel = busy.cancel.clone();
            return surface(rect().position(Position::new_absolute().bottom(edge).right(edge)))
                .child(
                    STATUS
                        .in_ink(Ink::OnFloating)
                        .label(&tokens, busy.label)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .child(
                    rect()
                        .tw(&tokens, &[w(Dim::Field)])
                        .child(progress_bar(busy.fraction.map(f64::from))),
                )
                .maybe_child(cancel.map(|cancel| {
                    icon_button(Icon::WindowClose, move |_| cancel.cancel())
                        .size(Dim::InlineButton)
                        .tooltip("Cancel")
                }))
                .into_element();
        }
        let (text, ink) = match notice {
            Some(notice) => (
                notice.text,
                if notice.error {
                    Ink::DangerOnFloating
                } else {
                    Ink::OnFloating
                },
            ),
            None => (status_text(&self.info), Ink::OnFloating),
        };
        // Settled in a corner the pill is pinned to it; while it slides it is
        // placed from the left.
        let strip_width = strip.read().width();
        let anchor = if slide >= 1.0 {
            Position::new_absolute().bottom(edge).right(edge)
        } else if slide <= 0.0 {
            Position::new_absolute().bottom(edge).left(edge)
        } else {
            let travel = (strip_width - 2.0 * edge - size().0).max(0.0);
            Position::new_absolute()
                .bottom(edge)
                .left(edge + travel * slide)
        };
        // The pill is as wide as its text up to the strip less the room it keeps from each edge: a long
        // text (a link's target) is cut with an ellipsis inside the strip, not laid out wider than it
        // and pinned past its left edge.
        let pill = surface(rect().position(anchor))
            .maybe(strip_width > 0.0, |pill| {
                // ui-lint: allow(the pill's cap is the strip's measured width less the edge it keeps on each side)
                pill.max_width(Size::px((strip_width - 2.0 * edge).max(0.0)))
            })
            .on_sized(move |event: Event<SizedEventData>| {
                size.set_if_modified((event.area.width(), event.area.height()));
            })
            .child(
                STATUS
                    .in_ink(ink)
                    .label(&tokens, text)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            );
        // A strip along the bottom of the list that only measures it and
        // listens to the pointer. It takes no input itself, so the list under
        // it keeps every press.
        rect()
            .component("StatusLineStrip")
            .position(Position::new_absolute().left(0.0).bottom(0.0))
            .tw(&tokens, &[w_full()])
            .interactive(false)
            .on_sized(move |event: Event<SizedEventData>| strip.set_if_modified(event.area))
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let area = strip.peek();
                let bounds = (area.min_x(), area.min_y(), area.max_x(), area.max_y());
                let next = corner_for(
                    *corner.peek(),
                    bounds,
                    *size.peek(),
                    (at.x as f32, at.y as f32),
                    S2.px(),
                    S3.px(),
                );
                corner.set_if_modified(next);
            })
            .child(pill)
            .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(shown: usize) -> StatusInfo {
        StatusInfo {
            shown,
            total_listed: shown,
            complete: true,
            ..StatusInfo::default()
        }
    }

    #[test]
    fn one_selected_link_says_where_it_leads() {
        let one = StatusInfo {
            selected: 1,
            link: Some("/mnt/x/Downloads".into()),
            ..info(5)
        };
        assert_eq!(
            status_text(&one),
            "1 of 5 selected · Link to /mnt/x/Downloads"
        );
    }

    #[test]
    fn an_empty_folder_has_nothing_to_say() {
        assert!(!has_news(&info(0)));
        assert!(has_news(&info(3)));
        assert!(has_news(&StatusInfo {
            loading: true,
            complete: false,
            ..info(0)
        }));
    }

    #[test]
    fn a_stopped_read_says_how_much_it_had_read() {
        let stopped = StatusInfo {
            stopped: true,
            complete: false,
            ..info(12_000)
        };
        assert_eq!(status_text(&stopped), "Stopped · 12,000 items read so far");
        assert!(has_news(&stopped));
    }

    #[test]
    fn thousands_are_separated() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_204), "1,204");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn one_item_is_singular() {
        assert_eq!(status_text(&info(1)), "1 item");
        assert_eq!(status_text(&info(0)), "0 items");
        assert_eq!(status_text(&info(1204)), "1,204 items");
    }

    #[test]
    fn a_selection_says_how_many_of_the_items_with_their_size() {
        let some = StatusInfo {
            selected: 3,
            selected_bytes: 4_200_000,
            ..info(1204)
        };
        let text = status_text(&some);
        assert!(text.starts_with("3 of 1,204 selected ("), "{text}");
        assert!(text.ends_with(')'), "{text}");
        let folders = StatusInfo {
            selected: 1,
            ..info(95)
        };
        assert_eq!(status_text(&folders), "1 of 95 selected");
    }

    #[test]
    fn the_pill_says_nothing_of_what_is_hidden() {
        let text = status_text(&info(95));
        assert!(!text.contains("hidden"), "{text}");
    }

    #[test]
    fn loading_and_unresponsive() {
        let loading = StatusInfo {
            loading: true,
            complete: false,
            total_listed: 1204,
            ..info(0)
        };
        assert_eq!(status_text(&loading), "Loading… 1,204 items");
        let stuck = StatusInfo {
            unresponsive: true,
            ..loading
        };
        assert_eq!(status_text(&stuck), "Waiting for the drive to answer…");
    }

    const LIST: Bounds = (100.0, 0.0, 900.0, 600.0);
    const PILL: (f32, f32) = (120.0, 24.0);

    #[test]
    fn the_pill_sits_in_the_bottom_corners_inset_by_the_edge() {
        assert_eq!(
            pill_bounds(LIST, PILL, Corner::Right, 8.0),
            (772.0, 568.0, 892.0, 592.0)
        );
        assert_eq!(
            pill_bounds(LIST, PILL, Corner::Left, 8.0),
            (108.0, 568.0, 228.0, 592.0)
        );
    }

    #[test]
    fn the_pill_dodges_a_pointer_that_nears_it_and_returns_when_the_pointer_leaves() {
        let go = |now, at| corner_for(now, LIST, PILL, at, 8.0, 12.0);
        // Far from it: stays.
        assert_eq!(go(Corner::Right, (500.0, 300.0)), Corner::Right);
        // Within the margin of the right pill: moves left.
        assert_eq!(go(Corner::Right, (770.0, 575.0)), Corner::Left);
        assert_eq!(go(Corner::Right, (760.0, 560.0)), Corner::Left);
        // Nearing it on the left: moves back right.
        assert_eq!(go(Corner::Left, (200.0, 580.0)), Corner::Right);
        // Left in the other corner while the pointer wanders the list.
        assert_eq!(go(Corner::Left, (500.0, 300.0)), Corner::Left);
        // Outside the list: the default corner.
        assert_eq!(go(Corner::Left, (50.0, 300.0)), Corner::Right);
        assert_eq!(go(Corner::Left, (500.0, 700.0)), Corner::Right);
    }

    #[test]
    fn a_pointer_close_to_both_corners_does_not_make_the_pill_flicker() {
        let narrow: Bounds = (0.0, 0.0, 260.0, 600.0);
        let at = (130.0, 580.0);
        assert_eq!(
            corner_for(Corner::Right, narrow, PILL, at, 8.0, 12.0),
            Corner::Right
        );
        assert_eq!(
            corner_for(Corner::Left, narrow, PILL, at, 8.0, 12.0),
            Corner::Left
        );
    }
}
