//! Containers: groups, rows, overlays, menus, banners and status pages.

use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use bingux_ui::style::Align;
use bingux_ui::surface::provide_enabled;
use freya::prelude::*;

use super::common::{LONG, WIDE, rows, with};
use crate::model::Spec;

// ---- Groups ----------------------------------------------------------------

pub const GROUP: &[Spec] = &[
    Spec::live("Title and description", || {
        Group::new("Network".to_string())
            .description("Groups take an optional description.")
            .row(row("Wi-Fi").subtitle("Connected to HomeNet"))
            .row(row("Ethernet").value("Not connected"))
            .into_element()
    }),
    Spec::forced("Untitled", || {
        Group::new(None::<String>)
            .row(row("Title only"))
            .row(row("Another"))
            .into_element()
    }),
    Spec::live("With a trailing action", || {
        Group::new("Saved networks".to_string())
            .trailing(action_button("Forget all", |_| {}).flat().compact())
            .row(row("HomeNet"))
            .into_element()
    }),
    Spec::forced("Free content", || {
        Group::new("Free content".to_string())
            .content(rect().child(caption("Any element can sit in a group instead of rows.")))
            .into_element()
    }),
    Spec::forced("Long text", || {
        Group::new(LONG.to_string())
            .description(LONG)
            .row(row(LONG).subtitle(LONG))
            .into_element()
    }),
    Spec::forced("Section label and caption", || {
        vstack(S2)
            .child(section_label("Section label"))
            .child(caption("A caption in the muted colour."))
            .child(centered_caption("A centred caption."))
            .child(action_row(
                Some("Note beside an action"),
                action_button("Do it", |_| {}).compact(),
            ))
            .into_element()
    }),
];

// ---- Rows ------------------------------------------------------------------

pub const ROW: &[Spec] = &[
    Spec::live("Title, subtitle, value", || {
        rows(vec![
            row("Title only").into_element(),
            row("With subtitle")
                .subtitle("Secondary text in the muted colour")
                .into_element(),
            row("With value").value("Value").into_element(),
            row("Monospace value")
                .mono_value("192.168.1.20")
                .into_element(),
            row("Opens a subpage")
                .navigates()
                .on_press(|_| {})
                .into_element(),
        ])
    }),
    Spec::live("Decorated", || {
        rows(vec![
            row("Leading icon").leading(Icon::Network).into_element(),
            row("Leading tone")
                .leading(Icon::DialogWarning)
                .leading_tone(Tone::Warning)
                .into_element(),
            row("Signal")
                .leading_signal(3, true)
                .subtitle("WPA2")
                .into_element(),
            row("Avatar").leading_avatar("guest").into_element(),
            row("Application icon")
                .leading_app("org.gnome.Nautilus")
                .into_element(),
            row("Missing application icon")
                .leading_app("no-such-icon")
                .into_element(),
            row("Badge").badge("3").into_element(),
        ])
    }),
    Spec::forced("Selected, destructive, disabled", || {
        rows(vec![
            row("Selected")
                .selected(true)
                .on_press(|_| {})
                .into_element(),
            row("Destructive")
                .destructive()
                .on_press(|_| {})
                .into_element(),
            row("Disabled")
                .subtitle("Not available")
                .enabled(false)
                .into_element(),
        ])
    }),
    Spec::live("Control and chevron", || {
        let mut on = use_state(|| true);
        rows(vec![
            row("Wired connection")
                .leading(Icon::Network)
                .subtitle("Connected · 1 Gbit/s")
                .trailing(switch(on(), move |next: bool| on.set(next)))
                .navigates()
                .on_press(|_| {})
                .into_element(),
            row("Opens a subpage")
                .navigates()
                .on_press(|_| {})
                .into_element(),
        ])
    }),
    Spec::live("Button, dropdown and chevron", || {
        rows(vec![
            row("Wi-Fi network")
                .leading(Icon::Network)
                .trailing(action_button("Connect", |_| {}).compact())
                .navigates()
                .on_press(|_| {})
                .into_element(),
            row("Language")
                .trailing(dropdown(
                    vec![("en".into(), "English".into()), ("fr".into(), "Français".into())],
                    "en",
                    |_| {},
                ))
                .navigates()
                .on_press(|_| {})
                .into_element(),
        ])
    }),
    Spec::live("Trailing controls fill the row", || {
        let mut on = use_state(|| true);
        rows(vec![
            with("Switch", switch(on(), move |next: bool| on.set(next))),
            with("Action", action_button("Action", |_| {}).compact()),
            row("Tall row")
                .subtitle("The button keeps its inset above and below")
                .trailing(action_button("Action", |_| {}).compact())
                .into_element(),
        ])
    }),
    Spec::forced("Long text and wide script", || {
        rows(vec![
            row(LONG).subtitle(LONG).value("Value").into_element(),
            row(WIDE).subtitle(WIDE).badge("新").into_element(),
        ])
    }),
    Spec::live("Context menu (right-click)", || {
        rows(vec![
            row("Right-click for a menu")
                .context_menu(vec![
                    menu_item("Rename", |_| {}),
                    menu_item("Duplicate", |_| {}).disabled(),
                    menu_item("Delete", |_| {}).destructive(),
                ])
                .into_element(),
        ])
    }),
];

pub const ROW_EDITORS: &[Spec] = &[
    Spec::live("Text row and entry row", || {
        let mut name = use_state(|| "Bingux".to_string());
        let mut committed = use_state(|| "Workstation".to_string());
        rows(vec![
            text_row("Name", name, move |next: String| name.set(next)).into_element(),
            entry_row(
                "Entry row",
                committed.read().clone(),
                move |next: String| committed.set(next),
            )
            .placeholder("Hostname")
            .into_element(),
            with("Action", row_button("Change", |_| {})),
        ])
    }),
    Spec::live("Switch row, optimistic", || {
        let sharing = use_state(|| true);
        let overlays = use_overlays();
        rows(vec![
            switch_row("Share on network", sharing(), move |next: bool| {
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
            })
            .into_element(),
        ])
    }),
    Spec::live("Nested rows", || {
        let mut advanced = use_state(|| false);
        rows(vec![
            switch_row("Hardware acceleration", advanced(), move |next: bool| {
                advanced.set(next)
            })
            .nested(
                advanced(),
                vec![
                    property_row("Driver", "vulkan-radeon").into_element(),
                    property_row("Renderer", "llvmpipe").into_element(),
                ],
            )
            .into_element(),
        ])
    }),
    Spec::live("Level row", || {
        let mut level = use_state(|| 40.0_f64);
        rows(vec![
            level_row("Brightness", level(), move |next| level.set(next))
                .subtitle("Dims the whole display")
                .mark(75.0)
                .danger_from(90.0)
                .into_element(),
        ])
    }),
    Spec::live("Choice list", || {
        let mut choice = use_state(|| "b".to_string());
        choice_list(
            vec![
                ("a".into(), "First".into()),
                ("b".into(), "Second".into()),
                ("c".into(), "Third".into()),
            ],
            choice.read().clone(),
            move |key: String| choice.set(key),
        )
        .into_element()
    }),
];

pub const EXPANDER: &[Spec] = &[
    Spec::live("Closed, open it", || {
        rows(vec![
            expander_row(
                "Advanced",
                vec![
                    switch_row("Option one", true, |_| {}).into_element(),
                    switch_row("Option two", false, |_| {}).into_element(),
                ],
            )
            .subtitle("Press to reveal two rows")
            .into_element(),
        ])
    }),
    Spec::forced("Expanded", || {
        rows(vec![
            expander_row(
                "Advanced",
                vec![switch_row("Option one", true, |_| {}).into_element()],
            )
            .expanded(true)
            .into_element(),
        ])
    }),
    Spec::forced("Property rows", || {
        rows(vec![
            property_row("Kernel", "6.12.0-100.fc43.x86_64").into_element(),
            property_row(
                "Processor",
                "AMD Ryzen 7 5800X 8-Core Processor with a long name that wraps",
            )
            .into_element(),
        ])
    }),
];

pub const CARD: &[Spec] = &[
    Spec::forced("A box around free content", || {
        let tokens = use_tokens();
        boxed(&tokens)
            .child(caption(
                "`boxed(&tokens)` is the shared box with 16 px padding.",
            ))
            .into_element()
    }),
    Spec::forced("Stacks, spacer, divider, clamp", || {
        let tokens = use_tokens();
        vstack(S2)
            .child(
                hstack(S2)
                    .tw(&tokens, &[grow_children(), w_full()])
                    .child(caption("Left"))
                    .child(spacer())
                    .child(caption("Right")),
            )
            .child(divider(&tokens))
            .child(clamp(
                Dim::Menu,
                caption("Clamped to a maximum width, centred."),
            ))
            .into_element()
    }),
    Spec::forced("Tile rows", || {
        let tokens = use_tokens();
        let tiles: Vec<Element> = (1..=6)
            .map(|n| {
                rect()
                    .tw(
                        &tokens,
                        &[
                            w_full(),
                            h_full(),
                            rounded(Radius::Card),
                            bg(Paint::Card),
                            center(),
                        ],
                    )
                    .child(caption(format!("Tile {n}")))
                    .into_element()
            })
            .collect();
        Group::new("Four a row at desktop, two on a phone".to_string())
            .rows(tile_rows(&tokens, tiles, 72.0))
            .into_element()
    }),
];

// ---- Overlays --------------------------------------------------------------

/// Every dialog spec is the same button that opens it; the script opens it and
/// photographs it.
const OPEN: &str = "click-text Open dialog; wait 500; save {out}";

pub const DIALOG: &[Spec] = &[
    Spec::live("Info: one response, which is the default", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog("Update installed", "Bingux was updated. Restart when it suits you.")
                    .cancel("Close"),
            )
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("Confirm: the suggested response is the default", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog("Restart now?", "The update needs a restart to finish.")
                    .cancel("Later")
                    .response("Restart", ResponseKind::Suggested, |_| {}),
            )
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("Destructive: Cancel is the default", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog(
                    "Forget this network?",
                    "Reconnecting will need the password again.",
                )
                .cancel("Cancel")
                .response("Forget", ResponseKind::Destructive, |_| {}),
            )
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("Destructive: Tab moves the focus, the default mark stays", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog(
                    "Forget this network?",
                    "Reconnecting will need the password again.",
                )
                .cancel("Cancel")
                .response("Forget", ResponseKind::Destructive, |_| {}),
            )
        })
        .into_element()
    })
    .script("click-text Open dialog; wait 500; key tab; wait 300; save {out}"),
    Spec::live("Destructive: Enter confirms (default_action), never focused", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog("Delete this profile?", "The saved settings are removed.")
                    .cancel("Cancel")
                    .response("Delete", ResponseKind::Destructive, |_| {})
                    .default_action("delete"),
            )
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("Destructive with a warning icon", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog(
                    "Delete 3 files permanently?",
                    "They will not go to the Trash, and Undo cannot bring them back.",
                )
                .icon(DialogIcon::Warning)
                .cancel("Cancel")
                .response("Delete permanently", ResponseKind::Destructive, |_| {}),
            )
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("With a field: Enter in it runs the default", || {
        let overlays = use_overlays();
        let name = use_state(|| "Holiday".to_string());
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog("Rename preset", "")
                    .content(Element::from(text_field(name, |_: String| {}).autofocus()))
                    .cancel("Cancel")
                    .response("Rename", ResponseKind::Suggested, |_| {}),
            )
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("Long title and message", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog(
                    "Stop searching “Documents/Projects/Quarterly reports 2024/Archive”?",
                    "Files in this folder no longer appear in search results. Files that are already open are not changed, and you can add the folder again from the search settings whenever you like.",
                )
                .cancel("Cancel")
                .response("Remove", ResponseKind::Destructive, |_| {}),
            )
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("Three responses", || {
        let overlays = use_overlays();
        action_button("Open dialog", move |_| {
            overlays.show_dialog(
                dialog("“Report.pdf” already exists", "Something with this name is already in “Documents”.")
                    .response("Skip", ResponseKind::Default, |_| {})
                    .response("Keep both", ResponseKind::Default, |_| {})
                    .response("Replace", ResponseKind::Destructive, |_| {})
                    .default_action("keep_both")
                    .cancel_action("skip"),
            )
        })
        .into_element()
    })
    .script(OPEN),
    Spec::live("Framed dialog", || {
        let overlays = use_overlays();
        action_button("Open framed dialog", move |_| {
            overlays.show_dialog(framed_dialog(
                "Properties",
                row("Any content").subtitle("A dialog that is only its content"),
                400.0,
                200.0,
            ))
        })
        .into_element()
    })
    .script("click-text Open framed dialog; wait 500; save {out}"),
];

/// A button that queues the toast `make` builds.
fn toast_button(make: fn() -> Toast) -> Element {
    let overlays = use_overlays();
    action_button("Show toast", move |_| overlays.show_toast(make())).into_element()
}

pub const TOAST: &[Spec] = &[
    Spec::live("Toast", || {
        let overlays = use_overlays();
        action_button("Show toast", move |_| overlays.toast("Settings saved")).into_element()
    })
    .script("click-text Show toast; wait 400; save {out}"),
    Spec::live("Toast with an action", || {
        let overlays = use_overlays();
        action_button("Show toast", move |_| overlays.toast_with_action("Network forgotten", "Undo", |_| {}))
            .into_element()
    })
    .script("click-text Show toast; wait 400; save {out}"),
    Spec::live("Success", || {
        toast_button(|| Toast::new("Your changes were saved.").kind(BannerKind::Success))
    })
    .script("click-text Show toast; wait 400; save {out}"),
    Spec::live("Info with a title and an action", || {
        toast_button(|| {
            Toast::new("Bingux Settings 2.1 is ready.")
                .title("Update available")
                .kind(BannerKind::Info)
                .action("Install", |_| {})
        })
    })
    .script("click-text Show toast; wait 400; save {out}"),
    Spec::live("Warning", || {
        toast_button(|| Toast::new("The battery is low.").kind(BannerKind::Warning).action("Settings", |_| {}))
    })
    .script("click-text Show toast; wait 400; save {out}"),
    Spec::live("Error", || {
        toast_button(|| Toast::new("Could not reach the network.").kind(BannerKind::Error).action("Retry", |_| {}))
    })
    .script("click-text Show toast; wait 400; save {out}"),
    Spec::live("Long message", || {
        let overlays = use_overlays();
        action_button("Show toast", move |_| {
            overlays.toast("The display could not be changed because the monitor rejected the mode that was asked for")
        })
        .into_element()
    })
    .script("click-text Show toast; wait 400; save {out}"),
    Spec::live("Long message with an action", || {
        let overlays = use_overlays();
        action_button("Show toast", move |_| {
            overlays.toast_with_action(
                "The display could not be changed because the monitor rejected the mode that was asked for",
                "Undo",
                |_| {},
            )
        })
        .into_element()
    })
    .script("click-text Show toast; wait 400; save {out}"),
    Spec::live("Three queued: they show one after another", || {
        let overlays = use_overlays();
        action_button("Show toasts", move |_| {
            overlays.toast("First");
            overlays.show_toast(Toast::new("Second").kind(BannerKind::Info));
            overlays.show_toast(Toast::new("Third").kind(BannerKind::Success));
        })
        .into_element()
    }),
];

#[derive(PartialEq)]
struct SheetBody;

impl Component for SheetBody {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let name = use_state(String::new);
        let mut auto_connect = use_state(|| true);
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Save".into(),
                enabled: !name.read().trim().is_empty(),
                on_press: EventHandler::new(move |_: ()| window.close()),
            }),
        });
        popup_column().child(
            Group::new(None::<String>)
                .row(form_row(
                    "Network name",
                    text_field(name, |_| {}).placeholder("Network name"),
                ))
                .row(form_row(
                    "Connect automatically",
                    checkbox(auto_connect(), move |next: bool| auto_connect.set(next)),
                )),
        )
    }
}

pub const POPUP: &[Spec] = &[
    Spec::live("Popup (a sheet on a phone)", || {
        let window = use_window();
        action_button("Open popup", move |_| {
            window.open(popup("Add a network", SheetBody));
        })
        .into_element()
    })
    .script("click-text Open popup; wait 600; save {out}"),
    Spec::live("OK and Cancel, grabber", || {
        let window = use_window();
        action_button("Open popup", move |_| {
            window.open(
                popup("Add a network", SheetBody)
                    .flags(PopupFlags::OK | PopupFlags::CANCEL | PopupFlags::GRABBER)
                    .size(520.0, 360.0),
            );
        })
        .into_element()
    }),
];

fn action_items() -> Vec<MenuItem> {
    vec![
        menu_item("Cut", |_| {}).shortcut("Ctrl+X"),
        menu_item("Copy", |_| {})
            .icon(Icon::EditCopy)
            .shortcut("Ctrl+C"),
        menu_item("Paste", |_| {}).disabled(),
        menu_separator(),
        menu_item("Show hidden files", |_| {}).checked(true),
        menu_item("Show preview", |_| {}).checked(false),
        menu_separator(),
        submenu(
            "Sort by",
            vec![
                menu_item("Name", |_| {}).checked(true),
                menu_item("Size", |_| {}).checked(false),
                menu_item("Date modified", |_| {}).checked(false),
            ],
        ),
        menu_separator(),
        menu_item("Delete", |_| {}).destructive(),
    ]
}

pub const MENU: &[Spec] = &[
    Spec::live("Menu button: items, ticks, shortcuts, a submenu", || {
        menu_button("Actions", action_items()).into_element()
    })
    .script("click-text Actions; wait 400; save {out}"),
    Spec::live("Type-ahead: type to jump to an item", || {
        menu_button("Actions", action_items()).into_element()
    })
    .script("click-text Actions; wait 300; type Sh; wait 300; save {out}"),
    Spec::live("Submenu open from the keyboard", || {
        menu_button("Actions", action_items()).into_element()
    })
    .script("click-text Actions; wait 300; key down; key down; key down; key down; key down; key down; key down; key right; wait 300; save {out}"),
    Spec::live("With an icon", || {
        menu_button("More", vec![menu_item("Refresh", |_| {}).icon(Icon::Refresh)]).icon(Icon::ViewMore).into_element()
    }),
    Spec::live("Context menu at the pointer", || {
        let overlays = use_overlays();
        action_button("Open at 200, 300", move |_| {
            overlays.show_menu((200.0, 300.0), action_items())
        })
        .into_element()
    }),
];

/// A column of banners as a page would hold them: the same width, spaced.
fn banners(items: Vec<Banner>) -> Element {
    vstack(S2).children(items.into_iter().map(|item| item.into_element())).into_element()
}

pub const BANNER: &[Spec] = &[
    Spec::live("Info with action, dismissible", || {
        let mut shown = use_state(|| true);
        vstack(S2)
            .child(
                banner("Updates are ready to install.")
                    .visible(shown())
                    .action("Install", |_| {})
                    .dismissible(move |_| shown.set(false)),
            )
            .child(action_button("Show again", move |_| shown.set(true)).compact())
            .into_element()
    }),
    Spec::forced("Every kind", || {
        banners(
            [
                (BannerKind::Neutral, "Settings were synced."),
                (BannerKind::Info, "Updates are ready to install."),
                (BannerKind::Success, "Your changes were saved."),
                (BannerKind::Warning, "The battery is low."),
                (BannerKind::Error, "Could not reach the network."),
            ]
            .into_iter()
            .map(|(kind, text)| banner(text).kind(kind).action("Open", |_| {}).dismissible(|_| {}))
            .collect(),
        )
    }),
    Spec::forced("Message only", || {
        banners(
            [BannerKind::Info, BannerKind::Success, BannerKind::Warning, BannerKind::Error]
                .into_iter()
                .map(|kind| banner("Nothing needs doing.").kind(kind))
                .collect(),
        )
    }),
    Spec::forced("Dismiss only", || {
        banners(vec![
            banner("The battery is low.").kind(BannerKind::Warning).dismissible(|_| {}),
        ])
    }),
    Spec::forced("Title and message", || {
        banners(vec![
            banner("The display could not be changed.")
                .title("Display error")
                .kind(BannerKind::Error)
                .action("Retry", |_| {})
                .dismissible(|_| {}),
            banner("Your changes will apply the next time you sign in.")
                .title("Restart required")
                .kind(BannerKind::Warning),
        ])
    }),
    Spec::forced("Several actions", || {
        banners(vec![
            banner("A new version of Bingux Settings is available.")
                .action("Later", |_| {})
                .action("Install", |_| {})
                .dismissible(|_| {}),
        ])
    }),
    Spec::forced("Custom icon, no icon", || {
        banners(vec![
            banner("Connected to HomeNet.").kind(BannerKind::Success).icon(Icon::Bluetooth),
            banner("Plain, with no glyph.").without_icon(),
        ])
    }),
    Spec::forced("Long text", || {
        banners(vec![
            banner(LONG).action("Open", |_| {}).dismissible(|_| {}),
            banner(LONG).kind(BannerKind::Error).title("Could not apply").action("Retry", |_| {}),
        ])
    }),
];

pub const STATUS: &[Spec] = &[
    Spec::forced("Loading", || {
        status(Status::Loading("Reading networks.".into())).into_element()
    }),
    Spec::forced("Empty", || {
        status(Status::Empty("No saved networks.".into())).into_element()
    }),
    Spec::forced("Error with an action", || {
        status(Status::Error("bluetoothctl is not installed.".into()))
            .action("Try again", |_| {})
            .into_element()
    }),
    Spec::forced("Info", || {
        status(Status::Info("No devices found.".into())).into_element()
    }),
];

pub const STATUS_PAGE: &[Spec] = &[
    Spec::forced("Icon, title, description and action", || {
        let tokens = use_tokens();
        rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::SpeakerRoom),
                    rounded(Radius::Card),
                    bg(Paint::Card),
                ],
            )
            .child(
                status_page(Icon::Bluetooth, "No devices found")
                    .description(
                        "Turn Bluetooth on and make the device discoverable, then try again.",
                    )
                    .action("Scan again", |_| {}),
            )
            .into_element()
    }),
    Spec::forced("Title only", || {
        let tokens = use_tokens();
        rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::Box24),
                    rounded(Radius::Card),
                    bg(Paint::Card),
                    items(Align::Center),
                ],
            )
            .child(status_page(Icon::Folder, "This folder is empty"))
            .into_element()
    }),
];

pub const SKELETON: &[Spec] = &[
    Spec::forced("Page skeleton", || skeleton()),
    Spec::forced("Rows and a shape", || {
        vstack(S3)
            .child(skeleton_rows(&["Network", "Security"]))
            .child(skeleton_shape(&[2, 3]))
            .child(rows(vec![skeleton_row()]))
            .into_element()
    }),
];

/// Controls below a component that calls `provide_enabled(false)` dim and
/// ignore the pointer without being disabled one by one.
#[derive(PartialEq)]
struct DisabledContainer;

impl Component for DisabledContainer {
    fn render(&self) -> impl IntoElement {
        provide_enabled(false);
        rows(vec![
            switch_row("Switch", true, |_| {}).into_element(),
            with("Button", action_button("Action", |_| {}).compact()),
            with(
                "Dropdown",
                dropdown(vec![("a".into(), "First".into())], "a", |_| {}),
            ),
        ])
    }
}

pub const CONTAINER_ENABLED: &[Spec] = &[Spec::forced(
    "provide_enabled(false) above three controls",
    || DisabledContainer.into_element(),
)];
