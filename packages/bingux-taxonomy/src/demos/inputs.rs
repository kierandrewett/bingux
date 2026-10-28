//! Controls: text input, forms and selection.

use bingux_ui::controls::colour_picker::{colour_chip, colour_picker};
use bingux_ui::controls::form::validate;
use bingux_ui::prelude::*;
use bingux_ui::style::colour_from_argb;
use freya::prelude::*;

use super::common::{LONG, rows, with};
use crate::model::Spec;

// ---- Text input ------------------------------------------------------------

pub const TEXT_FIELD: &[Spec] = &[
    Spec::live("Rest, hover, focus, typing", || {
        let value = use_state(String::new);
        text_field(value, |_| {})
            .placeholder("Type here")
            .fill_width()
            .into_element()
    }),
    Spec::forced("Filled", || {
        let value = use_state(|| "bingux-workstation".to_string());
        text_field(value, |_| {}).fill_width().into_element()
    }),
    Spec::forced("Password", || {
        let value = use_state(|| "hunter2".to_string());
        text_field(value, |_| {})
            .password()
            .fill_width()
            .into_element()
    }),
    Spec::forced("Invalid", || {
        let value = use_state(|| "relative/path".to_string());
        text_field(value, |_| {})
            .invalid(Some("Enter an absolute path.".into()))
            .fill_width()
            .into_element()
    }),
    Spec::forced("Disabled", || {
        let value = use_state(|| "read only".to_string());
        text_field(value, |_| {})
            .enabled(false)
            .fill_width()
            .into_element()
    }),
    Spec::forced("Leading icon, clearable", || {
        let value = use_state(|| "wifi".to_string());
        text_field(value, |_| {})
            .leading(Icon::Search)
            .clearable()
            .fill_width()
            .into_element()
    }),
    Spec::forced("Suffix", || {
        let value = use_state(|| "1500".to_string());
        text_field(value, |_| {})
            .suffix("bytes")
            .fill_width()
            .into_element()
    }),
    Spec::forced("Multiline, monospace", || {
        let value = use_state(|| "# Office\nEndpoint = vpn.example.com:51820".to_string());
        text_field(value, |_| {})
            .multiline()
            .lines(3)
            .monospace()
            .fill_width()
            .into_element()
    }),
    Spec::forced("Long text", || {
        let value = use_state(|| LONG.repeat(2));
        text_field(value, |_| {}).fill_width().into_element()
    }),
];

pub const SEARCH_FIELD: &[Spec] = &[
    Spec::live("Rest, hover, focus, typing", || {
        let query = use_state(String::new);
        search_field(query, |_| {}).fill_width().into_element()
    }),
    Spec::forced("With a query", || {
        let query = use_state(|| "wifi".to_string());
        search_field(query, |_| {}).fill_width().into_element()
    }),
];

fn form_demo(name: &'static str, password: &'static str, gateway: &'static str) -> Element {
    let name = use_state(|| name.to_string());
    let password = use_state(|| password.to_string());
    let gateway = use_state(|| gateway.to_string());
    let error = {
        let text = gateway.read();
        if text.is_empty() {
            None
        } else {
            validate::ipv4(&text).err()
        }
    };
    Group::titled("Network")
        .row(
            form_field("Network name", name)
                .hint("Exactly as the router shows it")
                .validate(validate::ssid),
        )
        .row(
            form_field("Password", password)
                .password()
                .validate(validate::wpa_passphrase)
                .hint("8 to 63 characters"),
        )
        .row(form_field("Gateway", gateway).monospace().error(error))
        .into_element()
}

pub const FORM_FIELD: &[Spec] = &[
    Spec::live("Valid, typing validates", || {
        form_demo("Home", "correct horse battery", "192.168.1.1")
    }),
    Spec::forced("Errors", || form_demo("", "short", "192.168.1")),
    Spec::forced("Optional, suffix, trailing control", || {
        let mtu = use_state(|| "1500".to_string());
        let allowed = use_state(|| "10.8.0.0/24".to_string());
        Group::titled("Tunnel")
            .row(
                form_field("MTU", mtu)
                    .suffix("bytes")
                    .wide()
                    .optional()
                    .validate_with(validate::integer_in(576..=9000)),
            )
            .row(
                form_field("Allowed IPs", allowed)
                    .monospace()
                    .wide()
                    .validate(validate::ipv4_cidr)
                    .trailing(icon_button(Icon::ListRemove, |_| {}).tooltip("Remove")),
            )
            .into_element()
    }),
    Spec::forced("Multiline", || {
        let peers =
            use_state(|| "PublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=".to_string());
        Group::titled("Peers")
            .row(
                form_field("Peers", peers)
                    .multiline(4)
                    .wide()
                    .monospace()
                    .hint("One peer per block"),
            )
            .into_element()
    }),
];

pub const FORM_STATE: &[Spec] = &[
    Spec::live("Pending bar, apply and revert", || {
        let mut changed = use_state(|| true);
        vstack(S3)
            .child(pending_bar(
                changed(),
                true,
                move |_| changed.set(false),
                move |_| changed.set(false),
            ))
            .child(action_button("Make a change", move |_| changed.set(true)).compact())
            .into_element()
    }),
    Spec::forced("Pending bar, invalid", || {
        vstack(S3)
            .child(pending_bar(true, false, |_| {}, |_| {}))
            .into_element()
    }),
    Spec::forced("Live state", || {
        live_state(vec![
            ("IP address".to_string(), "192.168.1.20".to_string()),
            ("Gateway".to_string(), "192.168.1.1".to_string()),
            ("Link speed".to_string(), "1 Gbit/s".to_string()),
        ])
    }),
];

pub const VALIDATORS: &[Spec] = &[Spec::forced("What each validator says", || {
    let checks: Vec<(&str, &str, Result<(), String>)> = vec![
        (
            "ipv4 192.168.1.300",
            "192.168.1.300",
            validate::ipv4("192.168.1.300"),
        ),
        ("ipv6 fe80::1", "fe80::1", validate::ipv6("fe80::1")),
        (
            "ip_list 1.1.1.1, nope",
            "1.1.1.1, nope",
            validate::ip_list("1.1.1.1, nope"),
        ),
        (
            "ipv4_cidr 10.0.0.0/33",
            "10.0.0.0/33",
            validate::ipv4_cidr("10.0.0.0/33"),
        ),
        (
            "mac 00:11:22:33:44",
            "00:11:22:33:44",
            validate::mac("00:11:22:33:44"),
        ),
        ("port 70000", "70000", validate::port("70000")),
        (
            "hostname_or_ip -bad-",
            "-bad-",
            validate::hostname_or_ip("-bad-"),
        ),
        (
            "url example.com",
            "example.com",
            validate::url("example.com"),
        ),
        ("ssid (empty)", "", validate::ssid("")),
        ("wpa_passphrase abc", "abc", validate::wpa_passphrase("abc")),
        ("wireguard_key abc", "abc", validate::wireguard_key("abc")),
    ];
    rows(
        checks
            .into_iter()
            .map(|(title, _, result)| {
                row(title)
                    .subtitle(match &result {
                        Ok(()) => "Accepted".to_string(),
                        Err(message) => message.clone(),
                    })
                    .into_element()
            })
            .collect(),
    )
})];

// ---- Selection -------------------------------------------------------------

fn languages() -> Vec<(String, String)> {
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
    .collect()
}

pub const DROPDOWN: &[Spec] = &[
    Spec::live("Closed, hover, open (a long list scrolls)", || {
        let mut language = use_state(|| "en".to_string());
        with(
            "Language",
            dropdown(languages(), language.read().clone(), move |key: String| {
                language.set(key)
            }),
        )
    })
    .script("click-text English; wait 400; save {out}"),
    Spec::forced("Disabled", || {
        with(
            "Language",
            dropdown(languages(), "en", |_| {}).enabled(false),
        )
    }),
    Spec::forced("With icons", || {
        with(
            "Icon",
            dropdown(
                vec![
                    ("a".into(), "Network".into()),
                    ("b".into(), "Bluetooth".into()),
                    ("c".into(), "Sound".into()),
                ],
                "b",
                |_| {},
            )
            .icons(vec![Icon::Network, Icon::Bluetooth, Icon::Sound]),
        )
    }),
    Spec::forced("Long option", || {
        with(
            "Option",
            dropdown(vec![("long".into(), LONG.into())], "long", |_| {}),
        )
    }),
];

pub const SEGMENTED: &[Spec] = &[
    Spec::live("Rest, hover, pressed", || {
        let mut segment = use_state(|| "light".to_string());
        segmented(
            vec![
                ("light".into(), "Light".into()),
                ("dark".into(), "Dark".into()),
                ("auto".into(), "Auto".into()),
            ],
            &segment.read().clone(),
            move |key: String| segment.set(key),
        )
        .into_element()
    }),
    Spec::forced("Filled to the width", || {
        segmented(
            vec![
                ("a".into(), "Details".into()),
                ("b".into(), "Security".into()),
            ],
            "a",
            |_| {},
        )
        .fill()
        .into_element()
    }),
    Spec::forced("As a row", || {
        rows(vec![
            choice_row(
                "Appearance",
                vec![
                    ("light".into(), "Light".into()),
                    ("dark".into(), "Dark".into()),
                ],
                "dark",
                |_| {},
            )
            .into_element(),
        ])
    }),
];

/// The picker as a popup's body. The colour lives in the popup, not in the
/// specimen that opens it: the specimen is rebuilt when the window changes
/// between desktop and phone, and a popup must not outlive its state.
#[derive(PartialEq)]
struct PickerPopup;

impl Component for PickerPopup {
    fn render(&self) -> impl IntoElement {
        let mut colour = use_state(|| colour_from_argb(255, 53, 132, 228));
        let tokens = use_tokens();
        popup_column()
            .child(colour_picker(colour(), move |next| colour.set(next)))
            .child(colour_chip(tokens, colour(), 24.0))
    }
}

pub const COLOUR_PICKER: &[Spec] = &[
    Spec::live("Pick a swatch or drag the hue", || {
        let mut colour = use_state(|| colour_from_argb(255, 53, 132, 228));
        let tokens = use_tokens();
        vstack(S2)
            .child(colour_picker(colour(), move |next| colour.set(next)))
            .child(colour_chip(tokens, colour(), 24.0))
            .into_element()
    })
    .page(420.0),
    Spec::live("Translucent start", || {
        let mut colour = use_state(|| colour_from_argb(128, 53, 132, 228));
        let tokens = use_tokens();
        vstack(S2)
            .child(colour_picker(colour(), move |next| colour.set(next)))
            .child(colour_chip(tokens, colour(), 24.0))
            .into_element()
    })
    .page(420.0),
    Spec::live("In a popup (a sheet on a phone)", || {
        let window = use_window();
        action_button("Pick a colour", move |_| {
            window.open(popup("Pick a colour", PickerPopup));
        })
        .into_element()
    })
    .script("click-text Pick a colour; wait 600; save {out}"),
];

/// How tall a framed strip is: the bar and a few lines of page.
const FRAME_HEIGHT: f32 = 168.0;

pub const TABS: &[Spec] = &[
    Spec::live("Select, close, add, drag to reorder", || {
        live_tabs(&["Home", "Documents", "Downloads"])
    })
    .page(FRAME_HEIGHT),
    Spec::live("Many tabs: scroll, fade, tab list", || {
        live_tabs(&[
            "Home",
            "Documents",
            "Downloads",
            "Pictures",
            "Music",
            "Videos",
            "Projects",
            "Archive",
            "Backups",
            "Desktop",
            "Templates",
            "Public",
        ])
    })
    .page(FRAME_HEIGHT),
    Spec::forced("Nine tabs: the strip scrolls, the selected tab in view", || {
        framed(
            tab_strip(
                [
                    "Home",
                    "Documents",
                    "Downloads",
                    "Pictures",
                    "Music",
                    "Videos",
                    "Projects",
                    "Archive",
                    "Backups",
                ]
                .iter()
                .enumerate()
                .map(|(index, name)| TabItem::new(index as u64 + 1, *name))
                .collect(),
                5,
            )
            .on_new(|_| {}),
            None,
        )
    })
    .page(FRAME_HEIGHT),
    Spec::forced("Long titles", || {
        framed(
            tab_strip(vec![TabItem::new(1, LONG), TabItem::new(2, "Short")], 1).on_new(|_| {}),
            None,
        )
    })
    .page(FRAME_HEIGHT),
    Spec::forced("With icons", || {
        framed(
            tab_strip(
                vec![
                    TabItem::new(1, "Home").icon(Icon::Folder),
                    TabItem::new(2, "Computer").icon(Icon::Computer),
                    TabItem::new(3, "Documents").icon(Icon::DocumentOpen),
                ],
                2,
            )
            .on_new(|_| {}),
            None,
        )
    })
    .page(FRAME_HEIGHT),
    Spec::forced("Single tab", || {
        framed(tab_strip(vec![TabItem::new(1, "Home")], 1).on_new(|_| {}), None)
    })
    .page(FRAME_HEIGHT),
    Spec::forced("Without a New tab button", || {
        framed(
            tab_strip(vec![TabItem::new(1, "Home"), TabItem::new(2, "Documents")], 1),
            None,
        )
    })
    .page(FRAME_HEIGHT),
    Spec::forced("Keyboard focus on the selected tab", || {
        framed(
            tab_strip(
                vec![TabItem::new(1, "Home"), TabItem::new(2, "Documents"), TabItem::new(3, "Downloads")],
                2,
            )
            .on_new(|_| {}),
            None,
        )
    })
    .page(FRAME_HEIGHT)
    .script("key tab; wait 300; save {out}"),
    Spec::forced("Painted in a card's surface", || {
        framed(
            tab_strip(
                vec![TabItem::new(1, "Home"), TabItem::new(2, "Documents"), TabItem::new(3, "Downloads")],
                1,
            )
            .on_new(|_| {}),
            Some(Paint::Card),
        )
    })
    .page(FRAME_HEIGHT),
];

/// `strip` in a frame over a few lines of page, as an application uses
/// tabs: the selected tab and the page are one surface, `surface` or the
/// page colour, and the bar behind the tabs is a step darker.
fn framed(strip: TabStrip, surface: Option<Paint>) -> Element {
    let tokens = use_tokens();
    let bar = rect().tw(&tokens, &[w_full(), h(Dim::Header)]).child(strip);
    let page = rect()
        .tw(&tokens, &[w_full(), flex_col(), p(S4), gap(S1)])
        .child(caption("The page the selected tab opens."))
        .child(caption("The tab and this page are one surface; there is no line between them."));
    let mut frame = tab_frame(bar, page);
    if let Some(surface) = surface {
        frame = frame.surface(surface);
    }
    frame.into_element()
}

/// A strip that adds, selects, closes and reorders its own tabs, starting
/// with `names`, so every behaviour of the control can be tried.
fn live_tabs(names: &'static [&'static str]) -> Element {
    let mut tabs = use_state(|| {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| TabItem::new(index as u64 + 1, *name))
            .collect::<Vec<_>>()
    });
    let mut selected = use_state(|| 1_u64);
    let mut next = use_state(|| names.len() as u64 + 1);
    framed(
        tab_strip(tabs.read().clone(), selected())
            .on_select(move |id| selected.set(id))
            .on_close(move |id| {
                let mut list = tabs.write();
                let Some(index) = list.iter().position(|tab| tab.id == id) else {
                    return;
                };
                list.remove(index);
                if selected() == id && !list.is_empty() {
                    selected.set(list[index.min(list.len() - 1)].id);
                }
            })
            .on_new(move |_| {
                let id = next();
                next.set(id + 1);
                tabs.write().push(TabItem::new(id, format!("Tab {id}")));
                selected.set(id);
            })
            .on_reorder(move |(id, to): (u64, usize)| {
                let mut list = tabs.write();
                if let Some(from) = list.iter().position(|tab| tab.id == id) {
                    let tab = list.remove(from);
                    let to = to.min(list.len());
                    list.insert(to, tab);
                }
            }),
        None,
    )
}
