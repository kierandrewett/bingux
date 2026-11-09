//! Data: the calendar views and the date and time pickers.

use bingux_ui::calendar::layout::Span;
use bingux_ui::calendar::palette::parse_hex;
use bingux_ui::calendar::{
    Banner, Block, Date, MonthItem, Weekday, date_picker, month_grid, month_view, time_grid,
    time_picker,
};
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::with;
use crate::model::Spec;

/// The day the specimens treat as today: a Wednesday.
fn today() -> Date {
    Date::new(2026, 11, 11).expect("a real date")
}

fn colour(hex: &str) -> Color {
    parse_hex(hex).expect("a colour")
}

fn block(
    id: &str,
    column: usize,
    from: u32,
    to: u32,
    title: &str,
    detail: &str,
    hex: &str,
) -> Block {
    Block {
        id: id.to_string(),
        column,
        span: Span {
            start: from,
            end: to,
        },
        title: title.to_string(),
        detail: detail.to_string(),
        colour: colour(hex),
        tentative: false,
        editable: true,
        clipped_start: false,
        clipped_end: false,
    }
}

const BLUE: &str = "#3584e4";
const GREEN: &str = "#5a9a53";
const PINK: &str = "#d5619a";

/// A week-like day set: Monday to Wednesday of today's week.
fn three_days() -> Vec<Date> {
    let first = today().week_start(Weekday::Mon);
    (0..3).map(|offset| first.add_days(offset)).collect()
}

fn blocks() -> Vec<Block> {
    vec![
        block("a", 0, 9 * 60 + 30, 9 * 60 + 45, "Stand-up", "09:30", GREEN),
        block(
            "b",
            1,
            10 * 60,
            11 * 60 + 30,
            "Design review",
            "10:00 to 11:30",
            GREEN,
        ),
        block(
            "c",
            1,
            10 * 60 + 15,
            12 * 60,
            "Roadmap",
            "10:15 to 12:00",
            BLUE,
        ),
        block(
            "d",
            1,
            10 * 60 + 30,
            11 * 60,
            "1:1 with Sam",
            "10:30 to 11:00",
            PINK,
        ),
        block(
            "e",
            2,
            14 * 60,
            16 * 60,
            "Planning",
            "14:00 to 16:00 · Room 2",
            BLUE,
        ),
    ]
}

pub const TIME_GRID: &[Spec] = &[
    Spec::live(
        "Three days: overlapping events share the width; press and drag over empty time to make one",
        || {
            let tokens = use_tokens();
            let mut selected = use_state(|| None::<String>);
            rect()
                .tw(&tokens, &[w_full(), h_full()])
                .child(
                    time_grid(three_days(), today())
                        .blocks(blocks())
                        .banners(vec![Banner {
                            id: "off".to_string(),
                            first: 1,
                            last: 2,
                            title: "Team offsite".to_string(),
                            colour: colour(PINK),
                            before: false,
                            after: false,
                        }])
                        .now(11 * 60 + 20)
                        .scroll_to(8 * 60, 1)
                        .selected(selected.read().clone())
                        .on_select(move |id: String| selected.set(Some(id))),
                )
                .into_element()
        },
    )
    .page(420.0),
    Spec::forced("Twelve-hour clock, a tentative and a read-only event", || {
        let tokens = use_tokens();
        let mut tentative = block("t", 0, 13 * 60, 14 * 60, "Lunch?", "1:00 to 2:00 PM", BLUE);
        tentative.tentative = true;
        let mut fixed = block("r", 1, 15 * 60, 16 * 60, "Holiday", "3:00 to 4:00 PM", GREEN);
        fixed.editable = false;
        rect()
            .tw(&tokens, &[w_full(), h_full()])
            .child(
                time_grid(three_days(), today())
                    .blocks(vec![tentative, fixed])
                    .twelve_hour(true)
                    .scroll_to(12 * 60, 1),
            )
            .into_element()
    })
    .page(300.0),
];

fn month_items() -> Vec<MonthItem> {
    let day = |d: u8| Date::new(2026, 11, d).expect("a real date");
    let item = |id: &str,
                first: Date,
                last: Date,
                title: &str,
                hex: &str,
                bar: bool,
                time: &str,
                minutes: u32| {
        MonthItem {
            id: id.to_string(),
            first,
            last,
            title: title.to_string(),
            colour: colour(hex),
            bar,
            time: time.to_string(),
            minutes,
        }
    };
    vec![
        item("1", day(9), day(9), "Stand-up", GREEN, false, "09:30", 570),
        item(
            "2",
            day(11),
            day(11),
            "Design review",
            GREEN,
            false,
            "10:00",
            600,
        ),
        item("3", day(11), day(11), "Dentist", BLUE, false, "16:00", 960),
        item("4", day(12), day(14), "Offsite", PINK, true, "", 0),
        item(
            "5",
            day(16),
            day(20),
            "Holiday in Lisbon",
            BLUE,
            true,
            "",
            0,
        ),
        item(
            "6",
            day(18),
            day(18),
            "Dinner out",
            PINK,
            false,
            "19:00",
            1140,
        ),
        item("7", day(18), day(18), "Gym", BLUE, false, "18:00", 1080),
        item("8", day(18), day(18), "Call", GREEN, false, "17:00", 1020),
        item(
            "9",
            day(18),
            day(18),
            "Parents’ evening",
            GREEN,
            false,
            "20:00",
            1200,
        ),
    ]
}

pub const MONTH_VIEW: &[Spec] = &[Spec::live(
    "A month: bars span days, a full day says how many more (arrows move, Enter chooses an event)",
    || {
        let tokens = use_tokens();
        let mut day = use_state(today);
        let mut picked = use_state(|| None::<String>);
        rect()
            .tw(&tokens, &[w_full(), h_full()])
            .child(
                month_view(day(), today(), move |chosen: Date| day.set(chosen))
                    .selected(day())
                    .items(month_items())
                    .selected_item(picked.read().clone())
                    .on_pick(move |id: Option<String>| picked.set(id)),
            )
            .into_element()
    },
)
.page(480.0)];

pub const MONTH_GRID: &[Spec] = &[
    Spec::live("Days with events are marked; arrows page the month", || {
        let mut day = use_state(today);
        let marked = vec![
            Date::new(2026, 11, 9).expect("a real date"),
            Date::new(2026, 11, 18).expect("a real date"),
        ];
        month_grid(day(), today(), move |chosen: Date| day.set(chosen))
            .selected(day())
            .marked(marked)
            .on_page(move |delta: i32| day.set(day().add_months(delta)))
            .into_element()
    }),
    Spec::forced("Week numbers, weeks start on Sunday", || {
        month_grid(today(), today(), |_: Date| {})
            .first_weekday(Weekday::Sun)
            .week_numbers(true)
            .selected(today())
            .into_element()
    }),
    Spec::forced(
        "Without the header and the neighbouring months' days",
        || {
            month_grid(today(), today(), |_: Date| {})
                .bare()
                .inside_only()
                .into_element()
        },
    ),
];

pub const PICKERS: &[Spec] = &[
    Spec::live("A date and a time", || {
        let mut date = use_state(today);
        let mut time = use_state(|| 9 * 60 + 30);
        let mut end = use_state(|| 10 * 60 + 30);
        rows_of(vec![
            with(
                "Date",
                date_picker(date(), today(), move |chosen: Date| date.set(chosen)),
            ),
            with(
                "Starts",
                time_picker(time(), false, move |minutes: u32| time.set(minutes)),
            ),
            with(
                "Ends, 12-hour, with the length",
                time_picker(end(), true, move |minutes: u32| end.set(minutes)).after(time()),
            ),
        ])
    }),
    Spec::forced("Disabled", || {
        rows_of(vec![
            with(
                "Date",
                date_picker(today(), today(), |_: Date| {}).enabled(false),
            ),
            with(
                "Time",
                time_picker(9 * 60, false, |_: u32| {}).enabled(false),
            ),
        ])
    }),
];

fn rows_of(rows: Vec<Element>) -> Element {
    super::common::rows(rows)
}
