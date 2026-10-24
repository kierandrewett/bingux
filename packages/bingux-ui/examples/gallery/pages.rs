//! Gallery pages shared by the windowed gallery and the snapshot example.

use bingux_ui::charts::{
    Segment, Series, SeriesColour, Unit, bar_chart, donut, line_chart, ring, sparkline,
};
use bingux_ui::prelude::*;
use bingux_ui::style::Align;
use freya::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Controls,
    Buttons,
    Rows,
    Feedback,
    Charts,
    Typography,
    Icons,
}

impl Page {
    pub const ALL: [Page; 7] = [
        Page::Controls,
        Page::Buttons,
        Page::Rows,
        Page::Feedback,
        Page::Charts,
        Page::Typography,
        Page::Icons,
    ];
}

impl NavItem for Page {
    fn title(self) -> &'static str {
        match self {
            Page::Controls => "Controls",
            Page::Buttons => "Buttons",
            Page::Rows => "Rows and groups",
            Page::Feedback => "Feedback",
            Page::Charts => "Charts",
            Page::Typography => "Typography",
            Page::Icons => "Icons",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::Controls => Icon::PreferencesSystem,
            Page::Buttons => Icon::Check,
            Page::Rows => Icon::Display,
            Page::Feedback => Icon::Refresh,
            Page::Charts => Icon::Computer,
            Page::Typography => Icon::About,
            Page::Icons => Icon::Appearance,
        }
    }
}

const SAMPLE_PANEL_HEIGHT: f32 = 320.0;
const SAMPLE_TILE_WIDTH: f32 = 132.0;
const SAMPLE_TILE_HEIGHT: f32 = 72.0;

#[derive(PartialEq)]
pub struct Gallery {
    pub initial: Page,
}

impl Component for Gallery {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let page = use_state(|| self.initial);
        let mut subpage = use_state(|| false);
        let content: Element = match page() {
            Page::Controls => ControlsPage.into(),
            Page::Buttons => ButtonsPage.into(),
            Page::Rows => RowsPage {
                open: EventHandler::new(move |_: ()| subpage.set(true)),
            }
            .into(),
            Page::Feedback => FeedbackPage.into(),
            Page::Charts => ChartsPage.into(),
            Page::Typography => TypographyPage.into(),
            Page::Icons => IconsPage.into(),
        };
        rect()
            .expanded()
            .tw(&tokens, &[ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(if subpage() && page() == Page::Rows {
                NavigationSplitView::new("Gallery", Page::ALL.to_vec(), page, SubpageSample)
                    .subpage("Subpage", move |_| subpage.set(false))
            } else {
                NavigationSplitView::new("Gallery", Page::ALL.to_vec(), page, content)
            }))
    }
}

#[derive(PartialEq)]
struct ControlsPage;

impl Component for ControlsPage {
    fn render(&self) -> impl IntoElement {
        let mut on = use_state(|| true);
        let mut volume = use_state(|| 62.0_f64);
        let mut stepped = use_state(|| 40.0_f64);
        let mut check = use_state(|| CheckState::On);
        let mut radio = use_state(|| "medium".to_string());
        let mut count = use_state(|| 3_i64);
        let mut segment = use_state(|| "light".to_string());
        let mut language = use_state(|| "en".to_string());
        page_column()
            .child(
                Group::new("Switch".to_string())
                    .row(switch_row("Enabled", on(), move |next| on.set(next)))
                    .row(row("Disabled").trailing(switch(true, |_| {}).enabled(false)))
                    .row(
                        row("Checkbox").trailing(
                            checkbox(check(), move |next: bool| check.set(next.into()))
                                .text("Remember"),
                        ),
                    )
                    .row(row("Mixed checkbox").trailing(checkbox(CheckState::Mixed, |_| {})))
                    .row(row("Disabled checkbox").trailing(checkbox(true, |_| {}).enabled(false))),
            )
            .child(
                Group::new("Slider".to_string())
                    .row(slider_row("Volume", volume(), move |next| volume.set(next)))
                    .row(
                        row("Stepped, with value").trailing(
                            slider(stepped(), move |next| stepped.set(next))
                                .step(10.0)
                                .show_value(),
                        ),
                    )
                    .row(row("Disabled").trailing(slider(30.0, |_| {}).enabled(false))),
            )
            .child(
                Group::new("Choice".to_string())
                    .row(
                        row("Radio group").subtitle("Horizontal layout").trailing(
                            radio_group(
                                vec![
                                    ("small".into(), "Small".into()),
                                    ("medium".into(), "Medium".into()),
                                    ("large".into(), "Large".into()),
                                ],
                                radio.read().clone(),
                                move |key: String| radio.set(key),
                            )
                            .horizontal(),
                        ),
                    )
                    .row(choice_row(
                        "Segmented",
                        vec![
                            ("light".into(), "Light".into()),
                            ("dark".into(), "Dark".into()),
                            ("auto".into(), "Auto".into()),
                        ],
                        &segment.read().clone(),
                        move |key: String| segment.set(key),
                    ))
                    .row(choice_row(
                        "Dropdown",
                        // A long list, to show the capped scrolling menu.
                        [
                            ("en", "English (United Kingdom)"),
                            ("fr", "French (France)"),
                            ("de", "German (Germany)"),
                            ("es", "Spanish (Spain)"),
                            ("it", "Italian (Italy)"),
                            ("pt", "Portuguese (Portugal)"),
                            ("nl", "Dutch (Netherlands)"),
                            ("sv", "Swedish (Sweden)"),
                            ("da", "Danish (Denmark)"),
                            ("nb", "Norwegian (Norway)"),
                            ("fi", "Finnish (Finland)"),
                            ("pl", "Polish (Poland)"),
                            ("cs", "Czech (Czechia)"),
                            ("el", "Greek (Greece)"),
                            ("tr", "Turkish (Turkey)"),
                            ("ja", "Japanese (Japan)"),
                        ]
                        .iter()
                        .map(|(key, name)| (key.to_string(), name.to_string()))
                        .collect(),
                        &language.read().clone(),
                        move |key: String| language.set(key),
                    ))
                    .row(
                        choice_row(
                            "Disabled dropdown",
                            vec![
                                ("a".into(), "First".into()),
                                ("b".into(), "Second".into()),
                                ("c".into(), "Third".into()),
                                ("d".into(), "Fourth".into()),
                            ],
                            "a",
                            |_| {},
                        )
                        .enabled(false),
                    )
                    .row(
                        row("Stepper").trailing(
                            stepper(count(), move |next| count.set(next))
                                .range(0, 10)
                                .suffix(" items"),
                        ),
                    ),
            )
    }
}

#[derive(PartialEq)]
struct ButtonsPage;

impl Component for ButtonsPage {
    fn render(&self) -> impl IntoElement {
        let plain = use_state(String::new);
        let query = use_state(|| "wifi".to_string());
        let secret = use_state(|| "hunter2".to_string());
        let bad = use_state(|| "relative/path".to_string());
        page_column()
            .child(
                Group::new("Menus".to_string())
                    .description("Opens where the pointer is pressed.")
                    .row(row("Menu").trailing(menu_button(
                        "Actions",
                        vec![
                            menu_item("Cut", |_| {}),
                            menu_item("Copy", |_| {}).icon(Icon::EditCopy),
                            menu_item("Paste", |_| {}).disabled(),
                            menu_separator(),
                            menu_item("Show hidden files", |_| {}).checked(true),
                            menu_item("Show preview", |_| {}).checked(false),
                            menu_separator(),
                            menu_item("Delete", |_| {}).destructive(),
                        ],
                    ))),
            )
            .child(
                Group::new("Buttons".to_string())
                    .description("Inside a row a button is 32 px high; on its own it is 36 px.")
                    .row(row("Default").trailing(action_button("Action", |_| {}).compact()))
                    .row(
                        row("With icon")
                            .trailing(action_button("Open", |_| {}).icon(Icon::Folder).compact()),
                    )
                    .row(row("Suggested").trailing(primary_button("Save", |_| {}).compact()))
                    .row(
                        row("Destructive")
                            .trailing(action_button("Delete", |_| {}).destructive().compact()),
                    )
                    .row(row("Flat").trailing(action_button("Skip", |_| {}).flat().compact()))
                    .row(
                        row("Loading")
                            .trailing(action_button("Saving", |_| {}).loading(true).compact()),
                    )
                    .row(
                        row("Disabled")
                            .trailing(action_button("Action", |_| {}).enabled(false).compact()),
                    )
                    .row(row("Row button").trailing(row_button("Change", |_| {}))),
            )
            .child(
                Group::new("Icon buttons".to_string()).row(
                    row("Refresh, delete, menu").trailing(
                        hstack(S1)
                            .child(icon_button(Icon::Refresh, |_| {}).tooltip("Refresh"))
                            .child(icon_button(Icon::Delete, |_| {}).tooltip("Delete").danger())
                            .child(icon_button(Icon::OpenMenu, |_| {}).tooltip("Menu")),
                    ),
                ),
            )
            .child(section_label("Action strip"))
            .child(action_strip(vec![
                strip_action(Icon::Delete, "Forget", |_| {}),
                strip_action(Icon::WindowClose, "Disconnect", |_| {}),
                strip_action(Icon::EmblemSystem, "Settings", |_| {}).enabled(false),
            ]))
            .child(section_label("Usage meter"))
            .child(usage_meter("35", "GB used", "256 GB total", 35.0 / 256.0))
            .child(
                Group::new("Text fields".to_string())
                    .row(row("Plain").trailing(text_field(plain, |_| {}).placeholder("Type here")))
                    .row(row("Search").trailing(search_field(query, |_| {})))
                    .row(row("Password").trailing(text_field(secret, |_| {}).password()))
                    .row(row("Invalid").trailing(
                        text_field(bad, |_| {}).invalid(Some("Enter an absolute path.".into())),
                    ))
                    .row(row("Disabled").trailing(text_field(plain, |_| {}).enabled(false))),
            )
    }
}

/// What a subpage holds: the header shows Back and the title.
#[derive(PartialEq)]
struct SubpageSample;

impl Component for SubpageSample {
    fn render(&self) -> impl IntoElement {
        page_column().child(
            Group::new("Inside a subpage".to_string())
                .description("Back, Alt+Left and the mouse Back button return to the page below.")
                .row(row("Opened from a navigating row"))
                .row(
                    row("Application")
                        .leading_app("org.gnome.Nautilus")
                        .subtitle("A leading application icon"),
                ),
        )
    }
}

#[derive(PartialEq)]
struct RowsPage {
    open: EventHandler<()>,
}

impl Component for RowsPage {
    fn render(&self) -> impl IntoElement {
        let mut name = use_state(|| "Bingux".to_string());
        let mut committed = use_state(|| "Workstation".to_string());
        let sharing = use_state(|| true);
        let mut advanced = use_state(|| false);
        let overlays = use_overlays();
        page_column()
            .child(
                Group::new("Plain rows".to_string())
                    .description("Groups take an optional description.")
                    .row(row("Title only"))
                    .row(row("With subtitle").subtitle("Secondary text in the muted colour"))
                    .row(row("With value").value("Value"))
                    .row(row("Opens a subpage").navigates().on_press({
                        let open = self.open.clone();
                        move |_| open.call(())
                    })),
            )
            .child(
                Group::new("Decorated".to_string())
                    .row(row("Leading icon").leading(Icon::Network))
                    .row(row("Application icon").leading_app("org.gnome.Nautilus"))
                    .row(row("Missing application icon").leading_app("no-such-icon"))
                    .row(row("Badge").badge("3"))
                    .row(row("Selected").selected(true).on_press(|_| {}))
                    .row(row("Destructive").destructive().on_press(|_| {}))
                    .row(row("Disabled").enabled(false)),
            )
            .child(
                Group::new("Editable".to_string())
                    .row(text_row("Name", name, move |next: String| name.set(next)))
                    .row(entry_row(
                        "Entry row",
                        committed.read().clone(),
                        move |next: String| committed.set(next),
                    ))
                    .row(row("Action").trailing(row_button("Change", |_| {}))),
            )
            .child(
                Group::new("Expander and properties".to_string())
                    .row(
                        expander_row(
                            "Advanced",
                            vec![
                                switch_row("Option one", true, |_| {}).into(),
                                switch_row("Option two", false, |_| {}).into(),
                            ],
                        )
                        .subtitle("Press to reveal two rows")
                        .expanded(true),
                    )
                    .row(property_row("Kernel", "6.12.0-100.fc43.x86_64"))
                    .row(property_row(
                        "Processor",
                        "AMD Ryzen 7 5800X 8-Core Processor with a long name that wraps",
                    )),
            )
            .child(
                Group::new("Behaviour".to_string())
                    .description(
                        "Context menus, optimistic changes, nested rows and selectable values."
                            .to_string(),
                    )
                    .row(row("Right-click for a menu").context_menu(vec![
                        menu_item("Rename", |_| {}),
                        menu_item("Duplicate", |_| {}).disabled(),
                        menu_item("Delete", |_| {}).destructive(),
                    ]))
                    .row(switch_row(
                        "Share on network",
                        sharing(),
                        move |next: bool| {
                            bingux_ui::runtime::optimistic(
                                sharing,
                                Some(overlays),
                                !next,
                                next,
                                move || -> Result<(), String> {
                                    if next {
                                        Err("The share service did not start".to_string())
                                    } else {
                                        Ok(())
                                    }
                                },
                            )
                        },
                    ))
                    .row(
                        switch_row("Hardware acceleration", advanced(), move |next: bool| {
                            advanced.set(next)
                        })
                        .nested(
                            advanced(),
                            vec![
                                property_row("Driver", "vulkan-radeon").into(),
                                property_row("Renderer", "llvmpipe").into(),
                            ],
                        ),
                    )
                    .row(property_row("Hostname", "bingux-workstation")),
            )
    }
}

#[derive(PartialEq)]
struct FeedbackPage;

impl Component for FeedbackPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut progress = use_state(|| 0.35_f64);
        let mut notice = use_state(|| true);
        let sheet_name = use_state(String::new);
        let sheet_tab = use_state(|| "details".to_string());
        let sheet_security = use_state(|| "personal".to_string());
        let sheet_password = use_state(String::new);
        let sheet_auto = use_state(|| true);
        let overlays = use_overlays();
        page_column()
        .child(Group::new("Skeleton".to_string()).description("Shown for a page before its data has arrived.".to_string()))
        .child(skeleton())
        .child(
            banner("Updates are ready to install.")
                .visible(notice())
                .action("Install", |_| {})
                .dismissible(move |_| notice.set(false)),
        )
        .child(
            Group::new("Overlays".to_string())
                .row(
                    row("Alert dialog").trailing(action_button("Open", move |_| {
                        overlays.show_dialog(
                            dialog(
                                "Forget this network?",
                                "Reconnecting will need the password again.",
                            )
                            .cancel("Cancel")
                            .response(
                                "Forget",
                                ResponseKind::Destructive,
                                |_| {},
                            ),
                        )
                    })
                    .compact()),
                )
                .row(row("Toast").trailing(action_button("Show", move |_| {
                    overlays.toast("Settings saved")
                })
                .compact()))
                .row(
                    row("Toast with action").trailing(action_button("Show", move |_| {
                        overlays.toast_with_action("Network forgotten", "Undo", |_| {})
                    })
                    .compact()),
                )
                .row(
                    row("Banner").trailing(action_button("Show again", move |_| notice.set(true)).compact()),
                )
                .row(
                    row("Sheet").trailing(action_button("Open", move |_| {
                        overlays.show_sheet(
                            sheet(
                                "Add a network",
                                FormDemoBody {
                                    name: sheet_name,
                                    tab: sheet_tab,
                                    security: sheet_security,
                                    password: sheet_password,
                                    auto_connect: sheet_auto,
                                },
                            )
                            .tabs(FormDemoTabs { tab: sheet_tab })
                            .primary("Save", move |_| overlays.close_sheet())
                            .primary_enabled(!sheet_name.read().trim().is_empty()),
                        )
                    })
                    .compact()),
                )
        )
        .child(
            Group::new("Progress".to_string())
                .row(row("Determinate").trailing(
                    rect().tw(&tokens, &[w(Dim::Field)]).child(progress_bar(progress()).width(Size::fill())),
                ))
                .row(row("Indeterminate").trailing(
                    rect().tw(&tokens, &[w(Dim::Field)]).child(progress_bar(None).width(Size::fill())),
                ))
                .row(row("Step").trailing(action_button("Advance", move |_| {
                    progress.set((progress() + 0.15).min(1.0))
                })
                .compact())),
        )
        .child(
            rect()
                .tw(&tokens, &[flex_row(), items(Align::Center), gap(S6)])
                .child(spinner(SpinnerSize::Small))
                .child(spinner(SpinnerSize::Medium))
                .child(spinner(SpinnerSize::Large).text("Loading")),
        )
        .child(status(Status::Loading("Reading networks.".into())))
        .child(
            status(Status::Error("bluetoothctl is not installed.".into()))
                .action("Try again", |_| {}),
        )
        .child(status(Status::Info("No devices found.".into())))
        .child(
            rect()
                .tw(&tokens, BOX)
                .height(Size::px(SAMPLE_PANEL_HEIGHT))
                .child(
                    status_page(Icon::Bluetooth, "No devices found")
                        .description(
                            "Turn Bluetooth on and make the device discoverable, then try again.",
                        )
                        .action("Scan again", |_| {}),
                ),
        )
    }
}

#[derive(PartialEq)]
struct TypographyPage;

impl Component for TypographyPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ramp: [(TextStyle, &str, &str); 12] = [
            (
                typography::HERO,
                "72%",
                "Hero, 36 light. One figure a page is about.",
            ),
            (
                typography::TITLE,
                "Display settings",
                "Title, 28 semibold. The name of a page or subpage.",
            ),
            (
                typography::HEADING,
                "Network",
                "Group title, 18 semibold. Names a card of rows.",
            ),
            (
                typography::BODY,
                "Wi-Fi password",
                "Row title, 14 normal, primary text.",
            ),
            (
                typography::EMPHASIS,
                "Connected to HomeNet",
                "Emphasis, 14 semibold. A name or state in body text.",
            ),
            (
                typography::VALUE,
                "Off",
                "Value, 14 normal, muted. Read-only text at the right of a row.",
            ),
            (
                typography::NUMERIC,
                "1,024 files",
                "Numeric, 14 normal, tabular figures so changing digits hold still.",
            ),
            (
                typography::CAPTION,
                "Stored on this device",
                "Subtitle and caption, 12 normal, muted.",
            ),
            (typography::BADGE, "New", "Badge, 11 semibold, accent."),
            (
                typography::OVERLINE,
                "Advanced",
                "Overline, 11 semibold, muted, tracked. Small capitals above a block.",
            ),
            (typography::LINK, "Learn more", "Link, 14 medium, accent."),
            (
                typography::BODY.in_ink(Ink::Danger),
                "Remove device",
                "Danger, 14 normal. Errors and destructive text.",
            ),
        ];
        let mut column = page_column()
            .tw(&tokens, &[px(GUTTER)])
            .child(
                Group::new("Network".to_string())
                    .description("Group description, 12 muted, under the group title.")
                    .row(
                        row("Wi-Fi")
                            .subtitle("Connected to HomeNet")
                            .trailing(typography::VALUE.label(&tokens, "On")),
                    )
                    .row(row("Notifications").badge("New"))
                    .row(row("Night light").trailing(typography::VALUE.label(&tokens, "Off"))),
            )
            .child(status(Status::Info("Up to date".to_string())));
        for (style, sample, note) in ramp {
            column = column.child(
                rect()
                    .tw(&tokens, &[w_full(), flex_col(), gap(S1)])
                    .child(style.label(&tokens, sample))
                    .child(caption(note)),
            );
        }
        column
    }
}

#[derive(PartialEq)]
struct IconsPage;

impl Component for IconsPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        page_column()
            .child(
                Group::new("Application icons".to_string())
                    .description("Full-colour icons from the icon theme, as the dock shows them.")
                    .row(
                        row("Sizes").trailing(
                            hstack(S3)
                                .child(app_icon("org.gnome.Nautilus", IconSize::AppLarge))
                                .child(app_icon("org.gnome.Nautilus", IconSize::Hero))
                                .child(app_icon("org.gnome.Nautilus", IconSize::App))
                                .child(app_icon("org.gnome.Nautilus", IconSize::AppSmall)),
                        ),
                    )
                    .row(row("Missing icon").trailing(app_icon("no-such-icon", IconSize::App))),
            )
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), wrap(S2), gap(S2)])
                    .children(Icon::ALL.iter().map(|glyph| {
                        Element::from(
                            rect()
                                .width(Size::px(SAMPLE_TILE_WIDTH))
                                .height(Size::px(SAMPLE_TILE_HEIGHT))
                                .tw(
                                    &tokens,
                                    &[rounded(Radius::Card), bg(Paint::Card), center(), gap(S1)],
                                )
                                .child(icon(*glyph, IconSize::AppSmall, Paint::Text.color(&tokens)))
                                .child(
                                    typography::CAPTION
                                        .label(&tokens, format!("{glyph:?}"))
                                        .max_lines(1),
                                ),
                        )
                    })),
            )
    }
}

/// A size in gibibytes, as the byte count the bytes unit expects.
fn gib(value: f64) -> f64 {
    value * 1024.0 * 1024.0 * 1024.0
}

#[derive(PartialEq)]
struct ChartsPage;

impl Component for ChartsPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        page_column()
            .child(section_label("Trends"))
            .child(chart_card(
                &tokens,
                line_chart([Series::new("CPU", day_wave(0.0, 8.0, 92.0))])
                    .y_unit(Unit::Percent)
                    .x_unit(Unit::Clock { offset: 0 })
                    .area()
                    .label("CPU use over 24 hours"),
            ))
            .child(chart_card(
                &tokens,
                line_chart([
                    Series::new("Memory", day_wave(1.0, 30.0, 70.0)),
                    Series::new("Swap", day_wave(2.5, 0.0, 12.0)),
                ])
                .y_unit(Unit::Percent)
                .x_unit(Unit::Clock { offset: 0 })
                .label("Memory and swap"),
            ))
            .child(section_label("Comparison"))
            .child(chart_card(
                &tokens,
                bar_chart([
                    Series::values("Reads", [42.0, 55.0, 38.0, 61.0, 70.0, 22.0, 18.0]),
                    Series::values("Writes", [12.0, 25.0, 31.0, 18.0, 40.0, 9.0, 6.0]),
                ])
                .categories(days)
                .label("Reads and writes per day"),
            ))
            .child(chart_card(
                &tokens,
                bar_chart([
                    Series::values("Apps", [4.0, 6.0, 5.0, 8.0, 7.0, 3.0, 2.0]),
                    Series::values("System", [2.0, 1.0, 3.0, 2.0, 2.0, 1.0, 1.0]),
                ])
                .categories(days)
                .stacked()
                .label("Stacked hours by category"),
            ))
            .child(section_label("Share"))
            .child(
                hstack(S6)
                    .child(chart_card(
                        &tokens,
                        donut([
                            Segment::new("Apps", gib(210.0)),
                            Segment::new("System", gib(90.0)),
                            Segment::new("Cache", gib(45.0)),
                        ])
                        .unit(Unit::Bytes)
                        .caption("Used")
                        .label("Storage used by category"),
                    ))
                    .child(chart_card(
                        &tokens,
                        ring(0.62)
                            .caption("62%")
                            .colour(SeriesColour::Success)
                            .label("Battery"),
                    )),
            )
            .child(section_label("Sparklines"))
            .child(chart_card(
                &tokens,
                hstack(S6)
                    .child(
                        sparkline(day_wave(0.5, 20.0, 80.0).iter().map(|point| point.1))
                            .area()
                            .label("Requests"),
                    )
                    .child(sparkline([3.0, 5.0, 4.0, 8.0, 6.0, 9.0, 7.0]).label("Errors")),
            ))
            .child(section_label("Edge cases"))
            .child(chart_card(
                &tokens,
                vstack(S4)
                    .child(
                        line_chart([Series::new("Empty", [])])
                            .height(Dim::Chart)
                            .label("Empty"),
                    )
                    .child(
                        line_chart([Series::new("One point", [(0.0, 5.0)])])
                            .height(Dim::Chart)
                            .label("One point"),
                    )
                    .child(
                        line_chart([Series::values("Flat", [40.0, 40.0, 40.0])])
                            .height(Dim::Chart)
                            .label("Flat"),
                    )
                    .child(
                        line_chart([Series::values("Gaps", [1.0, f64::NAN, 3.0, 2.0])])
                            .height(Dim::Chart)
                            .label("Gaps"),
                    ),
            ))
    }
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

/// A bordered card holding one chart, at full width.
fn chart_card(tokens: &Tokens, chart: impl IntoElement) -> Element {
    boxed(tokens).child(chart).into()
}

/// The tab row of the gallery sheet. It reads its selection while rendering.
#[derive(Clone, PartialEq)]
pub struct FormDemoTabs {
    pub tab: State<String>,
}

impl Component for FormDemoTabs {
    fn render(&self) -> impl IntoElement {
        let mut tab = self.tab;
        tabs(
            vec![
                ("details".to_string(), "Details".to_string()),
                ("security".to_string(), "Security".to_string()),
            ],
            tab.read().as_str(),
            move |key: String| tab.set(key),
        )
    }
}

/// The body of the gallery sheet. Each tab shows its own form rows.
#[derive(Clone, PartialEq)]
pub struct FormDemoBody {
    pub name: State<String>,
    pub tab: State<String>,
    pub security: State<String>,
    pub password: State<String>,
    pub auto_connect: State<bool>,
}

impl Component for FormDemoBody {
    fn render(&self) -> impl IntoElement {
        let tab = self.tab;
        let mut security = self.security;
        let mut auto_connect = self.auto_connect;
        if tab.read().as_str() == "security" {
            sheet_column().child(
                Group::new(None::<String>)
                    .row(form_row(
                        "Security",
                        dropdown(
                            vec![
                                ("personal".to_string(), "Personal password".to_string()),
                                ("open".to_string(), "Open, no password".to_string()),
                            ],
                            security.read().clone(),
                            move |key: String| security.set(key),
                        ),
                    ))
                    .row(form_row(
                        "Password",
                        text_field(self.password, |_| {}).password(),
                    )),
            )
        } else {
            sheet_column().child(
                Group::new(None::<String>)
                    .row(form_row(
                        "Network name",
                        text_field(self.name, |_| {}).placeholder("Network name"),
                    ))
                    .row(form_row(
                        "Connect automatically",
                        checkbox(auto_connect(), move |next: bool| auto_connect.set(next)),
                    )),
            )
        }
    }
}
