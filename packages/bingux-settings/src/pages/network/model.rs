//! What every Network screen shares: one snapshot of NetworkManager (radios,
//! devices, VPN profiles, connectivity) that is read together and refreshed
//! every few seconds, the `nmcli` helpers, and the optimistic switch state.
//!
//! Screens read [`use_overview`] for the facts a row shows. A screen that needs
//! more (a Wi-Fi scan, one profile's settings) loads that itself.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::routes::{Navigator, use_navigator};
use crate::system::{self, CommandError, background_then};
use crate::ui::{Overlays, Tokens, Window, centered_caption, heading, use_overlays, use_window};
use bingux_ui::style::*;

pub const QUICK: Duration = Duration::from_secs(5);
pub const CONNECT: Duration = Duration::from_secs(60);
/// How often the overview is read again while a Network screen is open.
const POLL: Duration = Duration::from_secs(5);

/// Run `nmcli` with `args`.
pub fn nmcli(args: &[&str], timeout: Duration) -> Result<String, CommandError> {
    system::run("nmcli", args, timeout)
}

/// Split one `nmcli -t` line on its unescaped colons, undoing `\:` and `\\`.
pub fn split_terse(line: &str) -> Vec<String> {
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

/// One terse value with `\:` and `\\` undone.
pub fn unescape(value: &str) -> String {
    split_terse(value).join(":")
}

pub fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The kind of a network device, from NetworkManager's `GENERAL.TYPE`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum DeviceKind {
    Ethernet,
    Wifi,
    /// A mobile broadband modem (`gsm` or `cdma`).
    Modem,
    WireGuard,
    Tun,
    Other,
}

impl DeviceKind {
    fn from_type(kind: &str) -> Self {
        match kind {
            "ethernet" => Self::Ethernet,
            "wifi" => Self::Wifi,
            "gsm" | "cdma" => Self::Modem,
            "wireguard" => Self::WireGuard,
            "tun" | "ip-tunnel" => Self::Tun,
            _ => Self::Other,
        }
    }
}

/// One network device and what is live on it.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub name: String,
    pub kind: DeviceKind,
    /// `connected`, `disconnected`, `unavailable`, `connecting (...)`, ...
    pub state: String,
    /// The active connection's name; empty when there is none.
    pub connection: String,
    pub uuid: String,
    pub hwaddr: String,
    /// Link speed in Mbit/s, when the device reports one.
    pub speed: Option<u32>,
    /// Whether a cable is plugged in (wired devices); true when unknown.
    pub carrier: bool,
    pub mtu: Option<u32>,
    /// Addresses with their prefix, such as `192.168.0.72/24`.
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub gateway4: String,
    pub gateway6: String,
    pub dns: Vec<String>,
}

impl Device {
    pub fn connected(&self) -> bool {
        self.state.starts_with("connected")
    }

    pub fn has_connection(&self) -> bool {
        !self.connection.is_empty()
    }

    /// The first IPv4 address without its prefix, or an empty string.
    pub fn address4(&self) -> String {
        self.ipv4
            .first()
            .map(|address| address.split('/').next().unwrap_or(address).to_string())
            .unwrap_or_default()
    }
}

/// Which plugin runs a VPN profile.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum VpnKind {
    WireGuard,
    /// A NetworkManager VPN plugin, by service type such as
    /// `org.freedesktop.NetworkManager.openvpn`.
    Plugin(String),
}

/// One VPN profile.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vpn {
    pub name: String,
    pub uuid: String,
    pub kind: VpnKind,
    pub active: bool,
    pub activating: bool,
    /// The tunnel device while it is up.
    pub device: String,
    pub autoconnect: bool,
}

/// The state of every radio and link, read in one pass.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Overview {
    /// NetworkManager's global state: `connected`, `connecting`, `disconnected`, ...
    pub state: String,
    /// `full`, `limited`, `portal`, `none` or `unknown`.
    pub connectivity: String,
    /// `enabled`, `disabled` or `missing`, from `nmcli radio`.
    pub wifi_hw: String,
    pub wifi_on: bool,
    pub wwan_hw: String,
    pub wwan_on: bool,
    /// Every radio is blocked in software (rfkill).
    pub airplane: bool,
    pub devices: Vec<Device>,
    pub vpns: Vec<Vpn>,
}

impl Overview {
    pub fn wifi_present(&self) -> bool {
        self.wifi_hw != "missing"
    }

    /// Wi-Fi hardware exists and is not blocked.
    pub fn wifi_usable(&self) -> bool {
        self.wifi_hw != "missing" && self.wifi_hw != "disabled"
    }

    pub fn wifi_device(&self) -> Option<&Device> {
        self.devices
            .iter()
            .find(|device| device.kind == DeviceKind::Wifi)
    }

    pub fn ethernet_devices(&self) -> impl Iterator<Item = &Device> {
        self.devices
            .iter()
            .filter(|device| device.kind == DeviceKind::Ethernet)
    }

    pub fn modem_device(&self) -> Option<&Device> {
        self.devices
            .iter()
            .find(|device| device.kind == DeviceKind::Modem)
    }

    /// A modem is present when NetworkManager has a modem device or reports
    /// WWAN hardware.
    pub fn modem_present(&self) -> bool {
        self.modem_device().is_some() || self.wwan_hw != "missing"
    }

    /// The device the machine reaches the network through: the connected
    /// device with a default gateway, wired before Wi-Fi before mobile (the
    /// order NetworkManager's default route metrics give them).
    pub fn primary(&self) -> Option<&Device> {
        let rank = |kind: DeviceKind| match kind {
            DeviceKind::Ethernet => 0,
            DeviceKind::Wifi => 1,
            _ => 2,
        };
        self.devices
            .iter()
            .filter(|device| {
                device.connected()
                    && matches!(
                        device.kind,
                        DeviceKind::Ethernet | DeviceKind::Wifi | DeviceKind::Modem
                    )
                    && (!device.gateway4.is_empty() || !device.gateway6.is_empty())
            })
            .min_by_key(|device| rank(device.kind))
    }

    pub fn vpn(&self, name: &str) -> Option<&Vpn> {
        self.vpns.iter().find(|vpn| vpn.name == name)
    }
}

/// True when the kernel has every radio blocked in software: the airplane
/// state. Each radio reports a `soft` flag in `/sys/class/rfkill`; airplane
/// mode is on only when there is at least one radio and all of them are
/// blocked. Snapshot builds can fake it with `BINGUX_SETTINGS_FAKE_AIRPLANE`.
pub fn airplane_blocked() -> bool {
    #[cfg(feature = "snapshot")]
    if let Some(value) = std::env::var_os("BINGUX_SETTINGS_FAKE_AIRPLANE") {
        return value == "1";
    }
    let Ok(entries) = std::fs::read_dir("/sys/class/rfkill") else {
        return false;
    };
    let mut any = false;
    for entry in entries.flatten() {
        let soft = std::fs::read_to_string(entry.path().join("soft")).unwrap_or_default();
        any = true;
        if soft.trim() != "1" {
            return false;
        }
    }
    any
}

/// Turn airplane mode on or off. `rfkill`, not `nmcli radio all`: it blocks
/// and unblocks every radio the kernel knows about, which is what airplane
/// mode means.
pub fn set_airplane_mode(on: bool) -> Result<(), CommandError> {
    #[cfg(feature = "snapshot")]
    if std::env::var_os("BINGUX_SETTINGS_FAKE_AIRPLANE").is_some() || fake_scenario().is_some() {
        return Ok(());
    }
    system::run(
        "rfkill",
        &[if on { "block" } else { "unblock" }, "all"],
        QUICK,
    )
    .map(|_| ())
}

/// Turn the Wi-Fi radio on or off.
pub fn set_wifi_radio(on: bool) -> Result<(), CommandError> {
    #[cfg(feature = "snapshot")]
    if fake_scenario().is_some() || std::env::var_os("BINGUX_SETTINGS_FAKE_WIFI").is_some() {
        return Ok(());
    }
    nmcli(&["radio", "wifi", if on { "on" } else { "off" }], QUICK).map(|_| ())
}

/// Parse `nmcli -t -f <fields> device show` output. Each device's block
/// starts at its `GENERAL.DEVICE` line; the other keys are indexed (`IP4.DNS[2]`).
fn parse_devices(output: &str) -> Vec<Device> {
    let mut devices: Vec<Device> = Vec::new();
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = unescape(value);
        // `IP4.DNS[2]` and `IP4.DNS` are the same key.
        let key = key.split('[').next().unwrap_or(key);
        if key == "GENERAL.DEVICE" {
            devices.push(Device {
                name: value,
                kind: DeviceKind::Other,
                state: String::new(),
                connection: String::new(),
                uuid: String::new(),
                hwaddr: String::new(),
                speed: None,
                carrier: true,
                mtu: None,
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                gateway4: String::new(),
                gateway6: String::new(),
                dns: Vec::new(),
            });
            continue;
        }
        let Some(device) = devices.last_mut() else {
            continue;
        };
        let value = value.trim().to_string();
        match key {
            "GENERAL.TYPE" => device.kind = DeviceKind::from_type(&value),
            // `100 (connected)` -> `connected`.
            "GENERAL.STATE" => {
                device.state = value
                    .split_once(' ')
                    .map(|(_, word)| word.trim_matches(['(', ')']).to_string())
                    .unwrap_or(value);
            }
            "GENERAL.CONNECTION" if value != "--" => device.connection = value,
            "GENERAL.CON-UUID" if value != "--" => device.uuid = value,
            "GENERAL.HWADDR" if value != "--" => device.hwaddr = value,
            "GENERAL.MTU" => device.mtu = value.parse().ok(),
            "CAPABILITIES.SPEED" => {
                device.speed = value.split_whitespace().next().and_then(|n| n.parse().ok())
            }
            "WIRED-PROPERTIES.CARRIER" => device.carrier = value != "off",
            "IP4.ADDRESS" if !value.is_empty() => device.ipv4.push(value),
            "IP6.ADDRESS" if !value.is_empty() => device.ipv6.push(value),
            "IP4.GATEWAY" => device.gateway4 = value,
            "IP6.GATEWAY" => device.gateway6 = value,
            "IP4.DNS" | "IP6.DNS" if !value.is_empty() => device.dns.push(value),
            _ => {}
        }
    }
    // Loopback, bridges, veth pairs and the like are not connections a person
    // manages here.
    devices.retain(|device| device.kind != DeviceKind::Other);
    devices
}

/// VPN profiles from `connection show` output (`NAME,UUID,TYPE,AUTOCONNECT,ACTIVE,DEVICE,STATE`).
fn parse_vpns(output: &str, service: impl Fn(&str) -> String) -> Vec<Vpn> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let fields = split_terse(line);
            let field = |index: usize| fields.get(index).cloned().unwrap_or_default();
            let kind = match field(2).as_str() {
                "wireguard" => VpnKind::WireGuard,
                "vpn" => VpnKind::Plugin(service(&field(1))),
                _ => return None,
            };
            let state = field(6);
            Some(Vpn {
                name: field(0),
                uuid: field(1),
                kind,
                active: field(4) == "yes" && state != "activating",
                activating: state == "activating",
                device: field(5),
                autoconnect: field(3) == "yes",
            })
        })
        .collect()
}

/// A VPN profile's plugin service type, or an empty string.
fn vpn_service(uuid: &str) -> String {
    nmcli(
        &["-g", "vpn.service-type", "connection", "show", uuid],
        QUICK,
    )
    .map(|text| text.trim().to_string())
    .unwrap_or_default()
}

/// A snapshot build can stand in a whole network with
/// `BINGUX_SETTINGS_FAKE_NETWORK=<scenario>`: `online` (Wi-Fi connected, one
/// wired device connected, one VPN up), `wifi-off`, `airplane`, `no-wifi`,
/// `wifi-only`, `offline`, `portal` or `modem` (online plus a mobile modem). Nothing is read
/// from `nmcli`.
#[cfg(feature = "snapshot")]
pub fn fake_scenario() -> Option<String> {
    std::env::var("BINGUX_SETTINGS_FAKE_NETWORK").ok()
}

#[cfg(not(feature = "snapshot"))]
pub fn fake_scenario() -> Option<String> {
    None
}

#[cfg(feature = "snapshot")]
fn fake_overview(scenario: &str) -> Overview {
    let device = |name: &str, kind, connection: &str, ip: &str| Device {
        name: name.into(),
        kind,
        state: if connection.is_empty() {
            "disconnected".into()
        } else {
            "connected".into()
        },
        connection: connection.into(),
        uuid: if connection.is_empty() {
            String::new()
        } else {
            format!("00000000-0000-4000-8000-{:012}", name.len())
        },
        hwaddr: "3C:52:82:11:9A:F0".into(),
        speed: (kind == DeviceKind::Ethernet).then_some(1000),
        carrier: true,
        mtu: Some(1500),
        ipv4: if ip.is_empty() {
            Vec::new()
        } else {
            vec![format!("{ip}/24")]
        },
        ipv6: if ip.is_empty() {
            Vec::new()
        } else {
            vec!["fe80::31cc:ca7d:dd41:b7a5/64".into()]
        },
        gateway4: if ip.is_empty() {
            String::new()
        } else {
            "192.168.0.1".into()
        },
        gateway6: String::new(),
        dns: if ip.is_empty() {
            Vec::new()
        } else {
            vec!["1.1.1.1".into(), "9.9.9.9".into()]
        },
    };
    let wifi_on = !matches!(scenario, "wifi-off" | "airplane" | "no-wifi");
    let wired_up = !matches!(scenario, "offline" | "airplane" | "wifi-only");
    let mut devices = Vec::new();
    if scenario != "no-wifi" {
        let joined = wifi_on && scenario != "offline";
        devices.push(device(
            "wlo1",
            DeviceKind::Wifi,
            if joined { "Home" } else { "" },
            if joined { "192.168.0.41" } else { "" },
        ));
    }
    devices.push(device(
        "enp42s0",
        DeviceKind::Ethernet,
        if wired_up { "Wired connection 1" } else { "" },
        if wired_up { "192.168.0.72" } else { "" },
    ));
    if scenario == "modem" {
        devices.push(device("wwan0", DeviceKind::Modem, "Vodafone", "10.64.12.7"));
    }
    if !matches!(scenario, "offline" | "airplane") {
        devices.push(device(
            "wg0",
            DeviceKind::WireGuard,
            "Work WireGuard",
            "10.8.0.2",
        ));
    }
    let vpn = |name: &str, kind: VpnKind, active: bool| Vpn {
        name: name.into(),
        uuid: format!("11111111-1111-4111-8111-{:012}", name.len()),
        kind,
        active,
        activating: false,
        device: if active { "wg0".into() } else { String::new() },
        autoconnect: false,
    };
    let offline = matches!(scenario, "offline" | "airplane");
    Overview {
        state: if offline { "disconnected" } else { "connected" }.into(),
        connectivity: match scenario {
            "offline" | "airplane" => "none",
            "portal" => "portal",
            _ => "full",
        }
        .into(),
        wifi_hw: if scenario == "no-wifi" {
            "missing"
        } else {
            "enabled"
        }
        .into(),
        wifi_on,
        wwan_hw: if scenario == "modem" {
            "enabled"
        } else {
            "missing"
        }
        .into(),
        wwan_on: scenario == "modem",
        airplane: scenario == "airplane",
        devices,
        vpns: vec![
            vpn("Work WireGuard", VpnKind::WireGuard, !offline),
            vpn(
                "Office OpenVPN",
                VpnKind::Plugin("org.freedesktop.NetworkManager.openvpn".into()),
                false,
            ),
        ],
    }
}

/// Read the overview. A failure means NetworkManager could not be asked.
pub fn load_overview() -> Result<Overview, CommandError> {
    #[cfg(feature = "snapshot")]
    if let Some(scenario) = fake_scenario() {
        return Ok(fake_overview(&scenario));
    }
    let general = split_terse(
        nmcli(&["-t", "-f", "STATE,CONNECTIVITY", "general"], QUICK)
            .map_err(|error| CommandError(format!("NetworkManager is not available. {}", error.0)))?
            .trim(),
    );
    let radio =
        split_terse(nmcli(&["-t", "-f", "WIFI-HW,WIFI,WWAN-HW,WWAN", "radio"], QUICK)?.trim());
    let field = |list: &[String], index: usize| list.get(index).cloned().unwrap_or_default();
    let devices = parse_devices(&nmcli(
        &[
            "-t",
            "-f",
            "GENERAL.DEVICE,GENERAL.TYPE,GENERAL.STATE,GENERAL.CONNECTION,GENERAL.CON-UUID,GENERAL.HWADDR,GENERAL.MTU,CAPABILITIES.SPEED,WIRED-PROPERTIES.CARRIER,IP4.ADDRESS,IP4.GATEWAY,IP4.DNS,IP6.ADDRESS,IP6.GATEWAY",
            "device",
            "show",
        ],
        QUICK,
    )?);
    let vpns = parse_vpns(
        &nmcli(
            &[
                "-t",
                "-f",
                "NAME,UUID,TYPE,AUTOCONNECT,ACTIVE,DEVICE,STATE",
                "connection",
                "show",
            ],
            QUICK,
        )?,
        vpn_service,
    );
    Ok(Overview {
        state: field(&general, 0),
        connectivity: field(&general, 1),
        wifi_hw: field(&radio, 0),
        wifi_on: field(&radio, 1) == "enabled",
        wwan_hw: field(&radio, 2),
        wwan_on: field(&radio, 3) == "enabled",
        airplane: airplane_blocked(),
        devices,
        vpns,
    })
}

/// Milliseconds since the epoch of the last finished overview read, so
/// several open screens do not each read it on every tick.
static LAST_READ: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Read the overview again now, and store it for every screen.
pub fn reload_overview(state: State<Option<Result<Overview, CommandError>>>) {
    background_then(
        || system::load_persistent("network-overview", load_overview),
        move |loaded| {
            LAST_READ.store(now_ms(), Ordering::SeqCst);
            let mut state = state;
            state.set(Some(loaded));
        },
    );
}

/// The shared overview, as the last good value at once and refreshed every
/// few seconds while a screen that calls this is open. Call from `render`.
pub fn use_overview() -> State<Option<Result<Overview, CommandError>>> {
    use_hook(|| {
        let state = system::cached_persistent::<Overview, CommandError>("network-overview");
        reload_overview(state);
        spawn(async move {
            loop {
                timer(POLL).await;
                if now_ms().saturating_sub(LAST_READ.load(Ordering::SeqCst)) >= 4000 {
                    reload_overview(state);
                }
            }
        });
        state
    })
}

/// Start loading the overview before the page is opened.
pub fn prefetch() {
    system::prefetch_persistent("network-overview", load_overview);
}

/// What a row on the Network page needs to act: the navigator, the toast and
/// popup surfaces, and the shared overview to reload after a change. Copy, so
/// every event handler owns one.
#[derive(Clone, Copy)]
pub struct NetCtx {
    pub navigator: Navigator,
    pub overlays: Overlays,
    pub window: Window,
    pub overview: State<Option<Result<Overview, CommandError>>>,
}

impl PartialEq for NetCtx {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl NetCtx {
    /// Read the overview again now.
    pub fn reload(self) {
        reload_overview(self.overview);
    }
}

/// The context for this screen. Call from `render`.
pub fn use_net_ctx() -> NetCtx {
    NetCtx {
        navigator: use_navigator(),
        overlays: use_overlays(),
        window: use_window(),
        overview: use_overview(),
    }
}

/// Pending values for switches that are changing, by key (`wired:enp3s0`,
/// `vpn:Home`). Pages and their subpages share it, so a change shows the
/// same everywhere at once.
pub fn pending() -> State<Vec<(String, bool)>> {
    thread_local! {
        static PENDING: State<Vec<(String, bool)>> = State::create_global(Vec::new());
    }
    PENDING.with(|pending| *pending)
}

/// The value a change has set for `key`, while it is in flight.
pub fn pending_for(key: &str) -> Option<bool> {
    pending()
        .read()
        .iter()
        .find(|(other, _)| other == key)
        .map(|(_, on)| *on)
}

/// Show `on` for `key` at once and run `command` in the background. On
/// success the overview is read again, and that read drops the pending value.
/// A failure puts the old value back and shows the error.
pub fn toggle_link(
    ctx: NetCtx,
    key: String,
    on: bool,
    command: impl FnOnce() -> Result<(), CommandError> + Send + 'static,
) {
    let shown = pending();
    let previous = shown.read().clone();
    let mut value = previous.clone();
    value.retain(|(other, _)| *other != key);
    value.push((key.clone(), on));
    bingux_ui::runtime::optimistic_then(
        shown,
        Some(ctx.overlays),
        previous,
        value,
        command,
        move || {
            background_then(
                || system::load_persistent("network-overview", load_overview),
                move |loaded| {
                    LAST_READ.store(now_ms(), Ordering::SeqCst);
                    let mut overview = ctx.overview;
                    overview.set(Some(loaded));
                    let mut shown = shown;
                    let mut list = shown.read().clone();
                    list.retain(|(other, _)| *other != key);
                    shown.set(list);
                },
            );
        },
    );
}

/// The icon, title and state at the top of a screen.
pub fn hero(tokens: Tokens, icon: Icon, title: String, state: String) -> Element {
    rect()
        .tw(&tokens, &[w_full(), items(Align::Center), gap(S2), py(S2)])
        .child(icons::icon(
            icon,
            IconSize::Hero,
            Paint::Text.color(&tokens),
        ))
        .child(heading(title))
        .child(centered_caption(state))
        .into()
}
