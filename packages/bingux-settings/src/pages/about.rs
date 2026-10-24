//! About: this machine, the operating system, and the Bingux and Gnoblin
//! versions. Everything is read once in the background when the page opens.

use std::time::Duration;

use freya::prelude::*;

use crate::system::{self, background};
use crate::ui::{Group, Status, page_title, property_row, status};

#[derive(Clone, PartialEq)]
struct Facts {
    hostname: String,
    os: String,
    kernel: String,
    hardware: String,
    processor: String,
    memory: String,
    disk: String,
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

fn disk() -> String {
    system::run(
        "df",
        &["-h", "--output=size,used", "/"],
        Duration::from_secs(3),
    )
    .ok()
    .and_then(|text| {
        let mut fields = text.lines().nth(1)?.split_whitespace();
        Some(format!(
            "{} used of {}",
            fields.nth(1)?,
            text.lines().nth(1)?.split_whitespace().next()?
        ))
    })
    .unwrap_or_else(|| "Unknown".to_string())
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
        let facts = use_state::<Option<Facts>>(|| None);
        use_hook(move || background(facts, collect));

        let Some(facts) = facts.read().clone() else {
            return rect()
                .width(Size::fill())
                .child(page_title("About"))
                .child(status(Status::Loading(
                    "Reading system information.".into(),
                )));
        };

        rect()
            .width(Size::fill())
            .child(page_title("About"))
            .child(
                Group::titled("Device")
                    .row(property_row("Device name", facts.hostname))
                    .row(property_row("Hardware model", facts.hardware))
                    .row(property_row("Processor", facts.processor))
                    .row(property_row("Memory", facts.memory))
                    .row(property_row("Disk", facts.disk)),
            )
            .child(
                Group::titled("System")
                    .row(property_row("Operating system", facts.os))
                    .row(property_row("Kernel", facts.kernel))
                    .row(property_row("Windowing system", "Wayland"))
                    .row(property_row("Bingux", env!("CARGO_PKG_VERSION")))
                    .row(property_row("Gnoblin", facts.gnoblin)),
            )
    }
}
