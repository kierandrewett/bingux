//! The Filters button of the search bar and the panel it opens.
//!
//! The panel is a popover under the button on a desktop and a sheet on a
//! phone ([`PanelKind::Choice`]). It edits [`Find::filters`], the same filters
//! `key:value` in the field makes, so the two never disagree:
//!
//! | Row | Sets | Typed as |
//! | --- | ---- | -------- |
//! | Type | any, folders, documents, images, video, audio, archives, code | `type:image` |
//! | Modified | any, today, last 7 days, last 30 days, this year, a range | `modified:week`, `modified:2024-01-01..2024-02-28` |
//! | Size | any, small (under 1 MB), medium (1 to 100 MB), large (over 100 MB), a range | `size:>100MB` |
//! | Name | contains, starts with, is exactly, a regular expression | `match:starts` |
//! | Sort | relevance, name, date modified, size | `sort:date` |
//! | Hidden files, Match case | switches | `hidden:yes`, `case:yes` |
//! | Build and dependency folders | a switch; the window's, not the query's | |
//!
//! File contents are not searched, so there is no switch for that.
//!
//! The rows are always the same height. A range shows its two fields under
//! the row; nothing else moves.

use bingux_files::find::{
    Clock, Filters, Hidden, Kind, Modified, Moment, NameMode, SizeCond, SortBy, day_text, parse_day, shift_days,
};
use freya::prelude::*;

use super::search::Find;
use crate::component::ComponentName;
use crate::controls::{action_button, dropdown, switch, text_field};
use crate::icons::Icon;
use crate::shell::panel::{PANEL_EDGE, PANEL_GAP, Panel, PanelKind, PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink};

// ---------------------------------------------------------------------------
// The choices, as keys
// ---------------------------------------------------------------------------

const ANY: &str = "any";
const CUSTOM: &str = "custom";
const MULTI: &str = "multi";

/// What the Type row offers: key, label.
pub(super) fn kind_options(filters: &Filters) -> Vec<(String, String)> {
    let mut options = vec![
        (ANY.to_string(), "Any type".to_string()),
        (Kind::Folder.word().to_string(), "Folders".to_string()),
        (Kind::Document.word().to_string(), "Documents".to_string()),
        (Kind::Image.word().to_string(), "Images".to_string()),
        (Kind::Video.word().to_string(), "Video".to_string()),
        (Kind::Audio.word().to_string(), "Audio".to_string()),
        (Kind::Archive.word().to_string(), "Archives".to_string()),
        (Kind::Code.word().to_string(), "Code".to_string()),
    ];
    if filters.kinds.len() > 1 {
        options.push((MULTI.to_string(), kind_names(&filters.kinds)));
    }
    options
}

pub(super) fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Folder => "Folders",
        Kind::Image => "Images",
        Kind::Video => "Video",
        Kind::Audio => "Audio",
        Kind::Document => "Documents",
        Kind::Archive => "Archives",
        Kind::Code => "Code",
    }
}

fn kind_names(kinds: &[Kind]) -> String {
    kinds.iter().map(|kind| kind_name(*kind)).collect::<Vec<_>>().join(", ")
}

pub(super) fn kind_key(filters: &Filters) -> String {
    match filters.kinds.as_slice() {
        [] => ANY.to_string(),
        [one] => one.word().to_string(),
        _ => MULTI.to_string(),
    }
}

/// Set the Type row to `key`; the "several" entry changes nothing.
pub(super) fn set_kind(filters: &mut Filters, key: &str) {
    if key == MULTI {
        return;
    }
    filters.kinds = Kind::from_word(key).into_iter().collect();
}

const MB: u64 = 1_000_000;

pub(super) const MODIFIED_PRESETS: [(&str, &str); 6] = [
    (ANY, "Any time"),
    ("today", "Today"),
    ("week", "Last 7 days"),
    ("month", "Last 30 days"),
    ("year", "This year"),
    (CUSTOM, "Custom range…"),
];

pub(super) fn modified_key(modified: Option<Modified>) -> &'static str {
    match modified {
        None => ANY,
        Some(Modified::Today) => "today",
        Some(Modified::Days(7)) => "week",
        Some(Modified::Days(30)) => "month",
        Some(Modified::ThisYear) => "year",
        Some(_) => CUSTOM,
    }
}

/// The condition a preset key stands for; `None` for any time and for custom.
pub(super) fn modified_preset(key: &str) -> Option<Modified> {
    match key {
        "today" => Some(Modified::Today),
        "week" => Some(Modified::Days(7)),
        "month" => Some(Modified::Days(30)),
        "year" => Some(Modified::ThisYear),
        _ => None,
    }
}

pub(super) const SIZE_PRESETS: [(&str, &str); 5] = [
    (ANY, "Any size"),
    ("small", "Small (under 1 MB)"),
    ("medium", "Medium (1 to 100 MB)"),
    ("large", "Large (over 100 MB)"),
    (CUSTOM, "Custom range…"),
];

pub(super) fn size_key(size: Option<SizeCond>) -> &'static str {
    match size {
        None => ANY,
        Some(SizeCond::Lt(n)) if n == MB => "small",
        Some(SizeCond::Between(low, high)) if low == MB && high == 100 * MB => "medium",
        Some(SizeCond::Gt(n)) if n == 100 * MB => "large",
        Some(_) => CUSTOM,
    }
}

pub(super) fn size_preset(key: &str) -> Option<SizeCond> {
    match key {
        "small" => Some(SizeCond::Lt(MB)),
        "medium" => Some(SizeCond::Between(MB, 100 * MB)),
        "large" => Some(SizeCond::Gt(100 * MB)),
        _ => None,
    }
}

pub(super) const NAME_MODES: [(&str, &str, NameMode); 4] = [
    ("contains", "Contains", NameMode::Contains),
    ("starts", "Starts with", NameMode::Starts),
    ("exact", "Is exactly", NameMode::Exact),
    ("regex", "Regular expression", NameMode::Regex),
];

pub(super) const SORTS: [(&str, &str, SortBy); 4] = [
    ("relevance", "Relevance", SortBy::Relevance),
    ("name", "Name", SortBy::Name),
    ("date", "Date modified", SortBy::Modified),
    ("size", "Size", SortBy::Size),
];

fn options_of<const N: usize>(table: [(&str, &str); N]) -> Vec<(String, String)> {
    table.iter().map(|(key, label)| (key.to_string(), label.to_string())).collect()
}

// ---------------------------------------------------------------------------
// Custom ranges, as the two fields
// ---------------------------------------------------------------------------

/// The text of a size bound in MB: `1`, `0.5`, nothing for no bound.
pub(super) fn mb_text(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return String::new();
    };
    if bytes % MB == 0 {
        return (bytes / MB).to_string();
    }
    let text = format!("{:.3}", bytes as f64 / MB as f64);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A size bound typed in MB; `Err` for text that is not a size.
fn parse_mb(text: &str) -> Result<Option<u64>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let value: f64 = text.parse().map_err(|_| ())?;
    if !value.is_finite() || value < 0.0 || value > 1.0e9 {
        return Err(());
    }
    Ok(Some((value * MB as f64).round() as u64))
}

/// The condition the two size fields stand for. `Err` when one is not a size
/// or the minimum is over the maximum; `Ok(None)` when both are empty.
pub(super) fn size_from_fields(min: &str, max: &str) -> Result<Option<SizeCond>, ()> {
    Ok(match (parse_mb(min)?, parse_mb(max)?) {
        (None, None) => None,
        (Some(low), None) => Some(SizeCond::Ge(low)),
        (None, Some(high)) => Some(SizeCond::Le(high)),
        (Some(low), Some(high)) if low <= high => Some(SizeCond::Between(low, high)),
        _ => return Err(()),
    })
}

/// The two size fields for a condition.
pub(super) fn size_fields(size: Option<SizeCond>) -> (String, String) {
    match size.map(SizeCond::bounds) {
        Some((low, high)) => (mb_text(low), mb_text(high)),
        None => (String::new(), String::new()),
    }
}

/// The condition the two date fields stand for (`YYYY-MM-DD`, the last day
/// included). `Err` when one is not a day or the first is after the second.
pub(super) fn modified_from_fields(from: &str, to: &str) -> Result<Option<Modified>, ()> {
    let day = |text: &str| -> Result<Option<Moment>, ()> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        parse_day(text).map(Some).ok_or(())
    };
    Ok(match (day(from)?, day(to)?) {
        (None, None) => None,
        (Some(from), None) => Some(Modified::Since(from)),
        (None, Some(to)) => Some(Modified::Before(shift_days(to, 1))),
        (Some(from), Some(to)) if from.start() <= to.start() => Some(Modified::Between(from, to)),
        _ => return Err(()),
    })
}

/// The two date fields for a condition, today being `today`.
pub(super) fn modified_fields(modified: Option<Modified>, today: Moment) -> (String, String) {
    let moment_text = |moment: Moment| day_text(moment);
    match modified {
        Some(Modified::Yesterday) => (moment_text(shift_days(today, -1)), moment_text(shift_days(today, -1))),
        Some(Modified::Days(days)) => (moment_text(shift_days(today, -i64::from(days))), String::new()),
        Some(Modified::Today) => (moment_text(today), moment_text(today)),
        Some(Modified::ThisYear) => (format!("{:04}-01-01", today.year), format!("{:04}-12-31", today.year)),
        Some(Modified::On(moment)) => (moment_text(moment), moment_text(moment.last_day())),
        Some(Modified::Since(moment)) => (moment_text(moment), String::new()),
        Some(Modified::Before(moment)) => (String::new(), moment_text(shift_days(moment, -1))),
        Some(Modified::Between(from, to)) => (moment_text(from), moment_text(to.last_day())),
        None => (String::new(), String::new()),
    }
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

/// How many form rows the panel has for `filters`, with `custom_when` and
/// `custom_size` the ranges shown: Type, Modified, (From, To), Size,
/// (Min, Max), Name, Sort, three switches.
pub(super) fn row_count(custom_when: bool, custom_size: bool) -> usize {
    5 + 3 + if custom_when { 2 } else { 0 } + if custom_size { 2 } else { 0 }
}

/// The height of the panel's rows: every row `Dim::Control`, `S2` between,
/// and the footer.
pub(super) fn form_height(rows: usize) -> f32 {
    let rows = rows + 1;
    rows as f32 * Dim::Control.px() + (rows.saturating_sub(1)) as f32 * S2.px()
}

/// The panel's width: the label column, the gap and the controls, inside
/// the panel's own padding.
pub(super) fn form_width() -> f32 {
    Dim::Box24.px() + S3.px() + Dim::Menu.px() + 2.0 * S2.px()
}

/// The Filters button and its panel.
#[derive(PartialEq)]
pub(super) struct FiltersButton {
    pub find: State<Find>,
    /// The window's "search build and dependency folders" switch.
    pub include_noise: State<bool>,
}

impl Component for FiltersButton {
    fn render(&self) -> impl IntoElement {
        let mut find = self.find;
        let mut area = use_state(Area::default);
        let scroll = use_scroll_controller(ScrollConfig::default);
        let (count, open) = {
            let now = find.read();
            (now.filters.count(), now.filters_open)
        };
        let label = if count == 0 {
            "Filters".to_string()
        } else {
            format!("Filters ({count})")
        };
        let toggle = EventHandler::new(move |_: ()| {
            let open = find.peek().filters_open;
            find.write().filters_open = !open;
        });
        let button = action_button(label, toggle.clone()).icon(Icon::Filter);
        // The size of the panel follows the ranges the form shows.
        let (custom_when, custom_size) = {
            let now = find.peek();
            (
                modified_key(now.filters.modified) == CUSTOM,
                size_key(now.filters.size) == CUSTOM,
            )
        };
        let content = FiltersForm {
            find,
            include_noise: self.include_noise,
        };
        let height = form_height(row_count(custom_when, custom_size));
        let mut panel = Panel::new(
            PanelKind::Choice,
            open,
            PanelTrigger::Control(PanelRect::from_area(&area.read())),
            vec![Element::from(content)],
        )
        .size(form_width(), height)
        .tall()
        .title("Filters")
        .on_dismiss(EventHandler::new(move |_: ()| find.write().filters_open = false));
        // A popover is as tall as its rows; a sheet scrolls, and so does a
        // popover that would not fit under the button in a short window (the
        // panel then caps its height and shows a scrollbar).
        let root = crate::surface::window_size();
        let below = area.read().max_y() + PANEL_GAP + height + 2.0 * S2.px() + PANEL_EDGE;
        if is_compact() || below > root.height {
            panel = panel.scroll(scroll);
        }
        rect()
            .component("FiltersButton")
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .child(button)
            .child(Element::from(panel))
    }
}

/// The rows of the panel.
#[derive(PartialEq)]
struct FiltersForm {
    find: State<Find>,
    include_noise: State<bool>,
}

fn form_row(tokens: &crate::theme::Tokens, label: &str, control: impl Into<Element>) -> Rect {
    rect()
        .tw(tokens, &[flex_row(), grow_children(), items(Align::Center), gap(S3), w_full(), h(Dim::Control)])
        .child(
            rect()
                .tw(tokens, &[w(Dim::Box24)])
                .child(BODY.in_ink(Ink::Muted).label(tokens, label.to_string()).max_lines(1)),
        )
        .child(rect().tw(tokens, &[w_flex()]).child(control))
}

fn switch_row(tokens: &crate::theme::Tokens, label: &str, on: bool, on_toggle: EventHandler<bool>) -> Rect {
    rect()
        .tw(tokens, &[flex_row(), grow_children(), items(Align::Center), gap(S3), w_full(), h(Dim::Control)])
        .child(
            rect()
                .tw(tokens, &[w_flex()])
                .child(BODY.in_ink(Ink::Text).label(tokens, label.to_string()).max_lines(1)),
        )
        .child(switch(on, on_toggle))
}

impl Component for FiltersForm {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut find = self.find;
        let mut include_noise = self.include_noise;
        let filters = find.read().filters.clone();
        let today = Clock::system().today();
        let today_moment = Moment::from_date(today);

        // A range stays open while its fields are empty or half typed.
        let mut custom_when = use_state(|| modified_key(filters.modified) == CUSTOM);
        let mut custom_size = use_state(|| size_key(filters.size) == CUSTOM);
        let (from0, to0) = modified_fields(filters.modified, today_moment);
        let (min0, max0) = size_fields(filters.size);
        let from = use_state(|| from0.clone());
        let to = use_state(|| to0.clone());
        let min = use_state(|| min0.clone());
        let max = use_state(|| max0.clone());
        let mut bad_when = use_state(|| false);
        let mut bad_size = use_state(|| false);
        let when_open = custom_when() || modified_key(filters.modified) == CUSTOM;
        let size_open = custom_size() || size_key(filters.size) == CUSTOM;

        let kind = dropdown(
            kind_options(&filters),
            kind_key(&filters),
            move |key: String| set_kind(&mut find.write().filters, &key),
        )
        .title("Type")
        .width(Dim::Menu);

        let modified = dropdown(
            options_of(MODIFIED_PRESETS),
            if when_open { CUSTOM } else { modified_key(filters.modified) },
            move |key: String| {
                if key == CUSTOM {
                    custom_when.set(true);
                    return;
                }
                custom_when.set(false);
                bad_when.set(false);
                find.write().filters.modified = modified_preset(&key);
            },
        )
        .title("Modified")
        .width(Dim::Menu);

        let range_field = |value: State<String>, placeholder: &str, apply: EventHandler<()>, bad: bool| {
            let on_submit = apply.clone();
            text_field(value, move |_: String| on_submit.call(()))
                .placeholder(placeholder.to_string())
                .width(Dim::Menu)
                .danger_outline(bad)
                .on_change(move |_: String| apply.call(()))
        };
        let apply_when = EventHandler::new(move |_: ()| match modified_from_fields(&from.peek(), &to.peek()) {
            Ok(next) => {
                bad_when.set(false);
                find.write().filters.modified = next;
            }
            Err(()) => bad_when.set(true),
        });
        let size = dropdown(
            options_of(SIZE_PRESETS),
            if size_open { CUSTOM } else { size_key(filters.size) },
            move |key: String| {
                if key == CUSTOM {
                    custom_size.set(true);
                    return;
                }
                custom_size.set(false);
                bad_size.set(false);
                find.write().filters.size = size_preset(&key);
            },
        )
        .title("Size")
        .width(Dim::Menu);
        let apply_size = EventHandler::new(move |_: ()| match size_from_fields(&min.peek(), &max.peek()) {
            Ok(next) => {
                bad_size.set(false);
                find.write().filters.size = next;
            }
            Err(()) => bad_size.set(true),
        });

        let name_key = NAME_MODES
            .iter()
            .find(|(_, _, mode)| *mode == filters.name_mode)
            .map_or("contains", |(key, _, _)| key);
        let name = dropdown(
            NAME_MODES.iter().map(|(key, label, _)| (key.to_string(), label.to_string())).collect(),
            name_key,
            move |key: String| {
                if let Some((_, _, mode)) = NAME_MODES.iter().find(|(known, _, _)| *known == key) {
                    find.write().filters.name_mode = *mode;
                }
            },
        )
        .title("Name")
        .width(Dim::Menu);

        let sort_key = SORTS
            .iter()
            .find(|(_, _, by)| *by == filters.sort)
            .map_or("relevance", |(key, _, _)| key);
        let sort = dropdown(
            SORTS.iter().map(|(key, label, _)| (key.to_string(), label.to_string())).collect(),
            sort_key,
            move |key: String| {
                if let Some((_, _, by)) = SORTS.iter().find(|(known, _, _)| *known == key) {
                    find.write().filters.sort = *by;
                }
            },
        )
        .title("Sort by")
        .width(Dim::Menu);

        let hidden_on = matches!(filters.hidden, Hidden::Include | Hidden::Only);
        let reset = {
            let tokens_changed = filters.count() > 0 || when_open || size_open;
            action_button("Reset", move |_: ()| {
                find.write().filters.clear_all();
                custom_when.set(false);
                custom_size.set(false);
                bad_when.set(false);
                bad_size.set(false);
            })
            .flat()
            .enabled(tokens_changed)
        };
        let done = action_button("Done", move |_: ()| find.write().filters_open = false).suggested();

        rect()
            .component("FiltersForm")
            .tw(&tokens, &[flex_col(), w_full(), gap(S2)])
            .child(form_row(&tokens, "Type", kind))
            .child(form_row(&tokens, "Modified", modified))
            .maybe_child(when_open.then(|| {
                rect()
                    .tw(&tokens, &[flex_col(), w_full(), gap(S2)])
                    .child(form_row(&tokens, "From", range_field(from, "YYYY-MM-DD", apply_when.clone(), bad_when())))
                    .child(form_row(&tokens, "To", range_field(to, "YYYY-MM-DD", apply_when.clone(), bad_when())))
            }))
            .child(form_row(&tokens, "Size", size))
            .maybe_child(size_open.then(|| {
                rect()
                    .tw(&tokens, &[flex_col(), w_full(), gap(S2)])
                    .child(form_row(
                        &tokens,
                        "At least",
                        range_field(min, "No minimum", apply_size.clone(), bad_size()).suffix("MB"),
                    ))
                    .child(form_row(
                        &tokens,
                        "At most",
                        range_field(max, "No limit", apply_size.clone(), bad_size()).suffix("MB"),
                    ))
            }))
            .child(form_row(&tokens, "Name", name))
            .child(form_row(&tokens, "Sort by", sort))
            .child(switch_row(
                &tokens,
                "Hidden files",
                hidden_on,
                EventHandler::new(move |on: bool| {
                    find.write().filters.hidden = if on { Hidden::Include } else { Hidden::Follow };
                }),
            ))
            .child(switch_row(
                &tokens,
                "Match case",
                filters.case_sensitive,
                EventHandler::new(move |on: bool| find.write().filters.case_sensitive = on),
            ))
            .child(switch_row(
                &tokens,
                "Build and dependency folders",
                *include_noise.read(),
                EventHandler::new(move |on: bool| include_noise.set(on)),
            ))
            .child(
                rect()
                    .tw(&tokens, &[flex_row(), items(Align::Center), justify(Align::End), gap(S2), w_full(), h(Dim::Control)])
                    .child(reset)
                    .child(done),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_and_its_key_name_each_other() {
        for (key, _) in MODIFIED_PRESETS {
            if key != ANY && key != CUSTOM {
                assert_eq!(modified_key(modified_preset(key)), key);
            }
        }
        assert_eq!(modified_key(None), ANY);
        assert_eq!(modified_key(Some(Modified::Yesterday)), CUSTOM);
        assert_eq!(modified_key(Some(Modified::Days(8))), CUSTOM);
        for (key, _) in SIZE_PRESETS {
            if key != ANY && key != CUSTOM {
                assert_eq!(size_key(size_preset(key)), key);
            }
        }
        assert_eq!(size_key(Some(SizeCond::Gt(5))), CUSTOM);
    }

    #[test]
    fn the_type_row_takes_one_kind_and_shows_several() {
        let mut filters = Filters::default();
        assert_eq!(kind_key(&filters), ANY);
        set_kind(&mut filters, "image");
        assert_eq!(filters.kinds, [Kind::Image]);
        assert_eq!(kind_key(&filters), "image");
        filters.kinds = vec![Kind::Image, Kind::Video];
        assert_eq!(kind_key(&filters), MULTI);
        let options = kind_options(&filters);
        assert_eq!(options.last().map(|(_, label)| label.as_str()), Some("Images, Video"));
        set_kind(&mut filters, MULTI);
        assert_eq!(filters.kinds.len(), 2, "the entry that shows several is not a choice");
        set_kind(&mut filters, ANY);
        assert!(filters.kinds.is_empty());
        assert_eq!(kind_options(&filters).len(), 8);
    }

    #[test]
    fn size_fields_read_in_megabytes() {
        assert_eq!(size_from_fields("", ""), Ok(None));
        assert_eq!(size_from_fields("1", ""), Ok(Some(SizeCond::Ge(MB))));
        assert_eq!(size_from_fields("", "0.5"), Ok(Some(SizeCond::Le(500_000))));
        assert_eq!(size_from_fields("1", "2.5"), Ok(Some(SizeCond::Between(MB, 2_500_000))));
        assert_eq!(size_from_fields("3", "2"), Err(()));
        assert_eq!(size_from_fields("big", ""), Err(()));
        assert_eq!(size_from_fields("-1", ""), Err(()));
        assert_eq!(size_fields(Some(SizeCond::Between(MB, 2_500_000))), ("1".into(), "2.5".into()));
        assert_eq!(size_fields(Some(SizeCond::Gt(100 * MB))), ("100".into(), String::new()));
        assert_eq!(size_fields(None), (String::new(), String::new()));
        assert_eq!(mb_text(Some(1_500_000)), "1.5");
        assert_eq!(mb_text(Some(0)), "0");
    }

    #[test]
    fn date_fields_read_as_days_with_the_last_day_included() {
        let day = |text| parse_day(text).unwrap();
        assert_eq!(modified_from_fields("", ""), Ok(None));
        assert_eq!(modified_from_fields("2024-01-01", ""), Ok(Some(Modified::Since(day("2024-01-01")))));
        assert_eq!(modified_from_fields("", "2024-02-28"), Ok(Some(Modified::Before(day("2024-02-29")))));
        assert_eq!(
            modified_from_fields("2024-01-01", "2024-02-28"),
            Ok(Some(Modified::Between(day("2024-01-01"), day("2024-02-28"))))
        );
        assert_eq!(modified_from_fields("2024-03-01", "2024-02-28"), Err(()));
        assert_eq!(modified_from_fields("2024-02", ""), Err(()), "a month is not a day");
        assert_eq!(modified_from_fields("soon", ""), Err(()));
        // Back into the fields.
        let today = day("2024-05-17");
        assert_eq!(
            modified_fields(Some(Modified::Between(day("2024-01-01"), day("2024-02-28"))), today),
            ("2024-01-01".into(), "2024-02-28".into())
        );
        assert_eq!(modified_fields(Some(Modified::Before(day("2024-02-29"))), today), (String::new(), "2024-02-28".into()));
        assert_eq!(
            modified_fields(Some(Modified::On(Moment::parse("2024-02").unwrap())), today),
            ("2024-02-01".into(), "2024-02-29".into())
        );
        assert_eq!(modified_fields(Some(Modified::Days(2)), today), ("2024-05-15".into(), String::new()));
    }

    #[test]
    fn a_range_adds_two_rows_each() {
        assert_eq!(row_count(false, false), 8);
        assert_eq!(row_count(true, false), 10);
        assert_eq!(row_count(true, true), 12);
    }
}
