//! About: this machine, the operating system, and the Bingux and Gnoblin
//! versions. Everything is read once in the background when the page opens.

use std::time::Duration;

use bingux_ui::style::*;
use freya::prelude::*;

use crate::system;
use crate::ui::{
    Group, app_icon, centered_caption, expander_row, heading, page_column, property_row,
    usage_meter, use_page_busy, vstack,
};

#[derive(Clone, Default, PartialEq)]
struct Facts {
    hostname: String,
    os: String,
    kernel: String,
    hardware: String,
    processor: String,
    memory: String,
    /// Bytes used and bytes in total on the root file system.
    disk: Option<(u64, u64)>,
    gnoblin: String,
}

fn os_release_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("PRETTY_NAME=")
                    .map(|value| value.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

fn dmi(name: &str) -> Option<String> {
    std::fs::read_to_string(format!("/sys/devices/virtual/dmi/id/{name}"))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| {
            !text.is_empty() && text != "To Be Filled By O.E.M." && text != "Default string"
        })
}

fn processor() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
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
    std::fs::read_to_string("/proc/meminfo")
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
    let hardware = match (dmi("sys_vendor"), dmi("product_name")) {
        (Some(vendor), Some(product)) => format!("{vendor} {product}"),
        (vendor, product) => vendor.or(product).unwrap_or_else(|| "Unknown".to_string()),
    };
    Facts {
        hostname: std::fs::read_to_string("/etc/hostname")
            .map(|text| text.trim().to_string())
            .ok()
            .filter(|text| !text.is_empty())
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "localhost".to_string()),
        os: os_release_name(),
        kernel: system::run("uname", &["-r"], Duration::from_secs(3)).unwrap_or_else(|e| e.0),
        hardware,
        processor: processor(),
        memory: memory(),
        disk: disk(),
        gnoblin: gnoblin_version(),
    }
}

#[derive(PartialEq)]
pub struct AboutPage;

impl Component for AboutPage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached("about", collect);
        let loaded = loaded.read().clone();
        use_page_busy(loaded.is_none());
        // Until the facts arrive every value is blank. Rows keep their height,
        // and the header lines keep a blank line box, so nothing moves when the
        // values fill in.
        let facts = loaded.unwrap_or_default();
        let header_line = |text: String| {
            if text.is_empty() {
                "\u{a0}".to_string()
            } else {
                text
            }
        };

        // A blank meter keeps the room until the figures arrive.
        let storage = {
            let (value, unit, total, fraction) = match facts.disk {
                Some((used, total)) => {
                    let (value, unit) = size_parts(used);
                    let (total_value, total_unit) = size_parts(total);
                    (
                        value,
                        format!("{unit} used"),
                        format!("{total_value} {total_unit} total"),
                        used as f64 / total.max(1) as f64,
                    )
                }
                None => ("\u{a0}".to_string(), String::new(), String::new(), 0.0),
            };
            Group::titled("Storage").row(usage_meter(value, unit, total, fraction))
        };

        page_column()
            .child(
                vstack(S2)
                    .tw_layout(&[w_full(), items(Align::Center), py(S2)])
                    .child(app_icon("bingux-settings", IconSize::AppLarge))
                    .child(heading(header_line(facts.os.clone())))
                    .child(centered_caption(header_line(facts.hostname.clone()))),
            )
            .child(
                Group::titled("This device")
                    .row(property_row("Device name", facts.hostname))
                    .row(property_row("Hardware model", facts.hardware))
                    .row(property_row("Processor", facts.processor))
                    .row(property_row("Memory", facts.memory)),
            )
            .child(storage)
            .child(
                Group::titled("Software")
                    .row(property_row("Operating system", facts.os))
                    .row(property_row("Bingux version", env!("CARGO_PKG_VERSION")))
                    .row(expander_row(
                        "Technical details",
                        vec![
                            property_row("Kernel", facts.kernel).into(),
                            property_row("Desktop version", facts.gnoblin).into(),
                            property_row("Display server", "Wayland").into(),
                        ],
                    )),
            )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("about", collect);
}
