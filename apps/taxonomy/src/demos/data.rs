//! Data: charts, trees, meters, bars and tiles.

use std::collections::HashSet;
use std::rc::Rc;

use bingux_ui::charts::{
    Bar, FlameTree, Segment, Series, SeriesColour, Unit, bar_chart, donut, flame_graph, line_chart,
    ring, sparkline, timeline_chart,
};
use bingux_ui::controls::TreeNode;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::rows;
use crate::model::Spec;

/// A size in gibibytes, as the byte count the bytes unit expects.
fn gib(value: f64) -> f64 {
    value * 1024.0 * 1024.0 * 1024.0
}

/// Hourly values over 24 hours: a smooth wave between `low` and `high`.
fn day_wave(phase: f64, low: f64, high: f64) -> Vec<(f64, f64)> {
    (0..=24)
        .map(|hour| {
            let wave = (hour as f64 / 24.0 * std::f64::consts::TAU + phase).sin();
            (
                hour as f64 * 3600.0,
                low + (high - low) * (0.5 + 0.5 * wave),
            )
        })
        .collect()
}

/// Traffic per hour over a day: quiet at night, busier later.
fn hourly(phase: f64, peak: f64) -> Vec<f64> {
    (0..24)
        .map(|hour| {
            let day = ((f64::from(hour) - 6.0) / 24.0 * std::f64::consts::TAU + phase).sin();
            (peak * (0.55 + 0.45 * day) * if hour < 5 { 0.2 } else { 1.0 }).max(0.0)
        })
        .collect()
}

/// A small call tree for the flame graph.
fn demo_tree() -> FlameTree {
    let mut tree = FlameTree::new("All");
    let render = tree.push(0, "render", 70.0);
    let layout = tree.push(render, "layout", 30.0);
    tree.push(layout, "measure text", 18.0);
    tree.push(render, "paint", 28.0);
    let input = tree.push(0, "input", 18.0);
    tree.push(input, "dispatch", 12.0);
    tree.push(0, "idle", 12.0);
    tree.add(0, 100.0);
    tree.sorted()
}

/// A bordered card holding one chart, at full width.
fn card(chart: impl IntoElement) -> Element {
    let tokens = use_tokens();
    boxed(&tokens).child(chart).into_element()
}

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub const LINE_CHART: &[Spec] = &[
    Spec::live("One series with an area (hover for values)", || {
        card(
            line_chart([Series::new("CPU", day_wave(0.0, 8.0, 92.0))])
                .y_unit(Unit::Percent)
                .x_unit(Unit::Clock { offset: 0 })
                .area()
                .label("CPU use over 24 hours"),
        )
    })
    .wide(),
    Spec::live("Two series", || {
        card(
            line_chart([
                Series::new("Memory", day_wave(1.0, 30.0, 70.0)),
                Series::new("Swap", day_wave(2.5, 0.0, 12.0)),
            ])
            .y_unit(Unit::Percent)
            .x_unit(Unit::Clock { offset: 0 })
            .label("Memory and swap"),
        )
    })
    .wide(),
    Spec::forced("Empty", || {
        card(
            line_chart([Series::new("Empty", [])])
                .height(Dim::Chart)
                .label("Empty"),
        )
    }),
    Spec::forced("One point", || {
        card(
            line_chart([Series::new("One point", [(0.0, 5.0)])])
                .height(Dim::Chart)
                .label("One point"),
        )
    }),
    Spec::forced("Flat line", || {
        card(
            line_chart([Series::values("Flat", [40.0, 40.0, 40.0])])
                .height(Dim::Chart)
                .label("Flat"),
        )
    }),
    Spec::forced("Gaps (NaN)", || {
        card(
            line_chart([Series::values("Gaps", [1.0, f64::NAN, 3.0, 2.0])])
                .height(Dim::Chart)
                .label("Gaps"),
        )
    }),
];

pub const BAR_CHART: &[Spec] = &[
    Spec::live("Grouped", || {
        card(
            bar_chart([
                Series::values("Reads", [42.0, 55.0, 38.0, 61.0, 70.0, 22.0, 18.0]),
                Series::values("Writes", [12.0, 25.0, 31.0, 18.0, 40.0, 9.0, 6.0]),
            ])
            .categories(DAYS)
            .label("Reads and writes per day"),
        )
    })
    .wide(),
    Spec::live("Stacked", || {
        card(
            bar_chart([
                Series::values("Apps", [4.0, 6.0, 5.0, 8.0, 7.0, 3.0, 2.0]),
                Series::values("System", [2.0, 1.0, 3.0, 2.0, 2.0, 1.0, 1.0]),
            ])
            .categories(DAYS)
            .stacked()
            .label("Stacked hours by category"),
        )
    })
    .wide(),
    Spec::live("Bytes per hour, stacked", || {
        card(
            bar_chart([
                Series::values("Received", hourly(1.0, 22.0e9)),
                Series::values("Sent", hourly(2.0, 9.0e9)),
            ])
            .categories((0..24).map(|hour| format!("{hour:02}:00")))
            .details((0..24).map(|hour| format!("{hour:02}:00 - {:02}:00", (hour + 1) % 24)))
            .y_unit(Unit::DecimalBytes)
            .stacked()
            .label("Data received and sent per hour"),
        )
    })
    .wide(),
];

/// A card `width` px wide, as a phone column is, holding `chart`.
fn column(width: f32, chart: impl IntoElement) -> Element {
    rect()
        .width(Size::px(width))
        .child(card(chart))
        .into_element()
}

/// Storage by category, with names long enough to wrap on a phone.
fn long_storage() -> bingux_ui::charts::Donut {
    donut([
        Segment::new("Applications and games", gib(812.0)),
        Segment::new("Documents, downloads and desktop", gib(1204.0)),
        Segment::new("Operating system", gib(95.0)),
    ])
    .unit(Unit::Bytes)
    .caption("Used of 2 TB")
    .label("Storage used by category")
}

pub const DONUT: &[Spec] = &[
    Spec::live("Donut", || {
        card(
            donut([
                Segment::new("Apps", gib(210.0)),
                Segment::new("System", gib(90.0)),
                Segment::new("Cache", gib(45.0)),
            ])
            .unit(Unit::Bytes)
            .caption("Used")
            .label("Storage used by category"),
        )
    }),
    // The ring is a square as wide as the card up to 200 px, centred, with
    // the legend under it; hover or tap a segment or a row to read it.
    Spec::live("Donut in a 360 px column", || column(360.0, long_storage())).wide(),
    Spec::live("Donut in a 320 px column", || {
        column(
            320.0,
            donut(
                [
                    "Apps", "System", "Cache", "Photos", "Music", "Video", "Other",
                ]
                .into_iter()
                .zip([210.0, 90.0, 45.0, 30.0, 22.0, 18.0, 5.0])
                .map(|(name, value)| Segment::new(name, gib(value))),
            )
            .unit(Unit::Bytes)
            .caption("Used")
            .label("Storage used by category"),
        )
    })
    .wide(),
    Spec::forced("Donut with no data", || {
        column(
            320.0,
            donut(Vec::<Segment>::new()).label("Storage used by category"),
        )
    })
    .wide(),
    Spec::forced("Ring", || {
        card(
            ring(0.62)
                .caption("Charged")
                .colour(SeriesColour::Success)
                .label("Battery"),
        )
    }),
];

pub const SPARKLINE: &[Spec] = &[Spec::forced("With and without an area", || {
    card(
        hstack(S6)
            .child(
                sparkline(day_wave(0.5, 20.0, 80.0).iter().map(|point| point.1))
                    .area()
                    .label("Requests"),
            )
            .child(sparkline([3.0, 5.0, 4.0, 8.0, 6.0, 9.0, 7.0]).label("Errors")),
    )
})];

pub const TIMELINE: &[Spec] = &[
    Spec::live("Frame times with a budget line", || {
        card(
            timeline_chart((0..90).map(|frame| {
                let at = f64::from(frame) * 0.13;
                let slow = frame % 17 == 0;
                Bar::new(
                    at,
                    0.012,
                    if slow {
                        31.0
                    } else {
                        9.0 + f64::from(frame % 5)
                    },
                )
            }))
            .x_range(0.0, 12.0)
            .x_unit(Unit::Suffix("s"))
            .y_unit(Unit::Suffix("ms"))
            .limit(16.7, "Budget")
            .value_name("Frame")
            .label("Frame times"),
        )
    })
    .wide(),
    Spec::live("Flame graph", || {
        let tokens = use_tokens();
        card(
            rect()
                .tw(&tokens, &[h(Dim::ArrangementHeight)])
                .child(flame_graph(Rc::new(demo_tree())).label("Call stacks")),
        )
    })
    .wide(),
];

/// The height of a tree's frame: eleven rows at the phone's 52 px and the
/// frame's padding (596 px), so no row is cut by the frame. The tree fills it.
const TREE_HEIGHT: f32 = 600.0;

/// The frame of a tree that is all closed: its one root row and some air.
const TREE_COLLAPSED_HEIGHT: f32 = 120.0;

/// Five levels, a branch that is closed and a leaf under it, and a root with
/// an icon, a detail and a badge.
fn tree_nodes() -> Vec<TreeNode> {
    vec![
        TreeNode::new(1, "Window")
            .detail("1100 x 820")
            .icon(Icon::Computer)
            .badge("root", Paint::Accent)
            .children(vec![
                TreeNode::new(2, "Column").children(vec![
                    TreeNode::new(3, "Label").detail("Wi-Fi"),
                    TreeNode::new(4, "Label").detail("Bluetooth"),
                ]),
                TreeNode::new(5, "ScrollView")
                    .detail("720 x 700")
                    .children(vec![TreeNode::new(6, "Group").children(vec![
                        TreeNode::new(7, "Row").children(vec![
                            TreeNode::new(8, "Label").detail("Network"),
                            TreeNode::new(9, "Label").detail("Details"),
                        ]),
                        TreeNode::new(10, "Row")
                            .children(vec![TreeNode::new(11, "Label").detail("Hidden")]),
                    ])]),
                TreeNode::new(12, "Footer").children(vec![TreeNode::new(13, "Label")]),
            ]),
    ]
}

/// A chain `levels` deep, one leaf at the end, for the least room a label gets.
fn deep_nodes(levels: u64) -> Vec<TreeNode> {
    let mut node = TreeNode::new(levels, "Label").detail("name");
    for id in (0..levels).rev() {
        node = TreeNode::new(id, "Box")
            .detail(format!("level {id}"))
            .children(vec![node]);
    }
    vec![node]
}

pub const TREE: &[Spec] = &[
    Spec::live("Expand, select, hover", || {
        // Branches 10 and 12 stay closed; row 8, four levels down, is
        // selected, and the script hovers the Bluetooth row.
        let expanded = use_state(|| HashSet::from([1_u64, 2, 5, 6, 7]));
        let selected = use_state(|| Some(8_u64));
        tree_view(tree_nodes(), expanded, selected).into_element()
    })
    .page(TREE_HEIGHT)
    .script("wait 300; hover-text Bluetooth; wait 300; save {out}"),
    Spec::forced("All collapsed", || {
        let expanded = use_state(HashSet::new);
        let selected = use_state(|| None);
        tree_view(tree_nodes(), expanded, selected).into_element()
    })
    .page(TREE_COLLAPSED_HEIGHT),
    Spec::forced("Ten levels deep", || {
        let expanded = use_state(|| (0_u64..10).collect::<HashSet<_>>());
        let selected = use_state(|| Some(10_u64));
        tree_view(deep_nodes(10), expanded, selected).into_element()
    })
    .page(TREE_HEIGHT),
];

pub const METER: &[Spec] = &[
    Spec::forced("Usage meter", || {
        usage_meter("35", "GB used", "256 GB total", 35.0 / 256.0).into_element()
    }),
    Spec::forced("Usage meter, nearly full", || {
        usage_meter("240", "GB used", "256 GB total", 240.0 / 256.0).into_element()
    }),
    Spec::forced("Usage bars: empty, half, warning", || {
        vstack(S3)
            .child(usage_bar(None))
            .child(usage_bar(0.5))
            .child(usage_bar(0.93))
            .into_element()
    }),
    Spec::forced("Stat tiles", || {
        hstack(S3)
            .child(stat_tile("42", "Devices").icon(Icon::Computer))
            .child(
                stat_tile("3", "Warnings")
                    .icon(Icon::DialogWarning)
                    .tone(Tone::Warning),
            )
            .child(
                stat_tile("1", "Error")
                    .icon(Icon::DialogError)
                    .tone(Tone::Danger),
            )
            .into_element()
    }),
];

pub const LEVEL_ROW: &[Spec] = &[
    Spec::live("Level row: slider with a mark and a danger zone", || {
        let mut level = use_state(|| 40.0_f64);
        rows(vec![
            level_row("Brightness", level(), move |next| level.set(next))
                .subtitle("Dims the whole display")
                .mark(75.0)
                .danger_from(90.0)
                .into_element(),
        ])
    }),
    Spec::forced("Disabled", || {
        rows(vec![
            level_row("Volume", 60.0, |_| {})
                .enabled(false)
                .into_element(),
        ])
    }),
    Spec::live("Slider row", || {
        let mut volume = use_state(|| 62.0_f64);
        rows(vec![
            slider_row("Volume", volume(), move |next| volume.set(next)).into_element(),
        ])
    }),
];

pub const ROW_ACTION: &[Spec] = &[
    Spec::live("Row action", || {
        rows(vec![
            row("Saved network")
                .trailing(row_action(Icon::Delete, "Forget", |_| {}))
                .into_element(),
        ])
    }),
    Spec::forced("Property rows", || {
        rows(vec![
            property_row("Kernel", "6.12.0-100.fc43.x86_64").into_element(),
            property_row("Hostname", "bingux-workstation").into_element(),
        ])
    }),
];

pub const FADER: &[Spec] = &[Spec::forced("Faders: empty, half, full, disabled", || {
    let low = use_state(|| 0.0_f64);
    let mid = use_state(|| 50.0_f64);
    let high = use_state(|| 100.0_f64);
    let off = use_state(|| 30.0_f64);
    let bank = |state: State<f64>, label: &'static str, enabled: bool| {
        let mut state = state;
        fader(*state.read(), move |value: f64| state.set(value))
            .height(140.0)
            .label(label)
            .enabled(enabled)
    };
    hstack(S6)
        .child(bank(low, "Low", true))
        .child(bank(mid, "Middle", true))
        .child(bank(high, "High", true))
        .child(bank(off, "Disabled", false))
        .into_element()
})];

pub const LEVEL_METER: &[Spec] = &[Spec::forced(
    "Level meter: silent, quiet, loud, clipping",
    || {
        vstack(S3)
            .child(level_meter(0.0).label("Silent"))
            .child(level_meter(0.35).label("Quiet"))
            .child(level_meter(0.8).label("Loud"))
            .child(level_meter(0.97).label("Clipping"))
            .into_element()
    },
)];
