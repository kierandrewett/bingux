//! Bluetooth: the adapter's power and discoverability, the devices BlueZ
//! knows about, and a timed scan for nearby devices, all through
//! `bluetoothctl`. Everything is read in the background and refreshed every
//! five seconds while no action is running.

use std::time::Duration;

use freya::prelude::*;

use crate::system::{self, CommandError, background, background_then};
use crate::theme::Tokens;
use crate::ui::{
    Group, Overlays, ResponseKind, Row, Status, dialog, page_column, row, row_button, status,
    switch_row, use_overlays, use_tokens,
};
use bingux_ui::scale::*;

const TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(45);
const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_secs(5);

#[derive(Clone, PartialEq)]
struct Adapter {
    powered: bool,
    discoverable: bool,
}

#[derive(Clone, PartialEq)]
struct Device {
    mac: String,
    name: String,
    paired: bool,
    connected: bool,
    battery: Option<u8>,
}

#[derive(Clone, PartialEq)]
struct Model {
    /// `None` when BlueZ has no controller.
    adapter: Option<Adapter>,
    devices: Vec<Device>,
}

type Loaded = Option<Result<Model, CommandError>>;

/// Everything the event handlers need. All of it is `Copy`.
#[derive(Clone, Copy)]
struct Ctx {
    model: State<Loaded>,
    error: State<Option<String>>,
    busy: State<Option<String>>,
    tokens: Tokens,
    powered: bool,
    /// No action is running, so the action buttons may be shown.
    idle: bool,
    overlays: Overlays,
}

fn bluetoothctl(args: &[&str], timeout: Duration) -> Result<String, CommandError> {
    system::run("bluetoothctl", args, timeout).map_err(|error| {
        // Without bluetoothd, bluetoothctl aborts on a D-Bus assertion and
        // prints a build-flag complaint that means nothing to a user.
        if error.0.contains("D-Bus") || error.0.contains("rdynamic") {
            CommandError("The Bluetooth service (bluetoothd) is not running.".into())
        } else {
            error
        }
    })
}

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

/// The value of `key: value` in a `bluetoothctl` report, trimmed.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.trim().split_once(':')?;
        (name.trim() == key).then(|| value.trim())
    })
}

fn flag(text: &str, key: &str) -> Result<bool, CommandError> {
    field(text, key)
        .map(|value| value == "yes")
        .ok_or_else(|| CommandError(format!("bluetoothctl did not report {key}.")))
}

/// `Battery Percentage` reads `0x5a (90)`; the percentage is in brackets.
fn battery(value: &str) -> Option<u8> {
    value
        .split('(')
        .nth(1)?
        .trim_end_matches(')')
        .trim()
        .parse()
        .ok()
}

/// Whether the kernel has a Bluetooth controller. Without one, and without
/// bluetoothd, `bluetoothctl` waits for its whole timeout, so check first.
fn has_controller() -> bool {
    std::fs::read_dir("/sys/class/bluetooth").is_ok_and(|mut entries| entries.next().is_some())
}

fn load() -> Result<Model, CommandError> {
    if !has_controller() {
        return Ok(Model {
            adapter: None,
            devices: Vec::new(),
        });
    }
    if bluetoothctl(&["list"], TIMEOUT)?.trim().is_empty() {
        return Ok(Model {
            adapter: None,
            devices: Vec::new(),
        });
    }
    let show = bluetoothctl(&["show"], TIMEOUT)?;
    let adapter = Adapter {
        powered: flag(&show, "Powered")?,
        discoverable: flag(&show, "Discoverable")?,
    };

    let listing = bluetoothctl(&["devices"], TIMEOUT)?;
    let mut devices = Vec::new();
    for line in listing.lines() {
        let Some(rest) = line.trim().strip_prefix("Device ") else {
            continue;
        };
        let (mac, known_name) = rest.split_once(' ').unwrap_or((rest, ""));
        let info = bluetoothctl(&["info", mac], TIMEOUT)?;
        devices.push(Device {
            mac: mac.to_string(),
            name: field(&info, "Alias").unwrap_or(known_name).to_string(),
            paired: flag(&info, "Paired")?,
            connected: flag(&info, "Connected")?,
            battery: field(&info, "Battery Percentage").and_then(battery),
        });
    }
    devices.sort_by_key(|device| device.name.to_lowercase());

    Ok(Model {
        adapter: Some(adapter),
        devices,
    })
}

/// Run one `bluetoothctl` action in the background, then show its error (or
/// clear it) and reload the model so the page shows what BlueZ now reports.
/// A second action is refused while one is still running.
fn perform(ctx: Ctx, label: String, arguments: Vec<String>, timeout: Duration) {
    if ctx.busy.read().is_some() {
        return;
    }
    let Ctx {
        mut busy,
        mut error,
        model,
        ..
    } = ctx;
    busy.set(Some(label));
    background_then(
        move || {
            let refs: Vec<&str> = arguments.iter().map(String::as_str).collect();
            bluetoothctl(&refs, timeout)
        },
        move |result| {
            busy.set(None);
            match result {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e.0)),
            }
            background(model, load);
        },
    );
}

fn adapter_group(adapter: &Adapter, ctx: Ctx) -> Group {
    let mut group = Group::titled("Adapter").row(switch_row(
        "Bluetooth",
        adapter.powered,
        move |next: bool| {
            let (label, word) = if next {
                ("Turning Bluetooth on…", "on")
            } else {
                ("Turning Bluetooth off…", "off")
            };
            perform(ctx, label.to_string(), args(&["power", word]), TIMEOUT);
        },
    ));
    if adapter.powered {
        group = group.row(switch_row(
            "Discoverable",
            adapter.discoverable,
            move |next: bool| {
                let word = if next { "on" } else { "off" };
                perform(
                    ctx,
                    "Changing discoverability…".to_string(),
                    args(&["discoverable", word]),
                    TIMEOUT,
                );
            },
        ));
        let scan = row("Scan for devices").subtitle("Looks for nearby devices for 8 seconds.");
        let scan = if ctx.idle {
            scan.trailing(row_button("Scan", move |_| {
                perform(
                    ctx,
                    "Scanning for nearby devices…".to_string(),
                    args(&["--timeout", "8", "scan", "on"]),
                    SCAN_TIMEOUT,
                )
            }))
        } else {
            scan
        };
        group = group.row(scan);
    }
    group
}

fn device_status(device: &Device) -> String {
    let state = if device.connected {
        "Connected"
    } else if device.paired {
        "Paired"
    } else {
        "Not paired"
    };
    match device.battery {
        Some(level) => format!("{state} · {level}% battery"),
        None => state.to_string(),
    }
}

/// One device: name and MAC, with its status and the actions it allows.
/// Pair applies to unpaired devices, Connect to paired ones; Remove works on
/// any device BlueZ knows. Pairing that needs a passkey fails with BlueZ's
/// message because bluetoothctl has no agent here.
fn device_row(device: &Device, ctx: Ctx) -> Row {
    let disconnect_mac = device.mac.clone();
    let disconnect_name = device.name.clone();
    let connect_mac = device.mac.clone();
    let connect_name = device.name.clone();
    let remove_mac = device.mac.clone();
    let remove_name = device.name.clone();
    let pair_mac = device.mac.clone();
    let pair_name = device.name.clone();

    let trailing = rect()
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .spacing(S2)
        .child(
            label()
                .text(device_status(device))
                .color(ctx.tokens.muted)
                .font_size(TEXT_MD)
                .max_lines(1),
        )
        .maybe_child((ctx.idle && ctx.powered && device.connected).then(|| {
            row_button("Disconnect", move |_| {
                perform(
                    ctx,
                    format!("Disconnecting from {disconnect_name}…"),
                    args(&["disconnect", disconnect_mac.as_str()]),
                    TIMEOUT,
                )
            })
        }))
        .maybe_child(
            (ctx.idle && ctx.powered && device.paired && !device.connected).then(|| {
                row_button("Connect", move |_| {
                    perform(
                        ctx,
                        format!("Connecting to {connect_name}…"),
                        args(&["connect", connect_mac.as_str()]),
                        CONNECT_TIMEOUT,
                    )
                })
            }),
        )
        .maybe_child((ctx.idle && ctx.powered && !device.paired).then(|| {
            row_button("Pair", move |_| {
                perform(
                    ctx,
                    format!("Pairing with {pair_name}…"),
                    args(&["--timeout", "30", "pair", pair_mac.as_str()]),
                    CONNECT_TIMEOUT,
                )
            })
        }))
        .maybe_child(ctx.idle.then(|| {
            row_button("Remove", move |_| {
                let remove_name = remove_name.clone();
                let remove_mac = remove_mac.clone();
                ctx.overlays.show_dialog(
                    dialog(
                        format!("Remove \"{remove_name}\"?"),
                        "The pairing is removed from this computer. Pair again to use it.",
                    )
                    .cancel("Cancel")
                    .response("Remove", ResponseKind::Destructive, move |_| {
                        perform(
                            ctx,
                            format!("Removing {remove_name}…"),
                            args(&["remove", remove_mac.as_str()]),
                            TIMEOUT,
                        )
                    }),
                )
            })
        }));

    row(device.name.clone())
        .subtitle(device.mac.clone())
        .trailing(trailing)
}

#[derive(PartialEq)]
pub struct BluetoothPage;

impl Component for BluetoothPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let model = use_state::<Loaded>(|| None);
        let error = use_state::<Option<String>>(|| None);
        let busy = use_state::<Option<String>>(|| None);

        use_hook(move || {
            background(model, load);
            spawn(async move {
                loop {
                    timer(POLL).await;
                    if busy.read().is_none() {
                        background(model, load);
                    }
                }
            });
        });

        let Some(loaded) = model.read().clone() else {
            return page_column()
                .child(status(Status::Loading("Reading Bluetooth devices.".into())));
        };
        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                return page_column().child(
                    status(Status::Error(e.0))
                        .action("Try again", move |_| background(model, load)),
                );
            }
        };
        let Some(adapter) = data.adapter else {
            return page_column()
                .child(status(Status::Empty("No Bluetooth adapter found.".into())));
        };

        let busy_message = busy.read().clone();
        let ctx = Ctx {
            model,
            error,
            busy,
            tokens,
            powered: adapter.powered,
            idle: busy_message.is_none(),
            overlays,
        };

        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }
        if let Some(message) = busy_message {
            page = page.child(status(Status::Loading(message)));
        }
        page = page.child(adapter_group(&adapter, ctx));
        if !adapter.powered {
            page = page.child(status(Status::Info(
                "Turn Bluetooth on to connect devices.".into(),
            )));
        }

        if data.devices.is_empty() {
            page = page.child(status(Status::Empty(
                "No devices known yet. Scan to find nearby devices.".into(),
            )));
        } else {
            let (paired, other): (Vec<&Device>, Vec<&Device>) =
                data.devices.iter().partition(|device| device.paired);
            if !paired.is_empty() {
                page = page.child(
                    Group::titled("My devices").rows(
                        paired
                            .into_iter()
                            .map(|device| Element::from(device_row(device, ctx))),
                    ),
                );
            }
            if !other.is_empty() {
                page = page.child(
                    Group::titled("Other devices").rows(
                        other
                            .into_iter()
                            .map(|device| Element::from(device_row(device, ctx))),
                    ),
                );
            }
        }
        page
    }
}
