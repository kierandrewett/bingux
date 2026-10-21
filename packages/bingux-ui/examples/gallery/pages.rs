//! Gallery pages shared by the windowed gallery and the snapshot example.

use bingux_ui::prelude::*;
use freya::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Controls,
    Buttons,
    Rows,
    Feedback,
    Typography,
    Icons,
}

impl Page {
    pub const ALL: [Page; 6] = [
        Page::Controls,
        Page::Buttons,
        Page::Rows,
        Page::Feedback,
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
        let content: Element = match page() {
            Page::Controls => ControlsPage.into(),
            Page::Buttons => ButtonsPage.into(),
            Page::Rows => RowsPage.into(),
            Page::Feedback => FeedbackPage.into(),
            Page::Typography => TypographyPage.into(),
            Page::Icons => IconsPage.into(),
        };
        rect()
            .expanded()
            .color(tokens.text)
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(NavigationSplitView::new(
                "Gallery",
                Page::ALL.to_vec(),
                page,
                content,
            )))
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
                        vec![
                            ("en".into(), "English (United Kingdom)".into()),
                            ("fr".into(), "French (France)".into()),
                            ("de".into(), "German (Germany)".into()),
                            ("es".into(), "Spanish (Spain)".into()),
                        ],
                        &language.read().clone(),
                        move |key: String| language.set(key),
                    ))
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
        let tokens = use_tokens();
        let plain = use_state(String::new);
        let query = use_state(|| "wifi".to_string());
        let secret = use_state(|| "hunter2".to_string());
        let bad = use_state(|| "relative/path".to_string());
        page_column()
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .spacing(S2)
                    .cross_align(Alignment::Center)
                    .child(action_button("Action", |_| {}))
                    .child(action_button("With icon", |_| {}).icon(Icon::Folder))
                    .child(primary_button("Primary", |_| {}))
                    .child(action_button("Flat", |_| {}).flat())
                    .child(action_button("Destructive", |_| {}).destructive())
                    .child(action_button("Loading", |_| {}).loading(true))
                    .child(action_button("Disabled", |_| {}).enabled(false)),
            )
            .child(
                rect()
                    .direction(Direction::Horizontal)
                    .spacing(S2)
                    .cross_align(Alignment::Center)
                    .child(icon_button(Icon::Refresh, |_| {}).tooltip("Refresh"))
                    .child(icon_button(Icon::Delete, |_| {}).tooltip("Delete").danger())
                    .child(icon_button(Icon::OpenMenu, |_| {}).tooltip("Menu"))
                    .child(row_button("Row pill", |_| {}))
                    .child(
                        label()
                            .text("Icon buttons and the row pill")
                            .color(tokens.muted),
                    ),
            )
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

#[derive(PartialEq)]
struct RowsPage;

impl Component for RowsPage {
    fn render(&self) -> impl IntoElement {
        let mut name = use_state(|| "Bingux".to_string());
        let mut committed = use_state(|| "Workstation".to_string());
        page_column()
            .child(
                Group::new("Plain rows".to_string())
                    .description("Groups take an optional description.")
                    .row(row("Title only"))
                    .row(row("With subtitle").subtitle("Secondary text in the muted colour"))
                    .row(row("With value").value("Value"))
                    .row(row("Navigates").navigates().on_press(|_| {})),
            )
            .child(
                Group::new("Decorated".to_string())
                    .row(row("Leading icon").leading(Icon::Network))
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
    }
}

#[derive(PartialEq)]
struct FeedbackPage;

impl Component for FeedbackPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut progress = use_state(|| 0.35_f64);
        let mut notice = use_state(|| true);
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
                    })),
                )
                .row(row("Toast").trailing(action_button("Show", move |_| {
                    overlays.toast("Settings saved")
                })))
                .row(
                    row("Toast with action").trailing(action_button("Show", move |_| {
                        overlays.toast_with_action("Network forgotten", "Undo", |_| {})
                    })),
                )
                .row(
                    row("Banner").trailing(action_button("Show again", move |_| notice.set(true))),
                ),
        )
        .child(
            Group::new("Progress".to_string())
                .row(row("Determinate").trailing(progress_bar(progress()).width(Size::px(FIELD_WIDTH))))
                .row(row("Indeterminate").trailing(progress_bar(None).width(Size::px(FIELD_WIDTH))))
                .row(row("Step").trailing(action_button("Advance", move |_| {
                    progress.set((progress() + 0.15).min(1.0))
                }))),
        )
        .child(
            rect()
                .direction(Direction::Horizontal)
                .spacing(S5)
                .cross_align(Alignment::Center)
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
                .width(Size::fill())
                .height(Size::px(SAMPLE_PANEL_HEIGHT))
                .corner_radius(RADIUS_LG)
                .background(tokens.card)
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
        page_column()
            .padding((0.0, GUTTER))
            .child(
                label()
                    .text("Hero title, 24 bold")
                    .font_size(TEXT_XL)
                    .font_weight(FontWeight::SEMI_BOLD),
            )
            .child(heading("Heading, 16 semibold"))
            .child(section_label("Section label, 13 semibold"))
            .child(label().text("Body, 14").font_size(TEXT_MD))
            .child(caption("Caption, 12 muted"))
            .child(
                label()
                    .text("Accent")
                    .font_size(TEXT_MD)
                    .color(tokens.accent),
            )
            .child(
                label()
                    .text("Danger")
                    .font_size(TEXT_MD)
                    .color(tokens.danger),
            )
            .child(
                label()
                    .text("Success")
                    .font_size(TEXT_MD)
                    .color(tokens.success),
            )
            .child(
                label()
                    .text("Warning")
                    .font_size(TEXT_MD)
                    .color(tokens.warning),
            )
    }
}

#[derive(PartialEq)]
struct IconsPage;

impl Component for IconsPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        page_column().child(
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::wrap_spacing(8.0))
                .spacing(S2)
                .children(Icon::ALL.iter().map(|glyph| {
                    Element::from(
                        rect()
                            .width(Size::px(SAMPLE_TILE_WIDTH))
                            .height(Size::px(SAMPLE_TILE_HEIGHT))
                            .corner_radius(RADIUS_LG)
                            .background(tokens.card)
                            .center()
                            .spacing(S1)
                            .child(icon(*glyph, 20.0, tokens.text))
                            .child(
                                label()
                                    .text(format!("{glyph:?}"))
                                    .font_size(TEXT_XS)
                                    .color(tokens.muted)
                                    .max_lines(1),
                            ),
                    )
                })),
        )
    }
}
