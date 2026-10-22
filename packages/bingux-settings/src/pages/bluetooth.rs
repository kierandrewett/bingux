//! Bluetooth: the adapter's power and discoverability, the devices BlueZ
//! knows about, and a timed scan for nearby devices, all through
//! `bluetoothctl`. Everything is read in the background and refreshed every
//! five seconds while no action is running.

use std::time::Duration;

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::routes::{Route, use_navigator};
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{
    Group, Overlays, ResponseKind, Row, Status, action_button, centered_caption, dialog, heading,
    page_column, primary_button, row, row_button, skeleton, status, switch_row, use_overlays,
    use_tokens,
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
    trusted: bool,
    battery: Option<u8>,
    /// The freedesktop icon name BlueZ reports, such as `audio-headset`.
    icon: String,
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
    field(text, key).map(|value| value == "yes").ok_or_else(|| {
        CommandError(format!(
            "Bluetooth did not report its {} state.",
            key.to_lowercase()
        ))
    })
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
            trusted: flag(&info, "Trusted").unwrap_or(false),
            icon: field(&info, "Icon").unwrap_or_default().to_string(),
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
        let scan = row("Scan for devices");
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

/// The bundled glyph for the icon name BlueZ reports.
fn device_icon(name: &str) -> Icon {
    match name {
        "audio-headphones" | "audio-headset" => Icon::Headphones,
        "audio-card" | "audio-speakers" => Icon::Speakers,
        "input-keyboard" => Icon::Keyboard,
        "input-mouse" | "input-tablet" => Icon::Mouse,
        "input-gaming" => Icon::Gamepad,
        "phone" => Icon::Phone,
        "computer" => Icon::Computer,
        "video-display" => Icon::Display,
        _ => Icon::Bluetooth,
    }
}

/// What kind of device the icon name stands for, in words.
fn device_kind(name: &str) -> &'static str {
    match name {
        "audio-headphones" | "audio-headset" => "Headphones",
        "audio-card" | "audio-speakers" => "Speakers",
        "input-keyboard" => "Keyboard",
        "input-mouse" => "Mouse",
        "input-tablet" => "Tablet",
        "input-gaming" => "Game controller",
        "phone" => "Phone",
        "computer" => "Computer",
        "video-display" => "Display",
        _ => "Other",
    }
}

/// The accent action a device allows: Pair for an unpaired one, Connect for
/// a paired one and Disconnect for a connected one. Pairing that needs a
/// passkey fails with BlueZ's message because bluetoothctl has no agent
/// here. Nothing is shown while another action runs or Bluetooth is off.
fn device_action(device: &Device, ctx: Ctx) -> Option<Element> {
    if !(ctx.idle && ctx.powered) {
        return None;
    }
    let mac = device.mac.clone();
    let name = device.name.clone();
    Some(if device.connected {
        action_button("Disconnect", move |_| {
            perform(
                ctx,
                format!("Disconnecting from {name}…"),
                args(&["disconnect", mac.as_str()]),
                TIMEOUT,
            )
        })
        .compact()
        .into()
    } else if device.paired {
        primary_button("Connect", move |_| {
            perform(
                ctx,
                format!("Connecting to {name}…"),
                args(&["connect", mac.as_str()]),
                CONNECT_TIMEOUT,
            )
        })
        .compact()
        .into()
    } else {
        primary_button("Pair", move |_| {
            perform(
                ctx,
                format!("Pairing with {name}…"),
                args(&["--timeout", "30", "pair", mac.as_str()]),
                CONNECT_TIMEOUT,
            )
        })
        .compact()
        .into()
    })
}

/// Ask before removing a device, then remove it.
fn confirm_remove(device: &Device, ctx: Ctx) {
    let name = device.name.clone();
    let mac = device.mac.clone();
    ctx.overlays.show_dialog(
        dialog(
            format!("Remove \"{name}\"?"),
            "The pairing is removed from this computer. Pair again to use it.",
        )
        .cancel("Cancel")
        .response("Remove", ResponseKind::Destructive, move |_| {
            perform(
                ctx,
                format!("Removing {name}…"),
                args(&["remove", mac.as_str()]),
                TIMEOUT,
            )
        }),
    );
}

/// One device in the list: its icon, name, status and address, the one
/// action it allows, and a chevron to its page, where it can be removed.
fn device_row(device: &Device, ctx: Ctx, open: impl Fn() + 'static) -> Row {
    row(device.name.clone())
        .leading(device_icon(&device.icon))
        .subtitle(format!("{} · {}", device_status(device), device.mac))
        .maybe_trailing(device_action(device, ctx))
        .navigates()
        .on_press(move |_| open())
}

/// Read BlueZ now and every five seconds while no action is running.
fn use_polling(model: State<Loaded>, busy: State<Option<String>>) {
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
}

/// A list row that opens the device's page.
fn open_row(device: &Device, ctx: Ctx, navigator: crate::routes::Navigator) -> Row {
    let route = Route::Device {
        mac: device.mac.clone(),
        name: device.name.clone(),
    };
    device_row(device, ctx, move || navigator.push(route.clone()))
}

#[derive(PartialEq)]
pub struct BluetoothPage;

impl Component for BluetoothPage {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let navigator = use_navigator();
        let model = system::cached("bluetooth");
        let error = use_state::<Option<String>>(|| None);
        let busy = use_state::<Option<String>>(|| None);
        use_polling(model, busy);

        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
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
                            .map(|device| Element::from(open_row(device, ctx, navigator))),
                    ),
                );
            }
            if !other.is_empty() {
                page = page.child(
                    Group::titled("Other devices").rows(
                        other
                            .into_iter()
                            .map(|device| Element::from(open_row(device, ctx, navigator))),
                    ),
                );
            }
        }
        page
    }
}

/// One device: its icon, name and status, the action it allows, what BlueZ
/// reports about it, and Remove.
#[derive(PartialEq)]
pub struct BluetoothDevicePage {
    pub mac: String,
}

impl Component for BluetoothDevicePage {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let navigator = use_navigator();
        let tokens = use_tokens();
        let model = system::cached("bluetooth");
        let error = use_state::<Option<String>>(|| None);
        let busy = use_state::<Option<String>>(|| None);
        use_polling(model, busy);

        // Go back to the list once BlueZ no longer knows the device.
        let mac = self.mac.clone();
        use_side_effect(move || {
            if let Some(Ok(data)) = &*model.read()
                && !data.devices.iter().any(|device| device.mac == mac)
            {
                navigator.pop();
            }
        });

        let Some(Ok(data)) = model.read().clone() else {
            return page_column().child(skeleton());
        };
        let Some(device) = data.devices.iter().find(|device| device.mac == self.mac) else {
            return page_column();
        };
        let busy_message = busy.read().clone();
        let ctx = Ctx {
            model,
            error,
            busy,
            powered: data.adapter.as_ref().is_some_and(|adapter| adapter.powered),
            idle: busy_message.is_none(),
            overlays,
        };

        let identity = rect()
            .width(Size::fill())
            .cross_align(Alignment::Center)
            .spacing(S2)
            .padding((S2, 0.0, S2, 0.0))
            .child(icons::icon(
                device_icon(&device.icon),
                ICON_XL,
                tokens.muted,
            ))
            .child(heading(device.name.clone()))
            .child(centered_caption(device_status(device)))
            .maybe_child(device_action(device, ctx));

        let yes_no = |value: bool| if value { "Yes" } else { "No" };
        let mut details = Group::titled("Details")
            .row(row("Type").value(device_kind(&device.icon)))
            .row(row("Address").value(device.mac.clone()))
            .row(row("Paired").value(yes_no(device.paired)))
            .row(row("Trusted").value(yes_no(device.trusted)));
        if let Some(level) = device.battery {
            details = details.row(row("Battery").value(format!("{level}%")));
        }

        let removable = device.clone();
        let mut page = page_column();
        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }
        if let Some(message) = busy_message {
            page = page.child(status(Status::Loading(message)));
        }
        page.child(identity).child(details).child(
            Group::new(None::<String>).row(
                row("Remove device")
                    .destructive()
                    .on_press(move |_| confirm_remove(&removable, ctx)),
            ),
        )
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("bluetooth", load);
}
