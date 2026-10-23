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
        let mut subpage = use_state(|| false);
        let content: Element = match page() {
            Page::Controls => ControlsPage.into(),
            Page::Buttons => ButtonsPage.into(),
            Page::Rows => RowsPage {
                open: EventHandler::new(move |_: ()| subpage.set(true)),
            }
            .into(),
            Page::Feedback => FeedbackPage.into(),
            Page::Typography => TypographyPage.into(),
            Page::Icons => IconsPage.into(),
        };
        rect()
            .expanded()
            .color(tokens.text)
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
                .row(row("Determinate").trailing(progress_bar(progress()).width(Size::px(FIELD_WIDTH))))
                .row(row("Indeterminate").trailing(progress_bar(None).width(Size::px(FIELD_WIDTH))))
                .row(row("Step").trailing(action_button("Advance", move |_| {
                    progress.set((progress() + 0.15).min(1.0))
                })
                .compact())),
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
            .padding((0.0, GUTTER))
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
                    .width(Size::fill())
                    .spacing(S1)
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
                                .child(app_icon("org.gnome.Nautilus", ICON_APP_LG))
                                .child(app_icon("org.gnome.Nautilus", ICON_XL))
                                .child(app_icon("org.gnome.Nautilus", ICON_APP))
                                .child(app_icon("org.gnome.Nautilus", ICON_APP_SM)),
                        ),
                    )
                    .row(row("Missing icon").trailing(app_icon("no-such-icon", ICON_APP))),
            )
            .child(
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
            rect()
                .spacing(S3)
                .child(form_row(
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
                .child(form_row(
                    "Password",
                    text_field(self.password, |_| {}).password(),
                ))
        } else {
            rect()
                .spacing(S3)
                .child(form_row(
                    "Network name",
                    text_field(self.name, |_| {}).placeholder("Network name"),
                ))
                .child(form_row(
                    "Connect automatically",
                    checkbox(auto_connect(), move |next: bool| auto_connect.set(next)),
                ))
        }
    }
}
