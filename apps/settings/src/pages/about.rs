//! About: the device, its operating system and the hardware it runs on. The
//! facts are read once in the background when the page opens. The device name
//! can be changed in a popup, and every fact can be copied as text.

use std::time::Duration;

use bingux_ui::motion::reveal_block_if;
use bingux_ui::style::*;
use bingux_ui::window::{PopupFlags, WindowLike, WindowParent};
use freya::prelude::*;

use super::loading::revealed;
use crate::system::{self, CommandResult};
use crate::theme::Tokens;
use crate::ui::typography::{CAPTION, EMPHASIS, Ink, TITLE};
use crate::ui::{
    Group, Icon, Overlays, PopupAction, PopupActions, Status, Window, action_button, caption,
    form_row, icon, menu_item, page_column, popup, popup_column, property_row, section_label,
    status, text_field, usage_meter, use_overlays, use_page_busy, use_popup_actions, use_tokens,
    use_window,
};

/// How long `hostnamectl` may take to write the new name.
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Facts {
    hostname: String,
    /// The hardware model, such as "Micro-Star International MS-7C56".
    model: String,
    /// `NAME` from os-release, shown large in the hero.
    os_name: String,
    /// `VERSION` from os-release, shown under the name.
    os_version: String,
    /// `PRETTY_NAME` from os-release, used in copied text.
    os: String,
    kernel: String,
    processor: String,
    memory: String,
    graphics: String,
    display: String,
    uptime: String,
    /// Bytes used and bytes in total on the root file system.
    disk: Option<(u64, u64)>,
    gnoblin: String,
}

/// A value from `/etc/os-release`, without its quotes.
fn os_release(key: &str) -> Option<String> {
    crate::system::read_to_string("/etc/os-release")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|value| value.trim_matches('"').to_string())
        })
}

fn dmi(name: &str) -> Option<String> {
    crate::system::read_to_string(format!("/sys/devices/virtual/dmi/id/{name}"))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| {
            !text.is_empty() && text != "To Be Filled By O.E.M." && text != "Default string"
        })
}

fn processor() -> String {
    crate::system::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, value)| value.trim().to_string())
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

fn memory() -> String {
    crate::system::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("MemTotal:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|kb| kb.parse::<f64>().ok())
        })
        .map(|kb| format!("{:.1} GiB", kb / 1024.0 / 1024.0))
        .unwrap_or_else(|| "Unknown".to_string())
}

/// The graphics adapter's name from `lspci`, without its revision.
fn graphics() -> String {
    system::run("lspci", &[], Duration::from_secs(3))
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| {
                    [
                        "VGA compatible controller",
                        "3D controller",
                        "Display controller",
                    ]
                    .iter()
                    .any(|class| line.contains(class))
                })
                .and_then(|line| line.split_once(": "))
                .map(|(_, device)| {
                    device
                        .split(" (rev")
                        .next()
                        .unwrap_or(device)
                        .trim()
                        .to_string()
                })
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

/// The first connected display's mode, such as "2560x1440".
fn display() -> String {
    let Ok(entries) = crate::system::read_dir("/sys/class/drm") else {
        return "Unknown".to_string();
    };
    let mut modes = entries.flatten().filter_map(|entry| {
        let path = entry.path();
        let status = crate::system::read_to_string(path.join("status")).ok()?;
        if status.trim() != "connected" {
            return None;
        }
        crate::system::read_to_string(path.join("modes"))
            .ok()?
            .lines()
            .next()
            .map(str::to_string)
    });
    let Some(first) = modes.next() else {
        return "Unknown".to_string();
    };
    match modes.count() {
        0 => first,
        more => format!("{first} (+{more} more)"),
    }
}

/// Time since boot, such as "3 days, 4 hours" or "42 minutes".
fn uptime() -> String {
    let Some(seconds) = crate::system::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
    else {
        return "Unknown".to_string();
    };
    let minutes = (seconds / 60.0) as u64;
    let (days, hours, minutes) = (minutes / 1440, minutes % 1440 / 60, minutes % 60);
    let plural =
        |count: u64, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    if days > 0 {
        format!("{}, {}", plural(days, "day"), plural(hours, "hour"))
    } else if hours > 0 {
        format!("{}, {}", plural(hours, "hour"), plural(minutes, "minute"))
    } else {
        plural(minutes, "minute")
    }
}

fn disk() -> Option<(u64, u64)> {
    let text = system::run(
        "df",
        &["-B1", "--output=size,used", "/"],
        Duration::from_secs(3),
    )
    .ok()?;
    let mut fields = text.lines().nth(1)?.split_whitespace();
    let total = fields.next()?.parse().ok()?;
    let used = fields.next()?.parse().ok()?;
    Some((used, total))
}

/// A size in decimal units, as storage is sold: "35" and "GB", "1.2" and "TB".
fn size_parts(bytes: u64) -> (String, &'static str) {
    const GB: f64 = 1e9;
    const TB: f64 = 1e12;
    let bytes = bytes as f64;
    if bytes >= TB {
        (format!("{:.1}", bytes / TB), "TB")
    } else if bytes >= 10.0 * GB {
        (format!("{:.0}", bytes / GB), "GB")
    } else {
        (format!("{:.1}", bytes / GB), "GB")
    }
}

fn size_text(bytes: u64) -> String {
    let (value, unit) = size_parts(bytes);
    format!("{value} {unit}")
}

fn gnoblin_version() -> String {
    system::run("gnoblinctl", &["--version"], Duration::from_secs(3))
        .ok()
        .and_then(|text| text.lines().next().map(|line| line.trim().to_string()))
        .map(|line| {
            line.strip_prefix("Gnoblin ")
                .map(str::to_string)
                .unwrap_or(line)
        })
        .unwrap_or_else(|| "Not running".to_string())
}

fn collect() -> Facts {
    let model = match (dmi("sys_vendor"), dmi("product_name")) {
        (Some(vendor), Some(product)) => format!("{vendor} {product}"),
        (vendor, product) => vendor.or(product).unwrap_or_else(|| "Unknown".to_string()),
    };
    let os = os_release("PRETTY_NAME").unwrap_or_else(|| "Unknown".to_string());
    let facts = Facts {
        hostname: crate::system::read_to_string("/etc/hostname")
            .map(|text| text.trim().to_string())
            .ok()
            .filter(|text| !text.is_empty())
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "localhost".to_string()),
        model,
        os_name: os_release("NAME").unwrap_or_else(|| os.clone()),
        os_version: os_release("VERSION").unwrap_or_default(),
        os,
        kernel: system::run("uname", &["-r"], Duration::from_secs(3)).unwrap_or_else(|e| e.0),
        processor: processor(),
        memory: memory(),
        graphics: graphics(),
        display: display(),
        uptime: uptime(),
        disk: disk(),
        gnoblin: gnoblin_version(),
    };
    #[cfg(feature = "snapshot")]
    let facts = fake_machine(facts);
    facts
}

/// Snapshot builds only: `BINGUX_SETTINGS_FAKE_ABOUT` fixes the two facts that change between
/// two runs on one machine, the uptime and how full the root file system is, so that two
/// captures of the page (left to right and right to left, say) are the same.
#[cfg(feature = "snapshot")]
fn fake_machine(mut facts: Facts) -> Facts {
    if std::env::var_os("BINGUX_SETTINGS_FAKE_ABOUT").is_some() {
        facts.uptime = "3 days, 4 hours".to_string();
        facts.disk = Some((124_000_000_000, 500_000_000_000));
    }
    facts
}

/// The rows of the System details group, in reading order.
fn system_specs(facts: &Facts) -> Vec<(&'static str, String)> {
    vec![
        ("Operating system", facts.os.clone()),
        ("Version", facts.os_version.clone()),
        ("Kernel", facts.kernel.clone()),
        ("Processor", facts.processor.clone()),
        ("Memory", facts.memory.clone()),
        ("Graphics", facts.graphics.clone()),
        ("Display", facts.display.clone()),
        ("Uptime", facts.uptime.clone()),
        ("Desktop", facts.gnoblin.clone()),
        ("Settings version", env!("CARGO_PKG_VERSION").to_string()),
    ]
}

/// Every fact as plain text, for the clipboard.
fn summary(facts: &Facts, name: &str) -> String {
    let mut lines = vec![
        format!("Device name: {name}"),
        format!("Hardware model: {}", facts.model),
    ];
    lines.extend(
        system_specs(facts)
            .into_iter()
            .map(|(title, value)| format!("{title}: {value}")),
    );
    match facts.disk {
        Some((used, total)) => lines.push(format!(
            "Storage: {} used of {}",
            size_text(used),
            size_text(total)
        )),
        None => lines.push("Storage: Unknown".to_string()),
    }
    lines.join("\n")
}

/// Copies `text` and says so, or says that it could not.
fn copy(text: String, overlays: Overlays) {
    match freya::clipboard::Clipboard::set(text) {
        Ok(()) => overlays.toast("Copied."),
        Err(_) => overlays.toast("The text could not be copied."),
    }
}

/// Stands in for a value that has not arrived, so its line keeps its height.
fn or_blank(text: String) -> String {
    if text.is_empty() {
        "\u{a0}".to_string()
    } else {
        text
    }
}

/// A titled section: its name above its body, with the name on the card edge.
fn section(tokens: &Tokens, title: &str, body: impl IntoElement) -> Element {
    rect()
        .tw(tokens, &[flex_col(), w_full(), gap(S2)])
        .child(section_label(title))
        .child(body)
        .into()
}

/// The device name sheet's state. It is `Copy`, so the sheet body and its
/// primary button share one set of fields.
#[derive(Clone, Copy, PartialEq)]
struct RenameSheet {
    draft: State<String>,
    /// The problem shown inside the sheet.
    error: State<Option<String>>,
    /// The name the hero shows once a rename is submitted. It is set before
    /// the system answers, and put back if the change fails.
    shown: State<Option<String>>,
    model: State<Option<Facts>>,
    overlays: Overlays,
}

impl RenameSheet {
    /// Opens the sheet with `name` in its field.
    fn open(self, name: String, window: Window) {
        let mut draft = self.draft;
        let mut error = self.error;
        draft.set(name);
        error.set(None);
        self.show(window);
    }

    fn show(self, window: Window) {
        window.open(popup("Device name", RenameBody { sheet: self }).flags(PopupFlags::FORM));
    }

    /// Checks the name, then writes it in the background. The hero shows the
    /// new name at once; a failure puts the old one back and says why.
    fn submit(self, window: Window) {
        let name = self.draft.read().trim().to_string();
        let mut error = self.error;
        if let Err(problem) = valid_hostname(&name) {
            error.set(Some(problem.to_string()));
            return;
        }
        let previous = self
            .shown
            .read()
            .clone()
            .or_else(|| {
                self.model
                    .read()
                    .as_ref()
                    .map(|facts| facts.hostname.clone())
            })
            .unwrap_or_default();
        window.close();
        error.set(None);
        system::optimistic(
            self.shown,
            Some(self.overlays),
            Some(previous),
            Some(name.clone()),
            move || set_hostname(&name).map(|_| ()),
        );
    }
}

/// A device name is a DNS label: letters, digits and hyphens, at most 63
/// characters, not starting or ending with a hyphen.
fn valid_hostname(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        Err("Enter a device name.")
    } else if name.len() > 63 {
        Err("The device name is longer than 63 characters.")
    } else if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || name.starts_with('-')
        || name.ends_with('-')
    {
        Err("Use only letters, numbers and hyphens, and do not start or end with a hyphen.")
    } else {
        Ok(())
    }
}

/// Sets the static host name through systemd, which asks polkit to allow it.
fn set_hostname(name: &str) -> CommandResult {
    system::run("hostnamectl", &["set-hostname", name], WRITE_TIMEOUT)
}

/// The sheet's name field and the error line under it.
#[derive(Clone, PartialEq)]
struct RenameBody {
    sheet: RenameSheet,
}

impl Component for RenameBody {
    fn render(&self) -> impl IntoElement {
        let sheet = self.sheet;
        let window = use_window();
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Rename".into(),
                enabled: true,
                on_press: EventHandler::new(move |_: ()| sheet.submit(window)),
            }),
        });
        let field = text_field(sheet.draft, move |_: String| sheet.submit(window));
        let error = sheet.error.read().clone();
        popup_column()
            .child(
                Group::new(None::<String>)
                    .row(form_row("Device name", field))
                    .footer(
                        "Other devices on your network use this name. Letters, numbers and hyphens, up to 63 characters.",
                    ),
            )
            .child(reveal_block_if(
                S6,
                error.map(|text| Element::from(status(Status::Error(text)))),
            ))
    }
}

#[derive(PartialEq)]
pub struct AboutPage;

impl Component for AboutPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let window = use_window();
        let model = system::use_cached_saved("about", collect);
        let shown = use_state(|| None::<String>);
        let draft = use_state(String::new);
        let error = use_state(|| None::<String>);
        let rename = RenameSheet {
            draft,
            error,
            shown,
            model,
            overlays,
        };
        let loaded = model.read().clone();
        use_page_busy(loaded.is_none());
        // Until the facts arrive every value is blank (and the page is hidden, see
        // the end of this function).
        let arrived = loaded.is_some();
        let facts = loaded.unwrap_or_default();
        let name = shown
            .read()
            .clone()
            .unwrap_or_else(|| facts.hostname.clone());
        let copy_text = summary(&facts, &name);
        let os_line = [facts.os_name.as_str(), facts.os_version.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        // The device card: the icon with the name, the operating system and the
        // model beside it, then the two actions in one row under them. The
        // actions sit on their own row so they never push past the card edge.
        let hero = rect().tw(&tokens, BOX).child(
            rect()
                .tw(&tokens, &[flex_col(), gap(GUTTER), p(GUTTER)])
                .child(
                    rect()
                        // Start, not centre: in this row (an inner height, in the page's scroll view) torin
                        // centred the icon tile in the whole height of the page, and it lay 738 px (773 on a
                        // phone) below its row, so the logo was never drawn. Layout lint: outside-parent.
                        .tw(&tokens, &[flex_row(), items(Align::Start), gap(GUTTER)])
                        .child(
                            rect()
                                .tw(
                                    &tokens,
                                    &[
                                        w(Dim::Box24),
                                        h(Dim::Box24),
                                        rounded(Radius::Tile),
                                        bg(Paint::Bg),
                                        border(Line::Hairline),
                                        flex_col(),
                                        items(Align::Center),
                                        justify(Align::Center),
                                        clip(),
                                    ],
                                )
                                .child(icon(
                                    Icon::Computer,
                                    IconSize::Hero,
                                    Paint::Text.color(&tokens),
                                )),
                        )
                        .child(
                            rect()
                                .tw(&tokens, &[w_flex(), flex_col(), gap(S1)])
                                // The name, the system and the model wrap, never
                                // end in an ellipsis, and can be selected.
                                .child(Element::from(
                                    bingux_ui::controls::select_text(or_blank(name.clone()))
                                        .style(TITLE),
                                ))
                                .child(Element::from(
                                    bingux_ui::controls::select_text(or_blank(os_line))
                                        .style(EMPHASIS),
                                ))
                                .child(Element::from(
                                    bingux_ui::controls::select_text(or_blank(facts.model.clone()))
                                        .style(CAPTION.in_ink(Ink::Muted)),
                                )),
                        ),
                )
                .child(
                    rect()
                        .tw(&tokens, &[flex_row(), gap(S2)])
                        .child(action_button("Rename", move |_| {
                            rename.open(name.clone(), window)
                        }))
                        .child(
                            action_button("Copy system info", move |_| {
                                copy(copy_text.clone(), overlays)
                            })
                            .suggested(),
                        ),
                ),
        );

        let storage = {
            let (value, unit, total, fraction, capacity) = match facts.disk {
                Some((used, total)) => {
                    let (value, unit) = size_parts(used);
                    (
                        value,
                        format!("{unit} used"),
                        format!("{} total", size_text(total)),
                        used as f64 / total.max(1) as f64,
                        format!("{} used of {}", size_text(used), size_text(total)),
                    )
                }
                // Once the facts have arrived, a disk that could not be read says so.
                None if arrived => (
                    "Unknown".to_string(),
                    String::new(),
                    String::new(),
                    0.0,
                    "Unknown".to_string(),
                ),
                None => (
                    "\u{a0}".to_string(),
                    String::new(),
                    String::new(),
                    0.0,
                    String::new(),
                ),
            };
            let copied = capacity.clone();
            let capacity_row =
                property_row("Capacity", or_blank(capacity))
                    .context_menu(vec![menu_item("Copy", move |_| {
                        copy(copied.clone(), overlays)
                    })]);
            rect()
                .tw(&tokens, &[flex_col(), gap(GUTTER)])
                .child(
                    rect()
                        .tw(&tokens, BOX)
                        .child(usage_meter(value, unit, total, fraction)),
                )
                .child(Group::new(None).rows(std::iter::once(capacity_row.into())))
        };

        let details = system_specs(&facts).into_iter().map(|(title, value)| {
            let copied = value.clone();
            property_row(title, or_blank(value))
                .context_menu(vec![menu_item("Copy", move |_| {
                    copy(copied.clone(), overlays)
                })])
                .into()
        });

        // How many lines each value takes (the name, the model, a processor, a graphics
        // card) is the machine's: on a phone they wrap, and a blank value cannot say how.
        // So with no saved facts the page is hidden, and fades in at its final layout
        // when they land; with saved facts it shows at once (see `loading::revealed`).
        revealed(
            bingux_ui::stage::staged(
                page_column()
                    .child(hero)
                    .child(section(
                        &tokens,
                        "System details",
                        Group::new(None).rows(details),
                    ))
                    .child(section(&tokens, "Storage", storage)),
            ),
            !arrived,
        )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_saved("about", collect);
}
