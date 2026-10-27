//! One connection's settings, shared by Wi-Fi networks, wired profiles and VPNs.
//!
//! [`ProfileSettings`] is what a connection's page puts below its everyday facts: a group of the
//! settings people change (connect automatically, metered, the Wi-Fi password, a VPN's tunnel) and
//! one Advanced row. [`SectionPage`] is one page below it. The Advanced page lists every editor
//! (IPv4, IPv6, DNS, Security, Identity, Proxy, the lesser options), the connection's behaviour and,
//! at the bottom, its read-only technical details; each editor is a page of its own below Advanced.
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

use super::details::TechnicalDetails;
use super::failure_page;
use super::model::{NetCtx, use_net_ctx};
use super::{Owner, Section};
use crate::pages::connection_editor::{
    CLONED_MAC_CHOICES, Changes, Kind, METERED_CHOICES, ProfileData, WIFI_SECURITY_CHOICES,
    dns_summary, dot1x_summary, ip_summary, label_of, permissions_value, split_cloned_mac,
    wifi_security_kind, yes_no,
};
use crate::pages::loading::veiled;
use crate::routes::Route;
use crate::ui::{
    Group, PageHead, Status, choice_row, page_column, row, status, stepper, switch_row,
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

/// The everyday settings of a connection and its Advanced row. A connection's page puts this
/// below its facts.
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
                .child(everyday_group(&page))
                .child(advanced_row(&page)),
            loading,
        )
        .into()
    }
}

/// Connect automatically and whether the connection is metered, then what belongs to this kind of
/// connection: a Wi-Fi network's password, a VPN's tunnel.
fn everyday_group(page: &Page) -> Element {
    let saved = &page.saved;
    let autoconnect = {
        let page = page.clone();
        switch_row(
            "Connect automatically",
            saved.yes("connection.autoconnect"),
            move |on: bool| page.apply_now(Changes::one("connection.autoconnect", yes_no(on))),
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
    let mut group = Group::new(None::<String>).row(autoconnect).row(metered);
    match page.kind() {
        Kind::Wifi => {
            let key_mgmt = saved.get("802-11-wireless-security.key-mgmt");
            if !key_mgmt.is_empty() && wifi_security_kind(saved) != "none" {
                group = group.row(Element::from(PasswordRow {
                    password: password_of(saved),
                }));
            }
        }
        Kind::Vpn => {
            group = group.row(section_row(page, Section::Tunnel, ""));
        }
        Kind::Wired => {}
    }
    group.into()
}

/// The saved key of a Wi-Fi profile, when NetworkManager keeps it itself.
fn password_of(saved: &ProfileData) -> String {
    let psk = saved.get("802-11-wireless-security.psk");
    if psk.is_empty() {
        saved.get("802-11-wireless-security.wep-key0").to_string()
    } else {
        psk.to_string()
    }
}

/// The Wi-Fi password: dots, with Show and Copy. Changing it is on the Security page.
#[derive(PartialEq)]
struct PasswordRow {
    password: String,
}

impl Component for PasswordRow {
    fn render(&self) -> impl IntoElement {
        let overlays = crate::ui::use_overlays();
        let shown = use_state(|| false);
        if self.password.is_empty() {
            return Element::from(
                row("Password")
                    .subtitle("Not saved here. It is asked for when connecting.")
                    .subtitle_one_line(),
            );
        }
        Element::from(super::wifi::secret_row(
            "Password",
            &self.password,
            shown,
            overlays,
        ))
    }
}

/// A dropdown's `(key, label)` pairs as owned strings.
fn options(choices: &[(&str, &str)]) -> Vec<(String, String)> {
    choices
        .iter()
        .map(|(key, label)| (key.to_string(), label.to_string()))
        .collect()
}

/// A row that opens `section` of the connection, with the current value at its right, or under
/// the title on a phone, where there is no room beside it.
fn section_row(page: &Page, section: Section, value: impl Into<String>) -> Element {
    let route = Route::Section {
        owner: page.owner.clone(),
        section: section.clone(),
    };
    let navigator = page.ctx.navigator;
    let value = value.into();
    let line = row(section.title());
    let line = if is_compact() {
        line.subtitle(value)
    } else {
        line.value(value)
    };
    line.navigates()
        .on_press(move |_| navigator.push(route.clone()))
        .into()
}

/// The one row to everything else, with what differs from the defaults under it.
fn advanced_row(page: &Page) -> Element {
    let route = Route::Section {
        owner: page.owner.clone(),
        section: Section::Advanced,
    };
    let navigator = page.ctx.navigator;
    Group::new(None::<String>)
        .row(
            row("Advanced")
                .subtitle(advanced_summary(&page.saved, page.kind()))
                .navigates()
                .on_press(move |_| navigator.push(route.clone())),
        )
        .into()
}

/// What the connection sets that is not the default, in a few words for the Advanced row.
fn advanced_summary(saved: &ProfileData, kind: Kind) -> String {
    let mut parts: Vec<String> = Vec::new();
    if saved.get("ipv4.method") == "manual" {
        parts.push("Manual IPv4".to_string());
    }
    match saved.get("ipv6.method") {
        "manual" => parts.push("Manual IPv6".to_string()),
        "disabled" | "ignore" => parts.push("IPv6 off".to_string()),
        _ => {}
    }
    if dns_summary(saved) != "Automatic" {
        parts.push("Custom DNS".to_string());
    }
    if saved.get("proxy.method") == "auto" {
        parts.push("Proxy".to_string());
    }
    if kind != Kind::Vpn {
        let identity = identity_summary(saved);
        if identity != "Default" {
            parts.push(identity);
        }
    }
    if kind == Kind::Wired && dot1x_summary(saved) == "802.1X on" {
        parts.push("802.1X".to_string());
    }
    let priority = saved.int("connection.autoconnect-priority").unwrap_or(0);
    if priority != 0 {
        parts.push(format!("Priority {priority}"));
    }
    if !saved.get("connection.permissions").is_empty() {
        parts.push("Only for you".to_string());
    }
    if parts.is_empty() {
        "Addresses, DNS, security and technical details".to_string()
    } else {
        parts.join(", ")
    }
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

/// One page below a connection's page: the Advanced page or one of its editors. `extra` is a group
/// the Advanced page of a kind of connection adds above the technical details.
#[derive(PartialEq)]
pub struct SectionPage {
    pub owner: Owner,
    pub section: Section,
    pub extra: Option<Element>,
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
        // The title says what the page is; the line under it says whose it is.
        let head = PageHead::new().subtitle(self.owner.connection_name().to_string());
        // The body keeps what is typed in its own state, so it is created again once, when the
        // first read lands, and never when the profile is read again.
        let body: Element = match &self.section {
            Section::Advanced => advanced_page(&page, self.extra.clone()),
            Section::Ipv4 => keyed(loading, ip::IpBody { page, version: 4 }),
            Section::Ipv6 => keyed(loading, ip::IpBody { page, version: 6 }),
            Section::Dns => keyed(loading, dns::DnsBody { page }),
            Section::Security => keyed(loading, security::SecurityBody { page }),
            Section::Identity => keyed(loading, identity::IdentityBody { page }),
            Section::Proxy => keyed(loading, proxy::ProxyBody { page }),
            Section::Options => keyed(loading, advanced::AdvancedBody { page }),
            Section::Usage | Section::Tunnel | Section::Peer(_) => {
                status(Status::Empty("Nothing to set here.".into())).into()
            }
        };
        veiled(page_column().child(head).child(body), loading).into()
    }
}

/// The Advanced page: the editors, how the connection behaves, then its technical details.
fn advanced_page(page: &Page, extra: Option<Element>) -> Element {
    let saved = &page.saved;
    let kind = page.kind();
    let addressing = Group::titled("Addressing")
        .row(section_row(page, Section::Ipv4, ip_summary(saved, 4, kind)))
        .row(section_row(page, Section::Ipv6, ip_summary(saved, 6, kind)))
        .row(section_row(page, Section::Dns, dns_summary(saved)));

    let mut identity = Group::new(None::<String>);
    match kind {
        Kind::Wifi => {
            identity = identity.row(section_row(
                page,
                Section::Security,
                label_of(WIFI_SECURITY_CHOICES, wifi_security_kind(saved)),
            ));
        }
        Kind::Wired => {
            identity = identity.row(section_row(page, Section::Security, dot1x_summary(saved)));
        }
        Kind::Vpn => {}
    }
    if kind != Kind::Vpn {
        identity = identity.row(section_row(
            page,
            Section::Identity,
            identity_summary(saved),
        ));
    }
    identity = identity.row(section_row(
        page,
        Section::Proxy,
        if saved.get("proxy.method") == "auto" {
            "Automatic"
        } else {
            "None"
        },
    ));

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
    let behaviour = Group::titled("Behaviour")
        .row(priority)
        .row(everyone)
        .row(section_row(page, Section::Options, ""));

    let mut column = page_column()
        .child(addressing)
        .child(identity)
        .child(behaviour)
        .maybe_child(extra);
    column = column.child(keyed(
        page.owner.connection_name().to_string(),
        TechnicalDetails {
            connection: page.owner.connection_name().to_string(),
        },
    ));
    column.into()
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

    #[test]
    fn the_advanced_row_names_only_what_is_not_default() {
        let plain = ProfileData::parse(
            "connection.type:802-3-ethernet\nipv4.method:auto\nipv6.method:auto\n802-3-ethernet.mtu:auto\n",
        );
        assert_eq!(
            advanced_summary(&plain, Kind::Wired),
            "Addresses, DNS, security and technical details"
        );
        let custom = ProfileData::parse(
            "connection.type:802-3-ethernet\nipv4.method:manual\nipv4.dns:1.1.1.1\nipv6.method:disabled\nconnection.autoconnect-priority:5\n",
        );
        assert_eq!(
            advanced_summary(&custom, Kind::Wired),
            "Manual IPv4, IPv6 off, Custom DNS, Priority 5"
        );
    }

    #[test]
    fn a_wifi_password_is_the_psk_or_the_wep_key() {
        let psk = ProfileData::parse("802-11-wireless-security.psk:secret12\n");
        assert_eq!(password_of(&psk), "secret12");
        let wep = ProfileData::parse("802-11-wireless-security.wep-key0:abcde\n");
        assert_eq!(password_of(&wep), "abcde");
        assert_eq!(password_of(&ProfileData::default()), "");
    }
}
