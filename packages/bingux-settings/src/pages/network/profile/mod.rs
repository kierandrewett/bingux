//! One connection's settings, shared by Wi-Fi networks, wired profiles and VPNs.
//!
//! [`ProfileSettings`] is what a connection's screen puts below its status: the General group
//! (applied at once) and a row for each deeper settings page. [`SectionPage`] is one of those
//! pages: live state at the top, then the settings of IPv4, IPv6, DNS, Security, Identity, Proxy or
//! Advanced.
//!
//! Saving follows one rule everywhere. A switch or dropdown writes with `nmcli connection modify` as
//! soon as it changes, and the control shows the new value before the command ends. A form whose
//! fields only make sense together (a manual address with its gateway, 802.1X credentials, a
//! password) keeps what is typed in a draft and shows the apply bar while the draft differs from the
//! profile; Apply is disabled while any field is invalid, and invalid text is never written. After a
//! change to a running connection, `nmcli device reapply` takes it into the live link, or a toast
//! offers a reconnect when NetworkManager cannot.

mod advanced;
mod dns;
mod draft;
mod identity;
mod ip;
mod proxy;
mod security;
mod store;

use freya::prelude::*;

use bingux_ui::style::*;

use super::failure_page;
use super::model::{NetCtx, use_net_ctx};
use super::{Owner, Section};
use crate::pages::connection_editor::{
    CLONED_MAC_CHOICES, Changes, Kind, METERED_CHOICES, ProfileData, WIFI_SECURITY_CHOICES,
    dns_summary, dot1x_summary, ip_summary, label_of, permissions_value, split_cloned_mac,
    wifi_security_kind, yes_no,
};
use crate::pages::loading::veiled;
use crate::pages::network::Device;
use crate::routes::Route;
use crate::ui::{
    Group, Status, choice_row, live_state, page_column, row, status, stepper, switch_row,
};
use store::ProfileState;

/// What every settings page needs: the screen's context, whose profile it is, and the profile as
/// last read.
#[derive(Clone, PartialEq)]
struct Page {
    ctx: NetCtx,
    owner: Owner,
    state: ProfileState,
    saved: ProfileData,
}

impl Page {
    /// Write one switch or dropdown now.
    fn apply_now(&self, changes: Changes) {
        store::apply_now(self.ctx, &self.owner, self.state, changes);
    }

    fn kind(&self) -> Kind {
        owner_kind(&self.owner)
    }
}

fn owner_kind(owner: &Owner) -> Kind {
    match owner {
        Owner::Wifi { .. } => Kind::Wifi,
        Owner::Wired { .. } => Kind::Wired,
        Owner::Vpn { .. } => Kind::Vpn,
    }
}

/// The General group and the rows that open a connection's settings pages.
/// A connection screen puts this below its status.
#[derive(PartialEq)]
pub struct ProfileSettings {
    pub owner: Owner,
}

impl Component for ProfileSettings {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let state = store::use_profile(&self.owner);
        let loaded = state.read().clone();
        let loading = loaded.is_none();
        let saved = match loaded {
            None => ProfileData::placeholder(owner_kind(&self.owner)),
            Some(Ok(saved)) => saved,
            Some(Err(error)) => {
                let name = self.owner.connection_name().to_string();
                return failure_page(error, move || store::reload(&name));
            }
        };
        let page = Page {
            ctx,
            owner: self.owner.clone(),
            state,
            saved,
        };
        veiled(
            page_column()
                .child(general_group(&page))
                .child(settings_group(&page)),
            loading,
        )
        .into()
    }
}

/// Connect automatically, its priority, who may use the profile and whether it is metered.
fn general_group(page: &Page) -> Element {
    let saved = &page.saved;
    let autoconnect = {
        let page = page.clone();
        switch_row(
            "Connect automatically",
            saved.yes("connection.autoconnect"),
            move |on: bool| page.apply_now(Changes::one("connection.autoconnect", yes_no(on))),
        )
    };
    let priority = {
        let page = page.clone();
        row("Priority")
            .subtitle("Higher priorities connect first when several connections are available.")
            .trailing(
                stepper(
                    saved.int("connection.autoconnect-priority").unwrap_or(0),
                    move |value: i64| {
                        page.apply_now(Changes::one(
                            "connection.autoconnect-priority",
                            value.to_string(),
                        ))
                    },
                )
                .range(-999, 999),
            )
    };
    let everyone = {
        let page = page.clone();
        switch_row(
            "Available to other users",
            saved.get("connection.permissions").is_empty(),
            move |on: bool| {
                page.apply_now(Changes::one(
                    "connection.permissions",
                    permissions_value(on),
                ))
            },
        )
    };
    let metered = {
        let page = page.clone();
        choice_row(
            "Metered connection",
            options(METERED_CHOICES),
            saved.get("connection.metered"),
            move |value: String| page.apply_now(Changes::one("connection.metered", value)),
        )
        .subtitle("Apps may use less data on a metered connection.")
    };
    Group::titled("General")
        .row(autoconnect)
        .row(priority)
        .row(everyone)
        .row(metered)
        .into()
}

/// A dropdown's `(key, label)` pairs as owned strings.
fn options(choices: &[(&str, &str)]) -> Vec<(String, String)> {
    choices
        .iter()
        .map(|(key, label)| (key.to_string(), label.to_string()))
        .collect()
}

/// A row that opens `section` of the connection, with the current value at its right.
fn section_row(page: &Page, section: Section, value: impl Into<String>) -> Element {
    let route = Route::Section {
        owner: page.owner.clone(),
        section: section.clone(),
    };
    let navigator = page.ctx.navigator;
    row(section.title())
        .value(value)
        .navigates()
        .on_press(move |_| navigator.push(route.clone()))
        .into()
}

/// The rows to the settings pages that exist for this kind of connection.
fn settings_group(page: &Page) -> Element {
    let saved = &page.saved;
    let kind = page.kind();
    let mut group = Group::titled("Settings");
    if kind == Kind::Vpn {
        group = group.row(section_row(page, Section::Tunnel, ""));
    }
    group = group
        .row(section_row(page, Section::Ipv4, ip_summary(saved, 4, kind)))
        .row(section_row(page, Section::Ipv6, ip_summary(saved, 6, kind)))
        .row(section_row(page, Section::Dns, dns_summary(saved)));
    match kind {
        Kind::Wifi => {
            group = group.row(section_row(
                page,
                Section::Security,
                label_of(WIFI_SECURITY_CHOICES, wifi_security_kind(saved)),
            ));
        }
        Kind::Wired => {
            group = group.row(section_row(page, Section::Security, dot1x_summary(saved)));
        }
        Kind::Vpn => {}
    }
    if kind != Kind::Vpn {
        group = group.row(section_row(
            page,
            Section::Identity,
            identity_summary(saved),
        ));
    }
    group = group
        .row(section_row(
            page,
            Section::Proxy,
            if saved.get("proxy.method") == "auto" {
                "Automatic"
            } else {
                "None"
            },
        ))
        .row(section_row(page, Section::Advanced, ""));
    group.into()
}

/// What differs from the default hardware settings: a cloned address, a set MTU, or `Default`.
fn identity_summary(saved: &ProfileData) -> String {
    let setting = saved.device_setting();
    let (mode, _) = split_cloned_mac(saved.get(&format!("{setting}.cloned-mac-address")));
    let mtu = saved.get(&format!("{setting}.mtu"));
    let mut parts = Vec::new();
    if !mode.is_empty() {
        parts.push(format!("{} MAC", label_of(CLONED_MAC_CHOICES, &mode)));
    }
    if !mtu.is_empty() && mtu != "auto" {
        parts.push(format!("MTU {mtu}"));
    }
    if parts.is_empty() {
        "Default".to_string()
    } else {
        parts.join(" · ")
    }
}

/// One section of a connection's settings, as its own subpage.
#[derive(PartialEq)]
pub struct SectionPage {
    pub owner: Owner,
    pub section: Section,
}

impl Component for SectionPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let state = store::use_profile(&self.owner);
        let loaded = state.read().clone();
        let loading = loaded.is_none();
        let saved = match loaded {
            None => ProfileData::placeholder(owner_kind(&self.owner)),
            Some(Ok(saved)) => saved,
            Some(Err(error)) => {
                let name = self.owner.connection_name().to_string();
                return failure_page(error, move || store::reload(&name));
            }
        };
        let page = Page {
            ctx,
            owner: self.owner.clone(),
            state,
            saved,
        };
        let live = live_rows(&page, &self.section);
        // The body keeps what is typed in its own state, so it is created again once, when the
        // first read lands, and never when the profile is read again.
        let body: Element = match &self.section {
            Section::Ipv4 => keyed(loading, ip::IpBody { page, version: 4 }),
            Section::Ipv6 => keyed(loading, ip::IpBody { page, version: 6 }),
            Section::Dns => keyed(loading, dns::DnsBody { page }),
            Section::Security => keyed(loading, security::SecurityBody { page }),
            Section::Identity => keyed(loading, identity::IdentityBody { page }),
            Section::Proxy => keyed(loading, proxy::ProxyBody { page }),
            Section::Advanced => keyed(loading, advanced::AdvancedBody { page }),
            Section::Tunnel | Section::Peer(_) => {
                status(Status::Empty("Nothing to set here.".into())).into()
            }
        };
        veiled(page_column().maybe_child(live).child(body), loading).into()
    }
}

/// `child` under a wrapper that is created again whenever `key` changes, so a form starts over with
/// the values it is given then.
pub(super) fn keyed(key: impl std::hash::Hash, child: impl Into<Element>) -> Element {
    rect()
        .key(key)
        .tw_layout(&[w_full()])
        .child(child.into())
        .into()
}

/// A link speed as a person says it: `1 Gbit/s`, `100 Mbit/s`.
pub fn speed_text(mbit: u32) -> String {
    if mbit >= 1000 && mbit % 1000 == 0 {
        format!("{} Gbit/s", mbit / 1000)
    } else if mbit >= 1000 {
        format!("{:.1} Gbit/s", f64::from(mbit) / 1000.0)
    } else {
        format!("{mbit} Mbit/s")
    }
}

/// What the running connection has right now, for the top of a page that sets it: the address,
/// gateway and DNS servers the settings below produced. `None` when the connection is not running.
fn live_rows(page: &Page, section: &Section) -> Option<Element> {
    let device = store::device_of(page.ctx, &page.owner)?;
    let rows = live_facts(&device, section);
    (!rows.is_empty()).then(|| live_state(rows))
}

fn live_facts(device: &Device, section: &Section) -> Vec<(String, String)> {
    let speed = device.speed.map(speed_text).unwrap_or_default();
    let all: Vec<(&str, String)> = match section {
        Section::Ipv4 => vec![
            ("IP address", device.ipv4.join(", ")),
            ("Gateway", device.gateway4.clone()),
            ("DNS", device.dns.join(", ")),
            ("Link speed", speed),
        ],
        Section::Ipv6 => vec![
            ("IP address", device.ipv6.join(", ")),
            ("Gateway", device.gateway6.clone()),
        ],
        Section::Dns => vec![("DNS servers", device.dns.join(", "))],
        Section::Identity => vec![
            ("Hardware address", device.hwaddr.clone()),
            (
                "MTU",
                device.mtu.map(|mtu| mtu.to_string()).unwrap_or_default(),
            ),
            ("Link speed", speed),
        ],
        _ => Vec::new(),
    };
    all.into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(label, value)| (label.to_string(), value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_speed_reads_naturally() {
        assert_eq!(speed_text(100), "100 Mbit/s");
        assert_eq!(speed_text(1000), "1 Gbit/s");
        assert_eq!(speed_text(2500), "2.5 Gbit/s");
    }

    #[test]
    fn identity_summary_names_only_what_differs() {
        let plain = ProfileData::parse("connection.type:802-3-ethernet\n802-3-ethernet.mtu:auto\n");
        assert_eq!(identity_summary(&plain), "Default");
        let custom = ProfileData::parse(
            "connection.type:802-3-ethernet\n802-3-ethernet.cloned-mac-address:random\n802-3-ethernet.mtu:1400\n",
        );
        assert_eq!(identity_summary(&custom), "Random MAC · MTU 1400");
    }
}
