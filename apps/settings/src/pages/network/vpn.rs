//! VPNs: the list on the Network page and one VPN's screen.
//!
//! A VPN is a NetworkManager profile of type `wireguard` (built in) or `vpn`
//! (run by a plugin such as OpenVPN). The switch in a row brings it up or
//! down with `nmcli connection up/down`; `up` can take a while, so the row
//! shows a spinner, and a failed attempt keeps its reason on the row until the
//! next attempt. The tunnel's own settings are in `vpn_tunnel`, adding a VPN in
//! `vpn_add`; the pieces they share are the child modules here.

pub(super) mod bus;
pub(super) mod data;
pub(super) mod nm;
pub(super) mod plugins;
pub(super) mod wireguard;

use std::sync::Mutex;

use freya::prelude::*;

use super::model::{self, NetCtx, Overview, Vpn};
use super::profile::ProfileSettings;
use super::{Owner, Section, usage};
use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::routes;
use crate::system::{self, CommandError};
use crate::ui::{
    CANCEL_ID, Group, ResponseKind, SpinnerSize, Status, action_strip, dialog, page_column, row, spinner,
    status, strip_action, switch, use_tokens,
};
use bingux_ui::shell::{MenuItem, menu_item, menu_separator};
use bingux_ui::style::*;

use data::PluginProfile;
use wireguard::WgProfile;

/// Why each VPN's last attempt to connect failed, by profile name. A new
/// attempt, or a connection that works, clears the entry. Kept outside the
/// UI state because the command that fails runs on a worker thread; the row
/// reads it again whenever its switch state changes, which a failure does.
static FAILURES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn set_failure(name: &str, reason: Option<String>) {
    let Ok(mut failures) = FAILURES.lock() else {
        return;
    };
    failures.retain(|(other, _)| other != name);
    if let Some(reason) = reason {
        failures.push((name.to_string(), reason));
    }
}

/// The reason the last attempt to connect `name` failed, until the next one.
pub fn failure_of(name: &str) -> Option<String> {
    #[cfg(feature = "snapshot")]
    if fake_mode().as_deref() == Some("failed") && name == "Office OpenVPN" {
        return Some(nm::activation_reason(
            "Connection activation failed: (11) Login failed",
        ));
    }
    FAILURES
        .lock()
        .ok()?
        .iter()
        .find(|(other, _)| other == name)
        .map(|(_, reason)| reason.clone())
}

/// A snapshot build can show the VPN states the real machine rarely has:
/// `BINGUX_SETTINGS_FAKE_VPN=failed|connecting|empty`.
#[cfg(feature = "snapshot")]
fn fake_mode() -> Option<String> {
    model::fake_scenario().and_then(|_| std::env::var("BINGUX_SETTINGS_FAKE_VPN").ok())
}

/// The VPN profiles to list.
fn vpns_of(ov: &Overview) -> Vec<Vpn> {
    #[cfg(feature = "snapshot")]
    match fake_mode().as_deref() {
        Some("empty") => return Vec::new(),
        Some("connecting") => {
            let mut vpns = ov.vpns.clone();
            if let Some(vpn) = vpns.get_mut(1) {
                vpn.activating = true;
            }
            return vpns;
        }
        _ => {}
    }
    ov.vpns.clone()
}

fn vpn_key(name: &str) -> String {
    format!("vpn:{name}")
}

/// Where one VPN stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    /// Up, with the tunnel's IPv4 address when it has one.
    Connected(String),
    Connecting,
    Failed(String),
    Off,
}

impl Link {
    /// The state line of a row or screen.
    pub fn line(&self) -> String {
        match self {
            Self::Connected(address) if address.is_empty() => "Connected".to_string(),
            Self::Connected(address) => format!("Connected · {address}"),
            Self::Connecting => "Connecting…".to_string(),
            Self::Failed(reason) => format!("Failed: {reason}"),
            Self::Off => "Not connected".to_string(),
        }
    }

    /// Whether a switch for this VPN shows on: connected or on its way.
    pub fn on(&self) -> bool {
        matches!(self, Self::Connected(_) | Self::Connecting)
    }
}

/// The state of `vpn`: what the person has just asked for while a change is
/// in flight, else what NetworkManager reports, else the last failure.
pub fn link_of(ov: &Overview, vpn: &Vpn) -> Link {
    link_from(
        ov,
        vpn,
        model::pending_for(&vpn_key(&vpn.name)),
        failure_of(&vpn.name),
    )
}

/// [`link_of`] with the in-flight request and the last failure given.
fn link_from(ov: &Overview, vpn: &Vpn, asked: Option<bool>, failure: Option<String>) -> Link {
    let connected = || {
        let address = ov
            .devices
            .iter()
            .find(|device| !vpn.device.is_empty() && device.name == vpn.device)
            .map(|device| device.address4())
            .unwrap_or_default();
        Link::Connected(address)
    };
    match asked {
        Some(false) => Link::Off,
        Some(true) if vpn.active => connected(),
        Some(true) => Link::Connecting,
        None if vpn.activating => Link::Connecting,
        None if vpn.active => connected(),
        None => match failure {
            Some(reason) => Link::Failed(reason),
            None => Link::Off,
        },
    }
}

/// Bring `name` up or down. A failed `up` keeps its reason for the row.
fn set_link(ctx: NetCtx, name: String, up: bool) {
    if up {
        set_failure(&name, None);
    }
    model::toggle_link(ctx, vpn_key(&name), up, move || {
        if up {
            // nmcli waits for the tunnel; its own limit is below ours so its
            // message, not a killed process, is what comes back.
            match nm::run(&["--wait", "60", "connection", "up", "id", &name]) {
                Ok(_) => Ok(()),
                Err(message) => {
                    let reason = nm::activation_reason(&message);
                    set_failure(&name, Some(reason.clone()));
                    Err(CommandError(reason))
                }
            }
        } else {
            nm::run(&["connection", "down", "id", &name])
                .map(|_| ())
                .map_err(CommandError)
        }
    });
}

/// Ask first: deleting removes the profile and the keys it holds.
fn confirm_delete(ctx: NetCtx, name: String) {
    ctx.overlays.show_dialog(
        dialog(
            format!("Delete \"{name}\"?"),
            "The VPN is deleted from this computer, with the keys and passwords saved in it.",
        )
        .cancel("Cancel")
        .default_action(CANCEL_ID)
        .response("Delete", ResponseKind::Destructive, move |_| {
            let target = name.clone();
            system::background_then(
                move || nm::run(&["connection", "delete", "id", &target]),
                move |result| {
                    if let Err(message) = result {
                        ctx.overlays.toast(message);
                    }
                    ctx.reload();
                },
            );
        }),
    );
}

/// The right-click menu for one VPN.
fn vpn_menu(ctx: NetCtx, name: &str, up: bool) -> Vec<MenuItem> {
    let toggle_name = name.to_string();
    let edit = routes::vpn(&name.to_string());
    let delete = name.to_string();
    let toggle = if up {
        menu_item("Disconnect", move |_| {
            set_link(ctx, toggle_name.clone(), false)
        })
        .icon(Icon::WindowClose)
    } else {
        menu_item("Connect", move |_| set_link(ctx, toggle_name.clone(), true))
            .icon(Icon::NetworkVpn)
    };
    vec![
        toggle,
        menu_item("Edit", move |_| ctx.router.open(edit.clone())).icon(Icon::PreferencesSystem),
        menu_separator(),
        menu_item("Delete", move |_| confirm_delete(ctx, delete.clone()))
            .icon(Icon::UserTrash)
            .destructive(),
    ]
}

fn vpn_row(ctx: NetCtx, ov: &Overview, vpn: &Vpn) -> Element {
    let link = link_of(ov, vpn);
    let name = vpn.name.clone();
    let route = routes::vpn(&vpn.name.clone());
    let toggle_name = vpn.name.clone();
    let mut trailing = rect().tw_layout(&[flex_row(), gap(S2), items(Align::Center)]);
    if link == Link::Connecting {
        trailing = trailing.child(Element::from(spinner(SpinnerSize::Small)));
    }
    trailing = trailing.child(Element::from(switch(link.on(), move |next: bool| {
        set_link(ctx, toggle_name.clone(), next)
    })));
    let mut item = row(vpn.name.clone())
        .leading(Icon::NetworkVpn)
        .subtitle(link.line())
        .subtitle_one_line()
        .trailing(trailing)
        .trailing_control()
        .navigates()
        .on_press(move |_| ctx.router.open(route.clone()))
        .context_menu(vpn_menu(ctx, &name, link.on()));
    if matches!(link, Link::Connected(_)) {
        item = item.leading_tone(Tone::Accent);
    } else if matches!(link, Link::Failed(_)) {
        item = item.leading_tone(Tone::Danger);
    }
    Element::from(item)
}

/// The VPN group of the Network page: one row per profile, then Add VPN.
pub fn vpn_group(ctx: NetCtx, ov: &Overview) -> Element {
    let mut rows: Vec<Element> = vpns_of(ov)
        .iter()
        .map(|vpn| vpn_row(ctx, ov, vpn))
        .collect();
    rows.push(Element::from(
        row("Add VPN…")
            .leading(Icon::ListAdd)
            .navigates()
            .on_press(move |_| ctx.router.open(routes::add_vpn(None))),
    ));
    Element::from(Group::titled("VPN").rows(rows))
}

/// A VPN profile as read for its screens.
#[derive(Clone, Debug, PartialEq)]
pub enum Profile {
    Wireguard(WgProfile),
    Plugin(PluginProfile),
}

/// Read the VPN profile named `name`.
pub fn load_profile(name: &str) -> Result<Profile, String> {
    #[cfg(feature = "snapshot")]
    if let Some(profile) = fake::profile(name) {
        return Ok(profile);
    }
    match nm::show(name, &["connection.type"])?[0].as_str() {
        "wireguard" => wireguard::load(name).map(Profile::Wireguard),
        "vpn" => data::load(name).map(Profile::Plugin),
        _ => Err(format!("\"{name}\" is not a VPN.")),
    }
}

/// Where the VPN connects to: a WireGuard peer's endpoint, or a plugin's
/// server address.
pub fn gateway_of(profile: &Profile) -> String {
    match profile {
        Profile::Wireguard(wg) => {
            let mut endpoints = wg
                .peers
                .iter()
                .map(|peer| peer.endpoint.as_str())
                .filter(|endpoint| !endpoint.is_empty());
            match (endpoints.next(), endpoints.count()) {
                (Some(first), 0) => first.to_string(),
                (Some(first), more) => format!("{first} (+{more} more)"),
                (None, _) => String::new(),
            }
        }
        Profile::Plugin(plugin) => {
            let short = plugins::short_of_service(&plugin.service);
            let table_key = plugins::fields(short)
                .iter()
                .find(|field| field.required)
                .and_then(|field| field.key());
            let guesses = ["remote", "gateway", "IPSec gateway", "address", "server"];
            table_key
                .into_iter()
                .chain(guesses)
                .find_map(|key| data::get(&plugin.data, key))
                .unwrap_or_default()
                .to_string()
        }
    }
}

fn dash(text: String) -> String {
    if text.is_empty() {
        "—".to_string()
    } else {
        text
    }
}

/// Read the profile `name` into `profile` in the background.
fn fetch(name: String, profile: State<Option<Result<Profile, String>>>) {
    let mut profile = profile;
    system::background_then(
        move || load_profile(&name),
        move |loaded| profile.set(Some(loaded)),
    );
}

/// One VPN: its state, what you can do with it, and its settings.
#[derive(PartialEq)]
pub struct VpnPage {
    pub name: String,
}

impl Component for VpnPage {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let tokens = use_tokens();
        let name = self.name.clone();
        let profile = use_state(|| None::<Result<Profile, String>>);
        use_hook({
            let name = name.clone();
            move || fetch(name, profile)
        });

        // Go back to the list once the profile is gone.
        let gone = name.clone();
        use_side_effect(move || {
            if let Some(Ok(ov)) = &*ctx.overview.read()
                && ov.vpn(&gone).is_none()
            {
                ctx.router.pop();
            }
        });

        let overview = ctx.overview.read().clone();
        let loaded = profile.read().clone();
        let loading = loaded.is_none();
        let ov = match overview {
            Some(Ok(ov)) => ov,
            Some(Err(failure)) => {
                return page_column().child(
                    status(Status::Error(failure.0)).action("Try again", move |_| ctx.reload()),
                );
            }
            None => {
                return veiled(
                    page_column().child(model::hero(
                        tokens,
                        Icon::NetworkVpn,
                        name.clone(),
                        "—".to_string(),
                    )),
                    true,
                );
            }
        };
        let Some(vpn) = vpns_of(&ov).into_iter().find(|vpn| vpn.name == name) else {
            return page_column();
        };
        let link = link_of(&ov, &vpn);

        let mut actions = Vec::new();
        let toggle = name.clone();
        if link.on() {
            actions.push(strip_action(Icon::WindowClose, "Disconnect", move |_| {
                set_link(ctx, toggle.clone(), false)
            }));
        } else {
            actions.push(strip_action(Icon::NetworkVpn, "Connect", move |_| {
                set_link(ctx, toggle.clone(), true)
            }));
        }
        let delete = name.clone();
        actions.push(strip_action(Icon::UserTrash, "Delete", move |_| {
            confirm_delete(ctx, delete.clone())
        }));

        // The everyday facts: where it connects to, the tunnel's address and this VPN's data
        // usage. A failed attempt says why in the line under the name.
        let mut info = Group::new(None::<String>);
        match &loaded {
            Some(Err(message)) => {
                let retry = name.clone();
                info = info.row(Element::from(
                    status(Status::Error(message.clone())).action("Try again", move |_| {
                        let mut shown = profile;
                        shown.set(None);
                        fetch(retry.clone(), profile)
                    }),
                ));
            }
            other => {
                let server = other
                    .as_ref()
                    .and_then(|loaded| loaded.as_ref().ok())
                    .map(gateway_of)
                    .unwrap_or_default();
                info = info.row(row("Server").mono_value(dash(server)));
            }
        }
        if let Link::Connected(address) = &link
            && !address.is_empty()
        {
            info = info.row(row("IP address").mono_value(address.clone()));
        }
        let owner = Owner::Vpn { name: name.clone() };
        info = info.row(usage::connection_usage_row(
            ctx,
            name.clone(),
            routes::section(&owner.clone(), &Section::Usage),
        ));

        veiled(
            page_column()
                .child(model::hero(
                    tokens,
                    Icon::NetworkVpn,
                    name.clone(),
                    link.line(),
                ))
                .child(action_strip(actions))
                .child(info)
                .child(ProfileSettings { owner }),
            loading,
        )
    }
}

/// Stand-in profiles for the fake network of snapshot builds, so the VPN
/// screens can be drawn without a real VPN.
#[cfg(feature = "snapshot")]
mod fake {
    use super::*;

    pub fn profile(name: &str) -> Option<Profile> {
        model::fake_scenario()?;
        match name {
            "Work WireGuard" => Some(Profile::Wireguard(WgProfile {
                uuid: "11111111-1111-4111-8111-000000000014".into(),
                device_name: "wg0".into(),
                interface: wireguard::Interface {
                    private_key: "YFAnE0psgIdiAF7XR4abxiwVRnlMfeltxu2s8/4Q43k=".into(),
                    listen_port: String::new(),
                    mtu: "1420".into(),
                    fwmark: String::new(),
                    addresses: vec!["10.8.0.2/32".into(), "fd08::2/128".into()],
                    dns: vec!["10.8.0.1".into()],
                },
                peers: vec![
                    wireguard::Peer {
                        public_key: "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=".into(),
                        preshared_key: String::new(),
                        endpoint: "vpn.example.com:51820".into(),
                        allowed_ips: vec!["0.0.0.0/0".into(), "::/0".into()],
                        keepalive: 25,
                    },
                    wireguard::Peer {
                        public_key: "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=".into(),
                        preshared_key: "FpCyhws9cxwWoV4xELtfJvjJN+zQVRE+GYI0f/CXe4I=".into(),
                        endpoint: "backup.example.com:51820".into(),
                        allowed_ips: vec!["10.8.0.0/24".into()],
                        keepalive: 0,
                    },
                ],
            })),
            "Office OpenVPN" => Some(Profile::Plugin(PluginProfile {
                uuid: "11111111-1111-4111-8111-000000000014".into(),
                service: "org.freedesktop.NetworkManager.openvpn".into(),
                user_name: String::new(),
                data: data::parse(
                    "ca = /home/kieran/.local/share/networkmanagement/certificates/nm-openvpn/office-ca.pem, \
                     cipher = AES-256-GCM, connection-type = password, dev = tun, \
                     password-flags = 1, remote = vpn.office.example:1194, \
                     remote-cert-tls = server, username = kieran",
                ),
                secrets: Vec::new(),
            })),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::network::model::{Device, DeviceKind, VpnKind};

    fn overview(vpns: Vec<Vpn>, devices: Vec<Device>) -> Overview {
        Overview {
            state: "connected".into(),
            connectivity: "full".into(),
            wifi_hw: "enabled".into(),
            wifi_on: true,
            wwan_hw: "missing".into(),
            wwan_on: false,
            airplane: false,
            devices,
            vpns,
        }
    }

    fn vpn(name: &str, active: bool, activating: bool) -> Vpn {
        Vpn {
            name: name.into(),
            uuid: String::new(),
            kind: VpnKind::WireGuard,
            active,
            activating,
            device: if active { "wg0".into() } else { String::new() },
            autoconnect: false,
        }
    }

    fn tunnel_device() -> Device {
        Device {
            name: "wg0".into(),
            kind: DeviceKind::WireGuard,
            state: "connected".into(),
            connection: "Work".into(),
            uuid: String::new(),
            hwaddr: String::new(),
            speed: None,
            carrier: true,
            mtu: None,
            ipv4: vec!["10.8.0.2/32".into()],
            ipv6: Vec::new(),
            gateway4: String::new(),
            gateway6: String::new(),
            dns: Vec::new(),
        }
    }

    #[test]
    fn a_connected_vpn_shows_its_tunnel_address() {
        let ov = overview(vec![vpn("Work", true, false)], vec![tunnel_device()]);
        let link = link_from(&ov, &ov.vpns[0], None, None);
        assert_eq!(link, Link::Connected("10.8.0.2".into()));
        assert_eq!(link.line(), "Connected · 10.8.0.2");
        assert!(link.on());
    }

    #[test]
    fn a_connected_vpn_without_a_device_has_no_address() {
        let ov = overview(vec![vpn("Work", true, false)], Vec::new());
        assert_eq!(link_from(&ov, &ov.vpns[0], None, None).line(), "Connected");
    }

    #[test]
    fn state_lines() {
        let ov = overview(
            vec![vpn("Idle", false, false), vpn("Busy", false, true)],
            Vec::new(),
        );
        let idle = link_from(&ov, &ov.vpns[0], None, None);
        let busy = link_from(&ov, &ov.vpns[1], None, None);
        assert_eq!(idle.line(), "Not connected");
        assert_eq!(busy.line(), "Connecting…");
        assert!(busy.on());
        assert!(!idle.on());
    }

    #[test]
    fn a_request_in_flight_wins_over_the_reported_state() {
        let ov = overview(
            vec![vpn("Up", true, false), vpn("Down", false, false)],
            Vec::new(),
        );
        assert_eq!(link_from(&ov, &ov.vpns[0], Some(false), None), Link::Off);
        assert_eq!(
            link_from(&ov, &ov.vpns[1], Some(true), None),
            Link::Connecting
        );
    }

    #[test]
    fn a_failure_shows_until_the_next_attempt() {
        let ov = overview(vec![vpn("Work", false, false)], Vec::new());
        let failed = link_from(
            &ov,
            &ov.vpns[0],
            None,
            Some("The server refused it.".into()),
        );
        assert_eq!(failed.line(), "Failed: The server refused it.");
        assert!(!failed.on());
        // A new attempt shows as connecting, not as the old failure.
        let retry = link_from(&ov, &ov.vpns[0], Some(true), Some("old".into()));
        assert_eq!(retry, Link::Connecting);
    }

    #[test]
    fn failures_are_stored_by_name_and_cleared() {
        set_failure("nm-test-failure", Some("Because.".into()));
        assert_eq!(failure_of("nm-test-failure").as_deref(), Some("Because."));
        set_failure("nm-test-failure", Some("Again.".into()));
        assert_eq!(failure_of("nm-test-failure").as_deref(), Some("Again."));
        set_failure("nm-test-failure", None);
        assert_eq!(failure_of("nm-test-failure"), None);
    }

    #[test]
    fn gateways_come_from_the_peer_or_the_plugin() {
        let peer = |endpoint: &str| wireguard::Peer {
            endpoint: endpoint.into(),
            ..wireguard::Peer::default()
        };
        let wg = |peers: Vec<wireguard::Peer>| {
            Profile::Wireguard(WgProfile {
                peers,
                ..WgProfile::default()
            })
        };
        assert_eq!(
            gateway_of(&wg(vec![peer("1.2.3.4:51820")])),
            "1.2.3.4:51820"
        );
        assert_eq!(
            gateway_of(&wg(vec![peer("1.2.3.4:51820"), peer(""), peer("b:1")])),
            "1.2.3.4:51820 (+1 more)"
        );
        assert_eq!(gateway_of(&wg(vec![peer("")])), "");
        let plugin = Profile::Plugin(PluginProfile {
            service: "org.freedesktop.NetworkManager.vpnc".into(),
            data: data::parse("IPSec gateway = 10.0.0.1, IPSec ID = grp"),
            ..PluginProfile::default()
        });
        assert_eq!(gateway_of(&plugin), "10.0.0.1");
    }
}
