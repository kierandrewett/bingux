//! A VPN's tunnel settings: WireGuard interface and peers, or a plugin's
//! gateway, sign-in and the rest of its `vpn.data`.
//!
//! Each screen keeps a draft of what is shown and a bar that applies it
//! together (`pending_bar`); nothing is written while a field is invalid.
//! Changes to a running tunnel are applied to it with `nmcli device reapply`
//! when NetworkManager takes that, and the screen says so when it does not.

use freya::prelude::*;

use super::model::{self, NetCtx};
use super::vpn::data::{self, PluginProfile};
use super::vpn::plugins::{self, Field, Kind, SECRET_FLAGS, Store};
use super::vpn::wireguard::{self, Interface, Peer, check};
use super::vpn::{self, Profile, nm};
use super::{Owner, Section};
use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::routes::Route;
use crate::system;
use crate::ui::{
    Group, ResponseKind, Status, action_button, action_row, choice_row, dialog, form_field,
    icon_button, page_column, pending_bar, primary_button, row, status, switch_row, text_field,
};
use bingux_ui::style::*;

/// The note shown when saved changes could not be given to a running tunnel.
const RECONNECT_NOTE: &str = "Saved. Turn the VPN off and on again to use the new settings.";

/// After saving: give the changes to the running tunnel `device`, if any.
/// Returns whether the person has to reconnect for them to count.
fn needs_reconnect(device: &Option<String>) -> bool {
    match device {
        Some(device) => !nm::reapply(device),
        None => false,
    }
}

/// The device of the running tunnel for `name`, from the shared overview.
fn running_device(ctx: NetCtx, name: &str) -> Option<String> {
    let loaded = ctx.overview.read();
    let ov = loaded.as_ref()?.as_ref().ok()?;
    let vpn = ov.vpn(name)?;
    (vpn.active && !vpn.device.is_empty()).then(|| vpn.device.clone())
}

/// A VPN's tunnel: loads the profile and shows the screen for its kind.
#[derive(PartialEq)]
pub struct TunnelPage {
    pub owner: Owner,
}

/// Read `name` into `profile`, then restart the forms with what was read.
fn reload(name: String, profile: State<Option<Result<Profile, String>>>, generation: State<u32>) {
    let (mut profile, mut generation) = (profile, generation);
    system::background_then(
        move || vpn::load_profile(&name),
        move |loaded| {
            profile.set(Some(loaded));
            generation.set(generation() + 1);
        },
    );
}

impl Component for TunnelPage {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let name = self.owner.connection_name().to_string();
        let profile = use_state(|| None::<Result<Profile, String>>);
        let generation = use_state(|| 0u32);
        use_hook({
            let name = name.clone();
            move || reload(name, profile, generation)
        });

        let loaded = profile.read().clone();
        let device = running_device(ctx, &name);
        let restart = EventHandler::new(move |_: ()| {
            let mut generation = generation;
            generation.set(generation() + 1);
        });
        let saved = {
            let name = name.clone();
            EventHandler::new(move |_: ()| reload(name.clone(), profile, generation))
        };
        let body: Element = match loaded {
            None => veiled(page_column().child(placeholder_group()), true).into(),
            Some(Err(message)) => {
                let retry = name.clone();
                page_column()
                    .child(
                        status(Status::Error(message)).action("Try again", move |_| {
                            reload(retry.clone(), profile, generation)
                        }),
                    )
                    .into()
            }
            Some(Ok(Profile::Wireguard(wg))) => WgTunnel {
                owner: self.owner.clone(),
                profile: wg,
                device,
                restart,
                saved,
            }
            .into(),
            Some(Ok(Profile::Plugin(plugin))) => PluginForm {
                name: name.clone(),
                profile: plugin,
                device,
                restart,
                saved,
            }
            .into(),
        };
        // A new key remounts the form, so its fields start from what was read.
        rect().key(generation()).child(body)
    }
}

/// The shape of the screen while the profile loads.
fn placeholder_group() -> Element {
    Group::titled("Tunnel")
        .row(row("Server").value("—"))
        .row(row("Sign in with").value("—"))
        .into()
}

/// The draft of a WireGuard interface: one text state per field.
#[derive(Clone, Copy)]
struct InterfaceDraft {
    private_key: State<String>,
    listen_port: State<String>,
    mtu: State<String>,
    fwmark: State<String>,
    addresses: State<String>,
    dns: State<String>,
}

impl InterfaceDraft {
    fn new(saved: &Interface) -> Self {
        Self {
            private_key: use_state(|| saved.private_key.clone()),
            listen_port: use_state(|| saved.listen_port.clone()),
            mtu: use_state(|| saved.mtu.clone()),
            fwmark: use_state(|| saved.fwmark.clone()),
            addresses: use_state(|| wireguard::list_text(&saved.addresses)),
            dns: use_state(|| wireguard::list_text(&saved.dns)),
        }
    }

    fn read(&self) -> Interface {
        Interface {
            private_key: self.private_key.read().trim().to_string(),
            listen_port: self.listen_port.read().trim().to_string(),
            mtu: self.mtu.read().trim().to_string(),
            fwmark: self.fwmark.read().trim().to_string(),
            addresses: wireguard::parse_list(&self.addresses.read()),
            dns: wireguard::parse_list(&self.dns.read()),
        }
    }
}

/// The first thing wrong with `interface`, in words, or `None`.
pub(super) fn interface_error(interface: &Interface) -> Option<String> {
    let optional = |text: &str, check: fn(&str) -> Result<(), String>| {
        if text.is_empty() { Ok(()) } else { check(text) }
    };
    check::key(&interface.private_key)
        .and_then(|_| optional(&interface.listen_port, check::port))
        .and_then(|_| optional(&interface.mtu, check::mtu))
        .and_then(|_| optional(&interface.fwmark, check::fwmark))
        .and_then(|_| check::cidr_list(&wireguard::list_text(&interface.addresses)))
        .and_then(|_| optional(&wireguard::list_text(&interface.dns), check::dns_list))
        .err()
        .or_else(|| {
            // NetworkManager refuses DNS servers of a family with no address.
            let has = |v6: bool, items: &[String]| items.iter().any(|item| item.contains(':') == v6);
            for v6 in [false, true] {
                if has(v6, &interface.dns) && !has(v6, &interface.addresses) {
                    return Some(format!(
                        "A DNS server needs an {} address for the tunnel. Add one above or remove the server.",
                        if v6 { "IPv6" } else { "IPv4" }
                    ));
                }
            }
            None
        })
}

/// A WireGuard tunnel: the interface and the list of peers.
#[derive(PartialEq)]
struct WgTunnel {
    owner: Owner,
    profile: wireguard::WgProfile,
    /// The tunnel's device while it is up.
    device: Option<String>,
    restart: EventHandler<()>,
    saved: EventHandler<()>,
}

impl Component for WgTunnel {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let name = self.owner.connection_name().to_string();
        let draft = InterfaceDraft::new(&self.profile.interface);
        let public = use_state(|| None::<String>);
        let error = use_state(|| None::<String>);
        let busy = use_state(|| false);
        let has_wg = wireguard::has_wg();
        use_hook({
            let key = self.profile.interface.private_key.clone();
            move || {
                if wireguard::has_wg() {
                    let mut public = public;
                    system::background_then(
                        move || wireguard::public_key(&key),
                        move |derived| public.set(derived),
                    );
                }
            }
        });

        let current = draft.read();
        let problem = interface_error(&current);
        let dirty = current != self.profile.interface;
        let apply = {
            let name = name.clone();
            let device = self.device.clone();
            let saved = self.saved.clone();
            move |_: ()| {
                let interface = draft.read();
                if *busy.peek() || interface_error(&interface).is_some() {
                    return;
                }
                let (mut busy, mut error) = (busy, error);
                busy.set(true);
                error.set(None);
                let (name, device, saved) = (name.clone(), device.clone(), saved.clone());
                system::background_then(
                    move || {
                        wireguard::save_interface(&name, &interface)
                            .map(|_| needs_reconnect(&device))
                    },
                    move |result| {
                        busy.set(false);
                        match result {
                            Ok(reconnect) => {
                                if reconnect {
                                    ctx.overlays.toast(RECONNECT_NOTE);
                                }
                                saved.call(());
                            }
                            Err(message) => error.set(Some(message)),
                        }
                    },
                );
            }
        };
        let revert = {
            let restart = self.restart.clone();
            move |_: ()| restart.call(())
        };

        let generate = move |_: ()| {
            system::background_then(
                wireguard::generate_private_key,
                move |result| match result {
                    Ok(key) => {
                        let mut private_key = draft.private_key;
                        private_key.set(key);
                    }
                    Err(message) => ctx.overlays.toast(message),
                },
            );
        };

        let public_row = if has_wg {
            row("Public key")
                .subtitle("Give this to the other side of the tunnel.")
                .mono_value(public.read().clone().unwrap_or_default())
        } else {
            row("Public key").subtitle(
                "Not shown here: the wg tool is not installed. The other side needs it; wg pubkey makes it from the private key.",
            )
        };
        let mut interface = Group::titled("Interface")
            .row(
                form_field("Private key", draft.private_key)
                    .password()
                    .monospace()
                    .hint("Stays on this computer. Never share it.")
                    .validate(check::key),
            )
            .row(public_row);
        if has_wg {
            interface = interface.row(action_row(
                Some("A new key needs the other side to be given the new public key."),
                action_button("Generate new key", generate).compact(),
            ));
        }
        interface = interface
            .row(
                form_field("Addresses", draft.addresses)
                    .multiline(3)
                    .monospace()
                    .hint("This computer's addresses inside the tunnel, one per line, with the prefix length.")
                    .placeholder("10.8.0.2/32")
                    .validate(check::cidr_list),
            )
            .row(
                form_field("DNS servers", draft.dns)
                    .multiline(2)
                    .monospace()
                    .optional()
                    .hint("Used while the tunnel is up, one per line.")
                    .validate(check::dns_list),
            )
            .row(
                form_field("Listen port", draft.listen_port)
                    .optional()
                    .placeholder("Automatic")
                    .validate(check::port),
            )
            .row(
                form_field("MTU", draft.mtu)
                    .optional()
                    .placeholder("Automatic")
                    .suffix("bytes")
                    .validate(check::mtu),
            )
            .row(
                form_field("Firewall mark", draft.fwmark)
                    .optional()
                    .placeholder("None")
                    .validate(check::fwmark),
            );

        let mut peers = Group::titled("Peers")
            .description("A peer is a server or computer this tunnel talks to.");
        for (index, peer) in self.profile.peers.iter().enumerate() {
            let owner = self.owner.clone();
            peers = peers.row(
                row(format!("Peer {}", index + 1))
                    .leading(Icon::NetworkServer)
                    .subtitle(peer_summary(peer))
                    .subtitle_one_line()
                    .navigates()
                    .on_press(move |_| {
                        ctx.navigator.push(Route::Section {
                            owner: owner.clone(),
                            section: Section::Peer(index),
                        })
                    }),
            );
        }
        let owner = self.owner.clone();
        let next = self.profile.peers.len();
        peers = peers.row(
            row("Add peer")
                .leading(Icon::ListAdd)
                .navigates()
                .on_press(move |_| {
                    ctx.navigator.push(Route::Section {
                        owner: owner.clone(),
                        section: Section::Peer(next),
                    })
                }),
        );

        page_column()
            .maybe_child(pending_bar(dirty, problem.is_none(), apply, revert))
            .maybe_child(
                error
                    .read()
                    .clone()
                    .map(|message| status(Status::Error(message))),
            )
            .child(interface)
            .child(peers)
    }
}

/// One line about a peer for the list: where it is and what it carries.
fn peer_summary(peer: &Peer) -> String {
    let endpoint = if peer.endpoint.is_empty() {
        "No endpoint".to_string()
    } else {
        peer.endpoint.clone()
    };
    let carries = if wireguard::is_all_traffic(&peer.allowed_ips) {
        "all traffic".to_string()
    } else if peer.allowed_ips.len() == 1 {
        peer.allowed_ips[0].clone()
    } else {
        format!("{} address ranges", peer.allowed_ips.len())
    };
    format!("{endpoint} · {carries}")
}

/// One WireGuard peer, or a new one when `index` is past the last.
#[derive(PartialEq)]
pub struct PeerPage {
    pub owner: Owner,
    pub index: usize,
}

impl Component for PeerPage {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let name = self.owner.connection_name().to_string();
        let profile = use_state(|| None::<Result<Profile, String>>);
        let generation = use_state(|| 0u32);
        use_hook({
            let name = name.clone();
            move || reload(name, profile, generation)
        });
        let loaded = profile.read().clone();
        let restart = EventHandler::new(move |_: ()| {
            let mut generation = generation;
            generation.set(generation() + 1);
        });
        let saved = {
            let name = name.clone();
            EventHandler::new(move |_: ()| reload(name.clone(), profile, generation))
        };
        let body: Element = match loaded {
            None => veiled(page_column().child(placeholder_peer()), true).into(),
            Some(Err(message)) => {
                let retry = name.clone();
                page_column()
                    .child(
                        status(Status::Error(message)).action("Try again", move |_| {
                            reload(retry.clone(), profile, generation)
                        }),
                    )
                    .into()
            }
            Some(Ok(Profile::Wireguard(wg))) => PeerForm {
                owner: self.owner.clone(),
                profile: wg,
                index: self.index,
                device: running_device(ctx, &name),
                restart,
                saved,
            }
            .into(),
            Some(Ok(Profile::Plugin(_))) => page_column()
                .child(status(Status::Empty(
                    "Only a WireGuard VPN has peers.".into(),
                )))
                .into(),
        };
        rect().key(generation()).child(body)
    }
}

fn placeholder_peer() -> Element {
    Group::titled("Peer")
        .row(row("Public key").value("—"))
        .row(row("Endpoint").value("—"))
        .into()
}

/// The draft of one peer.
#[derive(Clone, Copy)]
struct PeerDraft {
    public_key: State<String>,
    preshared_key: State<String>,
    endpoint: State<String>,
    allowed: State<String>,
    keepalive: State<String>,
}

impl PeerDraft {
    fn new(saved: &Peer) -> Self {
        Self {
            public_key: use_state(|| saved.public_key.clone()),
            preshared_key: use_state(|| saved.preshared_key.clone()),
            endpoint: use_state(|| saved.endpoint.clone()),
            allowed: use_state(|| wireguard::list_text(&saved.allowed_ips)),
            keepalive: use_state(|| {
                if saved.keepalive == 0 {
                    String::new()
                } else {
                    saved.keepalive.to_string()
                }
            }),
        }
    }

    fn read(&self) -> Peer {
        Peer {
            public_key: self.public_key.read().trim().to_string(),
            preshared_key: self.preshared_key.read().trim().to_string(),
            endpoint: self.endpoint.read().trim().to_string(),
            allowed_ips: wireguard::parse_list(&self.allowed.read()),
            keepalive: self.keepalive.read().trim().parse().unwrap_or(0),
        }
    }
}

/// The first thing wrong with `peer`, in words, or `None`. `others` are the
/// public keys of the interface's other peers.
pub(super) fn peer_error(peer: &Peer, keepalive_text: &str, others: &[&str]) -> Option<String> {
    let optional = |text: &str, check: fn(&str) -> Result<(), String>| {
        if text.is_empty() { Ok(()) } else { check(text) }
    };
    check::key(&peer.public_key)
        .and_then(|_| optional(&peer.preshared_key, check::key))
        .and_then(|_| optional(&peer.endpoint, check::endpoint))
        .and_then(|_| check::cidr_list(&wireguard::list_text(&peer.allowed_ips)))
        .and_then(|_| optional(keepalive_text, check::keepalive))
        .err()
        .or_else(|| {
            others
                .contains(&peer.public_key.as_str())
                .then(|| "Another peer of this tunnel already has this public key.".to_string())
        })
}

#[derive(PartialEq)]
struct PeerForm {
    owner: Owner,
    profile: wireguard::WgProfile,
    index: usize,
    device: Option<String>,
    restart: EventHandler<()>,
    saved: EventHandler<()>,
}

impl PeerForm {
    /// The peers after replacing or adding this one with `peer`, or removing
    /// it when `peer` is `None`.
    fn peers_with(&self, peer: Option<Peer>) -> Vec<Peer> {
        let mut peers = self.profile.peers.clone();
        match (peer, self.index < peers.len()) {
            (Some(peer), true) => peers[self.index] = peer,
            (Some(peer), false) => peers.push(peer),
            (None, true) => {
                peers.remove(self.index);
            }
            (None, false) => {}
        }
        peers
    }
}

impl Component for PeerForm {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let is_new = self.index >= self.profile.peers.len();
        let saved_peer = self
            .profile
            .peers
            .get(self.index)
            .cloned()
            .unwrap_or_default();
        let draft = PeerDraft::new(&saved_peer);
        let error = use_state(|| None::<String>);
        let busy = use_state(|| false);

        let peer = draft.read();
        let keepalive_text = draft.keepalive.read().trim().to_string();
        let others: Vec<&str> = self
            .profile
            .peers
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.index)
            .map(|(_, other)| other.public_key.as_str())
            .collect();
        let problem = peer_error(&peer, &keepalive_text, &others);
        let dirty = peer != saved_peer;
        let duplicate = (!peer.public_key.is_empty() && others.contains(&peer.public_key.as_str()))
            .then(|| "Another peer of this tunnel already has this public key.".to_string());

        let apply = EventHandler::new({
            let form = PeerForm {
                owner: self.owner.clone(),
                profile: self.profile.clone(),
                index: self.index,
                device: self.device.clone(),
                restart: self.restart.clone(),
                saved: self.saved.clone(),
            };
            let others: Vec<String> = others.iter().map(|key| key.to_string()).collect();
            move |_: ()| {
                let peer = draft.read();
                let keepalive_text = draft.keepalive.read().trim().to_string();
                let others: Vec<&str> = others.iter().map(String::as_str).collect();
                if *busy.peek() || peer_error(&peer, &keepalive_text, &others).is_some() {
                    return;
                }
                let (mut busy, mut error) = (busy, error);
                busy.set(true);
                error.set(None);
                let peers = form.peers_with(Some(peer));
                let uuid = form.profile.uuid.clone();
                let device = form.device.clone();
                let saved = form.saved.clone();
                let was_new = form.index >= form.profile.peers.len();
                system::background_then(
                    move || wireguard::save_peers(&uuid, &peers).map(|_| needs_reconnect(&device)),
                    move |result| {
                        busy.set(false);
                        match result {
                            Ok(reconnect) => {
                                if reconnect {
                                    ctx.overlays.toast(RECONNECT_NOTE);
                                }
                                if was_new {
                                    ctx.navigator.pop();
                                } else {
                                    saved.call(());
                                }
                            }
                            Err(message) => error.set(Some(message)),
                        }
                    },
                );
            }
        });
        let revert = {
            let restart = self.restart.clone();
            move |_: ()| restart.call(())
        };

        let all = wireguard::is_all_traffic(&peer.allowed_ips);
        let mut traffic = Group::titled("Traffic")
            .description(
                "Route all traffic sends everything you do online through this peer. \
                 Otherwise only the addresses you list go through it.",
            )
            .row(switch_row("Route all traffic", all, move |on: bool| {
                let mut allowed = draft.allowed;
                allowed.set(if on {
                    wireguard::list_text(&wireguard::ALL_TRAFFIC.map(String::from).to_vec())
                } else {
                    String::new()
                });
            }));
        if !all {
            traffic = traffic.row(
                form_field("Allowed addresses", draft.allowed)
                    .multiline(3)
                    .monospace()
                    .hint("Networks that go through this peer, one per line, such as 10.8.0.0/24.")
                    .placeholder("10.8.0.0/24")
                    .validate(check::cidr_list),
            );
        }

        let peer_group = Group::titled("Peer")
            .row(
                form_field("Public key", draft.public_key)
                    .monospace()
                    .hint("The other side's public key.")
                    .validate(check::key)
                    .error(duplicate),
            )
            .row(
                form_field("Preshared key", draft.preshared_key)
                    .password()
                    .monospace()
                    .optional()
                    .hint("An extra secret both sides share, if they use one.")
                    .validate(check::key),
            )
            .row(
                form_field("Endpoint", draft.endpoint)
                    .optional()
                    .hint("Where to reach it, as host:port. Blank if it connects to you.")
                    .placeholder("vpn.example.com:51820")
                    .validate(check::endpoint),
            )
            .row(
                form_field("Keepalive", draft.keepalive)
                    .optional()
                    .placeholder("Off")
                    .suffix("seconds")
                    .hint("Keeps the connection open through a router. 25 is usual.")
                    .validate(check::keepalive),
            );

        // A saved peer applies its changes together, like the interface. A new
        // peer has nothing to revert to, so it has a button to add it.
        let bar = if is_new {
            None
        } else {
            pending_bar(dirty, problem.is_none(), apply.clone(), revert)
        };
        let mut page = page_column()
            .maybe_child(bar)
            .maybe_child(
                error
                    .read()
                    .clone()
                    .map(|message| status(Status::Error(message))),
            )
            .child(peer_group)
            .child(traffic);
        if is_new {
            let add = apply.clone();
            page = page.child(
                Group::new(None::<String>).row(action_row(
                    None,
                    primary_button("Add peer", move |_| add.call(()))
                        .enabled(problem.is_none() && !*busy.read())
                        .loading(*busy.read()),
                )),
            );
        } else {
            let form = PeerForm {
                owner: self.owner.clone(),
                profile: self.profile.clone(),
                index: self.index,
                device: self.device.clone(),
                restart: self.restart.clone(),
                saved: self.saved.clone(),
            };
            let label = format!("Peer {}", self.index + 1);
            page = page.child(
                Group::new(None::<String>).row(
                    row("Delete peer")
                        .leading(Icon::UserTrash)
                        .destructive()
                        .on_press(move |_| confirm_delete_peer(ctx, &form, &label)),
                ),
            );
        }
        page
    }
}

/// Ask first: deleting a peer ends the tunnel to it.
fn confirm_delete_peer(ctx: NetCtx, form: &PeerForm, label: &str) {
    let peers = form.peers_with(None);
    let uuid = form.profile.uuid.clone();
    let device = form.device.clone();
    ctx.overlays.show_dialog(
        dialog(
            format!("Delete {label}?"),
            "This tunnel stops talking to that peer.",
        )
        .cancel("Cancel")
        .response("Delete", ResponseKind::Destructive, move |_| {
            let (peers, uuid, device) = (peers.clone(), uuid.clone(), device.clone());
            system::background_then(
                move || wireguard::save_peers(&uuid, &peers).map(|_| needs_reconnect(&device)),
                move |result| match result {
                    Ok(reconnect) => {
                        if reconnect {
                            ctx.overlays.toast(RECONNECT_NOTE);
                        }
                        ctx.navigator.pop();
                    }
                    Err(message) => ctx.overlays.toast(message),
                },
            );
        }),
    );
}

/// How many friendly fields a plugin form has room for.
pub(super) const MAX_FIELDS: usize = 10;

/// A setting outside a plugin's friendly fields: shown in the Advanced list.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Extra {
    pub key: String,
    pub value: String,
    pub secret: bool,
}

/// The settings of `profile` that no friendly field of `short` shows.
fn extras_of(short: &str, profile: &PluginProfile) -> Vec<Extra> {
    let data = profile
        .data
        .iter()
        .filter(|(key, _)| !plugins::is_covered(short, key))
        .map(|(key, value)| Extra {
            key: key.clone(),
            value: value.clone(),
            secret: false,
        });
    let secrets = profile
        .secrets
        .iter()
        .filter(|(key, _)| {
            !plugins::fields(short)
                .iter()
                .any(|field| field.key() == Some(key.as_str()))
        })
        .map(|(key, value)| Extra {
            key: key.clone(),
            value: value.clone(),
            secret: true,
        });
    data.chain(secrets).collect()
}

/// The text a field shows for `profile`.
fn initial_value(field: &Field, profile: &PluginProfile) -> String {
    match (field.store, field.kind) {
        (Store::UserName, _) => profile.user_name.clone(),
        (Store::Data(key), Kind::Secret { .. }) => data::get(&profile.secrets, key)
            .unwrap_or_default()
            .to_string(),
        (Store::Data(key), Kind::Choice(_)) => data::get(&profile.data, key)
            .map(str::to_string)
            .unwrap_or_else(|| plugins::choice_default(field).to_string()),
        (Store::Data(key), _) => data::get(&profile.data, key)
            .unwrap_or_default()
            .to_string(),
    }
}

/// How a secret field's stored value is treated, as a flags number.
fn initial_flags(field: &Field, profile: &PluginProfile) -> String {
    match field.kind {
        Kind::Secret { flags } => data::get(&profile.data, flags).unwrap_or("0").to_string(),
        _ => "0".to_string(),
    }
}

/// `profile` with the friendly `fields` set to `values` (and `flags` for
/// secrets) and the Advanced `extras` as its other settings.
pub(super) fn compose(
    short: &str,
    profile: &PluginProfile,
    fields: &[Field],
    values: &[String],
    flags: &[String],
    extras: &[Extra],
) -> PluginProfile {
    let mut composed = profile.clone();
    // Start from the settings the friendly fields do not own.
    composed
        .data
        .retain(|(key, _)| !plugins::is_covered(short, key));
    composed
        .secrets
        .retain(|(key, _)| !fields.iter().any(|field| field.key() == Some(key.as_str())));
    for (index, field) in fields.iter().enumerate() {
        let value = values[index].trim();
        match (field.store, field.kind) {
            (Store::UserName, _) => composed.user_name = value.to_string(),
            (Store::Data(key), Kind::Secret { flags: flags_key }) => {
                let asked = flags[index] == "2";
                data::set(
                    &mut composed.secrets,
                    key,
                    if asked { "" } else { values[index].as_str() },
                );
                // Flags 0 is NetworkManager's default and stays unwritten.
                data::set(
                    &mut composed.data,
                    flags_key,
                    if flags[index] == "0" {
                        ""
                    } else {
                        &flags[index]
                    },
                );
            }
            (Store::Data(key), Kind::Choice(_)) => data::set(&mut composed.data, key, value),
            (Store::Data(key), _) => data::set(&mut composed.data, key, value),
        }
    }
    for extra in extras {
        let list = if extra.secret {
            &mut composed.secrets
        } else {
            &mut composed.data
        };
        data::set(list, &extra.key, &extra.value);
    }
    composed
}

/// Sorted by key, so two profiles with the same settings compare equal
/// whatever order the settings were written in.
fn normalised(mut profile: PluginProfile) -> PluginProfile {
    profile.data.sort();
    profile.secrets.sort();
    profile
}

/// The first thing wrong with the visible fields, in words, or `None`.
pub(super) fn field_error(
    fields: &[Field],
    values: &[String],
    visible: &dyn Fn(&Field) -> bool,
) -> Option<String> {
    for (index, field) in fields.iter().enumerate() {
        if !visible(field) {
            continue;
        }
        let value = values[index].trim();
        if value.is_empty() {
            if field.required {
                return Some(format!("{} is needed.", field.label));
            }
            continue;
        }
        if let Some(check) = field.check
            && let Err(message) = check(value)
        {
            return Some(message);
        }
    }
    None
}

/// One text state per possible friendly field of a plugin, and one for how a
/// secret is kept, so the number of hooks never changes with the plugin.
#[derive(Clone)]
pub(super) struct FieldStates {
    values: Vec<State<String>>,
    flags: Vec<State<String>>,
}

/// What the fields of [`FieldStates`] hold now.
pub(super) struct FieldTexts {
    pub values: Vec<String>,
    pub flags: Vec<String>,
}

impl FieldStates {
    /// The states, seeded from `profile`. Call from `render`.
    pub(super) fn new(fields: &[Field], profile: &PluginProfile) -> Self {
        let seed = |value: fn(&Field, &PluginProfile) -> String| -> Vec<State<String>> {
            (0..MAX_FIELDS)
                .map(|index| {
                    use_state(|| {
                        fields
                            .get(index)
                            .map(|field| value(field, profile))
                            .unwrap_or_default()
                    })
                })
                .collect()
        };
        Self {
            values: seed(initial_value),
            flags: seed(initial_flags),
        }
    }

    pub(super) fn read(&self) -> FieldTexts {
        FieldTexts {
            values: self
                .values
                .iter()
                .map(|state| state.read().clone())
                .collect(),
            flags: self
                .flags
                .iter()
                .map(|state| state.read().clone())
                .collect(),
        }
    }
}

impl FieldTexts {
    /// The value of the field stored under `key`, or its default for a choice.
    fn value_of(&self, fields: &[Field], key: &str) -> String {
        fields
            .iter()
            .position(|field| field.key() == Some(key))
            .map(|index| {
                let text = self.values[index].trim().to_string();
                if text.is_empty() {
                    plugins::choice_default(&fields[index]).to_string()
                } else {
                    text
                }
            })
            .unwrap_or_default()
    }

    pub(super) fn visible(&self, fields: &[Field], field: &Field) -> bool {
        plugins::visible(field, |key| self.value_of(fields, key))
    }
}

/// The rows of `fields` that show now, added to `group`.
pub(super) fn field_rows(
    mut group: Group,
    fields: &[Field],
    states: &FieldStates,
    texts: &FieldTexts,
) -> Group {
    for (index, field) in fields.iter().enumerate() {
        if !texts.visible(fields, field) {
            continue;
        }
        let state = states.values[index];
        match field.kind {
            Kind::Choice(options) => {
                let options: Vec<(String, String)> = options
                    .iter()
                    .map(|(value, label)| (value.to_string(), label.to_string()))
                    .collect();
                let selected = texts.value_of(fields, field.key().unwrap_or_default());
                group = group.row(choice_row(
                    field.label,
                    options,
                    &selected,
                    move |key: String| {
                        let mut state = state;
                        state.set(key);
                    },
                ));
            }
            Kind::Secret { .. } => {
                let flag_state = states.flags[index];
                let mut options: Vec<(String, String)> = SECRET_FLAGS
                    .iter()
                    .map(|(value, label)| (value.to_string(), label.to_string()))
                    .collect();
                let current = texts.flags[index].clone();
                if !options.iter().any(|(value, _)| *value == current) {
                    options.push((current.clone(), "Not needed".to_string()));
                }
                group = group.row(choice_row(
                    format!("Remember {}", field.label.to_lowercase()),
                    options,
                    &current,
                    move |key: String| {
                        let mut flag_state = flag_state;
                        flag_state.set(key);
                    },
                ));
                if current != "2" {
                    group = group.row(secret_field(field, state));
                }
            }
            Kind::Text | Kind::Path => group = group.row(text_field_row(field, state)),
        }
    }
    group
}

/// A plugin VPN's settings: the friendly fields of the plugin, then every
/// other `vpn.data` key as an editable list.
#[derive(PartialEq)]
struct PluginForm {
    name: String,
    profile: PluginProfile,
    device: Option<String>,
    restart: EventHandler<()>,
    saved: EventHandler<()>,
}

impl Component for PluginForm {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let short = plugins::short_of_service(&self.profile.service).to_string();
        let fields = plugins::fields(&short);
        let profile = &self.profile;
        let states = FieldStates::new(fields, profile);
        let extras = use_state(|| extras_of(&short, profile));
        let new_key = use_state(String::new);
        let new_value = use_state(String::new);
        let error = use_state(|| None::<String>);
        let busy = use_state(|| false);

        let texts = states.read();
        let current_extras = extras.read().clone();
        let composed = compose(
            &short,
            profile,
            fields,
            &texts.values,
            &texts.flags,
            &current_extras,
        );
        let dirty = normalised(composed.clone()) != normalised(profile.clone());
        let problem = field_error(fields, &texts.values, &|field| texts.visible(fields, field));

        let apply = {
            let name = self.name.clone();
            let device = self.device.clone();
            let saved = self.saved.clone();
            let composed = composed.clone();
            move |_: ()| {
                if *busy.peek() {
                    return;
                }
                let (mut busy, mut error) = (busy, error);
                busy.set(true);
                error.set(None);
                let (name, device, saved, composed) = (
                    name.clone(),
                    device.clone(),
                    saved.clone(),
                    composed.clone(),
                );
                system::background_then(
                    move || data::save(&name, &composed).map(|_| device.is_some()),
                    move |result| {
                        busy.set(false);
                        match result {
                            Ok(running) => {
                                // A plugin reads its settings when it connects;
                                // a running tunnel cannot be given new ones.
                                if running {
                                    ctx.overlays.toast(RECONNECT_NOTE);
                                }
                                saved.call(());
                            }
                            Err(message) => error.set(Some(message)),
                        }
                    },
                );
            }
        };
        let revert = {
            let restart = self.restart.clone();
            move |_: ()| restart.call(())
        };

        let mut sign_in = Group::titled(&plugins::display_name(&short));
        if !plugins::is_known(&short) {
            sign_in = sign_in.description(
                "This app has no screen for this kind of VPN yet, so its settings are listed below as the plugin keeps them.",
            );
        }
        sign_in = field_rows(sign_in, fields, &states, &texts);

        let mut advanced = Group::titled("Advanced").description(
            "Every other setting of this VPN, as its plugin keeps it. Change them only if you know what they do.",
        );
        for extra in &current_extras {
            advanced = advanced.row(Element::from(rect().key(&extra.key).child(ExtraRow {
                extra: extra.clone(),
                extras,
            })));
        }
        let taken: Vec<String> = current_extras
            .iter()
            .map(|extra| extra.key.clone())
            .collect();
        let new_name = new_key.read().trim().to_string();
        let name_problem = new_key_error(&new_name, &taken);
        let can_add = !new_name.is_empty() && name_problem.is_none();
        let add = move |_: ()| {
            let name = new_key.read().trim().to_string();
            if name.is_empty() {
                return;
            }
            let mut list = extras.read().clone();
            list.push(Extra {
                key: name,
                value: new_value.read().trim().to_string(),
                secret: false,
            });
            let (mut extras, mut key, mut value) = (extras, new_key, new_value);
            extras.set(list);
            key.set(String::new());
            value.set(String::new());
        };
        advanced = advanced
            .row(
                form_field("New setting", new_key)
                    .optional()
                    .placeholder("name")
                    .monospace()
                    .error(name_problem),
            )
            .row(
                form_field("Value", new_value)
                    .optional()
                    .monospace()
                    .on_submit(add.clone()),
            )
            .row(action_row(
                None,
                action_button("Add setting", add).compact().enabled(can_add),
            ));

        page_column()
            .maybe_child(pending_bar(dirty, problem.is_none(), apply, revert))
            .maybe_child(
                error
                    .read()
                    .clone()
                    .map(|message| status(Status::Error(message))),
            )
            .child(sign_in)
            .child(advanced)
    }
}

/// Why `key` cannot be a new setting's name, or `None`.
fn new_key_error(key: &str, taken: &[String]) -> Option<String> {
    if key.is_empty() {
        None
    } else if key.contains(['=', ',']) {
        Some("A setting's name cannot contain = or a comma.".to_string())
    } else if taken.iter().any(|other| other == key) {
        Some("This setting is already in the list.".to_string())
    } else {
        None
    }
}

/// A text or path field of a plugin.
fn text_field_row(field: &Field, state: State<String>) -> Element {
    let mut input = form_field(field.label, state);
    if !field.hint.is_empty() {
        input = input.hint(field.hint);
    }
    if !field.placeholder.is_empty() {
        input = input.placeholder(field.placeholder);
    }
    if !field.required {
        input = input.optional();
    }
    if field.kind == Kind::Path {
        input = input.monospace();
    }
    if let Some(check) = field.check {
        input = input.validate(check);
    }
    input.into()
}

/// A secret field of a plugin.
fn secret_field(field: &Field, state: State<String>) -> Element {
    form_field(field.label, state).password().optional().into()
}

/// One setting of the Advanced list: its name, an editable value and a
/// button that removes it. Edits go straight into the draft list.
#[derive(PartialEq)]
struct ExtraRow {
    extra: Extra,
    extras: State<Vec<Extra>>,
}

impl Component for ExtraRow {
    fn render(&self) -> impl IntoElement {
        let key = self.extra.key.clone();
        let value = use_state(|| self.extra.value.clone());
        let extras = self.extras;
        let secret = self.extra.secret;
        {
            let key = key.clone();
            use_side_effect_with_deps(&value.read().clone(), move |text: &String| {
                let mut list = extras.read().clone();
                if let Some(entry) = list.iter_mut().find(|entry| entry.key == key)
                    && entry.value != *text
                {
                    entry.value = text.clone();
                    let mut extras = extras;
                    extras.set(list);
                }
            });
        }
        let remove_key = key.clone();
        let mut field = text_field(value, |_: String| {}).monospace();
        if secret {
            field = field.password();
        }
        let remove = icon_button(Icon::UserTrash, move |_| {
            let mut list = extras.read().clone();
            list.retain(|entry| entry.key != remove_key);
            let mut extras = extras;
            extras.set(list);
        })
        .tooltip("Remove this setting");
        row(key).trailing(
            rect()
                .tw_layout(&[flex_row(), gap(S2), items(Align::Center)])
                .child(field)
                .child(remove),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::network::vpn::plugins::fields;

    fn openvpn_profile() -> PluginProfile {
        PluginProfile {
            uuid: "u".into(),
            service: "org.freedesktop.NetworkManager.openvpn".into(),
            user_name: String::new(),
            data: data::parse(
                "ca = /etc/ca.pem, cipher = AES-256-GCM, connection-type = password, \
                 dev = tun, password-flags = 1, remote = vpn.example.net:1194, username = kieran",
            ),
            secrets: data::parse("password = s3cret"),
        }
    }

    fn state_of(profile: &PluginProfile) -> (Vec<String>, Vec<String>) {
        let fields = fields("openvpn");
        (
            fields.iter().map(|f| initial_value(f, profile)).collect(),
            fields.iter().map(|f| initial_flags(f, profile)).collect(),
        )
    }

    #[test]
    fn unchanged_fields_compose_to_the_same_profile() {
        let profile = openvpn_profile();
        let (values, flags) = state_of(&profile);
        let extras = extras_of("openvpn", &profile);
        let composed = compose(
            "openvpn",
            &profile,
            fields("openvpn"),
            &values,
            &flags,
            &extras,
        );
        assert_eq!(normalised(composed), normalised(profile));
    }

    #[test]
    fn extras_are_the_keys_no_field_owns() {
        let extras = extras_of("openvpn", &openvpn_profile());
        let keys: Vec<&str> = extras.iter().map(|extra| extra.key.as_str()).collect();
        assert_eq!(keys, ["cipher", "dev"]);
        // An unknown plugin owns nothing but the user name.
        let other = PluginProfile {
            service: "org.freedesktop.NetworkManager.pptp".into(),
            data: data::parse("gateway = 1.2.3.4, user = bob"),
            secrets: data::parse("password = p"),
            ..PluginProfile::default()
        };
        let extras = extras_of("pptp", &other);
        assert_eq!(extras.len(), 3);
        assert!(
            extras
                .iter()
                .any(|extra| extra.key == "password" && extra.secret)
        );
    }

    #[test]
    fn editing_a_field_and_a_flag_changes_the_profile() {
        let profile = openvpn_profile();
        let (mut values, mut flags) = state_of(&profile);
        let fields = fields("openvpn");
        let at = |label: &str| fields.iter().position(|f| f.label == label).unwrap();
        values[at("Server")] = "other.example.net".into();
        flags[at("Password")] = "2".into();
        let extras = extras_of("openvpn", &profile);
        let composed = compose("openvpn", &profile, fields, &values, &flags, &extras);
        assert_eq!(
            data::get(&composed.data, "remote"),
            Some("other.example.net")
        );
        // Ask every time: the saved password is dropped and the flag written.
        assert_eq!(data::get(&composed.secrets, "password"), None);
        assert_eq!(data::get(&composed.data, "password-flags"), Some("2"));
    }

    #[test]
    fn flag_zero_is_not_written_and_extras_come_back() {
        let profile = openvpn_profile();
        let (values, mut flags) = state_of(&profile);
        let fields = fields("openvpn");
        let at = fields.iter().position(|f| f.label == "Password").unwrap();
        flags[at] = "0".into();
        let mut extras = extras_of("openvpn", &profile);
        extras.push(Extra {
            key: "reneg-seconds".into(),
            value: "3600".into(),
            secret: false,
        });
        let composed = compose("openvpn", &profile, fields, &values, &flags, &extras);
        assert_eq!(data::get(&composed.data, "password-flags"), None);
        assert_eq!(data::get(&composed.data, "reneg-seconds"), Some("3600"));
        assert_eq!(data::get(&composed.data, "cipher"), Some("AES-256-GCM"));
    }

    #[test]
    fn the_user_name_of_a_generic_plugin_is_vpn_user_name() {
        let profile = PluginProfile {
            service: "org.freedesktop.NetworkManager.pptp".into(),
            data: data::parse("gateway = 1.2.3.4"),
            ..PluginProfile::default()
        };
        let generic = fields("pptp");
        let composed = compose(
            "pptp",
            &profile,
            generic,
            &["bob".to_string()],
            &["0".to_string()],
            &extras_of("pptp", &profile),
        );
        assert_eq!(composed.user_name, "bob");
        assert_eq!(data::get(&composed.data, "gateway"), Some("1.2.3.4"));
    }

    #[test]
    fn required_and_checked_fields_block_apply() {
        let fields = fields("openvpn");
        let always = |_: &Field| true;
        let mut values: Vec<String> = fields.iter().map(|_| String::new()).collect();
        assert_eq!(
            field_error(fields, &values, &always).as_deref(),
            Some("Server is needed.")
        );
        values[0] = "vpn.example.com:1194".into();
        assert_eq!(field_error(fields, &values, &always), None);
        let at = fields
            .iter()
            .position(|f| f.label == "Server certificate (CA)")
            .unwrap();
        values[at] = "relative/ca.pem".into();
        assert!(field_error(fields, &values, &always).is_some());
        // A hidden field is not checked.
        assert_eq!(
            field_error(fields, &values, &|f: &Field| f.label == "Server"),
            None
        );
    }

    #[test]
    fn new_setting_names_are_checked() {
        let taken = vec!["cipher".to_string()];
        assert_eq!(new_key_error("", &taken), None);
        assert_eq!(new_key_error("reneg-seconds", &taken), None);
        assert!(new_key_error("a=b", &taken).is_some());
        assert!(new_key_error("cipher", &taken).is_some());
    }

    fn interface() -> Interface {
        Interface {
            private_key: "YFAnE0psgIdiAF7XR4abxiwVRnlMfeltxu2s8/4Q43k=".into(),
            addresses: vec!["10.8.0.2/32".into()],
            dns: vec!["10.8.0.1".into()],
            ..Interface::default()
        }
    }

    #[test]
    fn interface_errors() {
        assert_eq!(interface_error(&interface()), None);
        let mut bad = interface();
        bad.private_key = "short".into();
        assert!(interface_error(&bad).is_some());
        let mut bad = interface();
        bad.addresses.clear();
        assert!(interface_error(&bad).is_some());
        let mut bad = interface();
        bad.dns.push("fd08::1".into());
        let message = interface_error(&bad).expect("v6 dns without v6 address");
        assert!(message.contains("IPv6"));
        let mut bad = interface();
        bad.mtu = "12".into();
        assert!(interface_error(&bad).is_some());
    }

    #[test]
    fn peer_errors_include_duplicates() {
        let peer = Peer {
            public_key: "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=".into(),
            allowed_ips: vec!["10.8.0.0/24".into()],
            ..Peer::default()
        };
        assert_eq!(peer_error(&peer, "", &[]), None);
        assert!(peer_error(&peer, "", &[peer.public_key.as_str()]).is_some());
        let mut bad = peer.clone();
        bad.endpoint = "no-port".into();
        assert!(peer_error(&bad, "", &[]).is_some());
        assert!(peer_error(&peer, "70000", &[]).is_some());
    }

    #[test]
    fn peer_summaries() {
        let peer = Peer {
            endpoint: "vpn.example.com:51820".into(),
            allowed_ips: vec!["0.0.0.0/0".into(), "::/0".into()],
            ..Peer::default()
        };
        assert_eq!(peer_summary(&peer), "vpn.example.com:51820 · all traffic");
        let peer = Peer {
            allowed_ips: vec!["10.0.0.0/8".into(), "192.168.0.0/16".into()],
            ..Peer::default()
        };
        assert_eq!(peer_summary(&peer), "No endpoint · 2 address ranges");
    }
}
