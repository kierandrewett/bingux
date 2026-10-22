//! Network: the wired connections NetworkManager manages. Everything comes
//! from `nmcli`.

use std::time::Duration;

use freya::prelude::*;

use crate::icons::Icon;
use crate::system::{self, CommandError};
use crate::ui::{Group, Status, page_column, row, skeleton, status};

const QUICK: Duration = Duration::from_secs(5);

#[derive(Clone, PartialEq)]
struct WiredDevice {
    name: String,
    state: String,
    connection: String,
}

type Loaded = Result<Vec<WiredDevice>, CommandError>;

/// Split one `nmcli -t` line on its unescaped colons, undoing `\:` and `\\`.
fn split_terse(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ':' => fields.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    fields.push(current);
    fields
}

/// Ethernet devices that NetworkManager manages.
fn parse_wired(output: &str) -> Vec<WiredDevice> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let fields = split_terse(line);
            let field = |index: usize| fields.get(index).cloned().unwrap_or_default();
            let kind = field(1);
            let state = field(2);
            if kind != "ethernet" || state == "unmanaged" {
                return None;
            }
            Some(WiredDevice {
                name: field(0),
                state,
                connection: field(3),
            })
        })
        .collect()
}

fn load() -> Loaded {
    system::run(
        "nmcli",
        &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"],
        QUICK,
    )
    .map(|text| parse_wired(&text))
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A device state as one plain word, such as "Connected" or "Not connected".
fn wired_state(state: &str) -> String {
    match state.split(' ').next().unwrap_or(state) {
        "disconnected" => "Not connected".to_string(),
        word => capitalise(word),
    }
}

#[derive(PartialEq)]
pub struct NetworkPage;

impl Component for NetworkPage {
    fn render(&self) -> impl IntoElement {
        let model = system::use_cached("network", load);
        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let wired = match loaded {
            Ok(wired) => wired,
            Err(failure) => {
                return page_column().child(
                    status(Status::Error(failure.0))
                        .action("Try again", move |_| system::background(model, load)),
                );
            }
        };
        let group = Group::titled("Wired");
        if wired.is_empty() {
            return page_column()
                .child(group.row(status(Status::Empty("No wired network devices.".into()))));
        }
        // The connection's name ("Wired connection 1") says more than the
        // interface name ("enp42s0"). Without a connection, say Ethernet.
        page_column().child(group.rows(wired.iter().enumerate().map(|(index, device)| {
            let title = if !device.connection.is_empty() && device.connection != "--" {
                device.connection.clone()
            } else if wired.len() > 1 {
                format!("Ethernet {}", index + 1)
            } else {
                "Ethernet".to_string()
            };
            Element::from(
                row(title)
                    .leading(Icon::NetworkWired)
                    .value(wired_state(&device.state)),
            )
        })))
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("network", load);
}
