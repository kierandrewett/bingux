//! The Advanced page: retries and name lookup for the connection, Wi-Fi's hidden network and band,
//! and for IPv4 and IPv6 whether they are required and how the connection announces itself and
//! ranks its routes.
//!
//! Switches and dropdowns apply at once. The DHCP host name and client ID and the route metrics are
//! text, so they wait in a draft for Apply.

use freya::prelude::*;

use bingux_ui::controls::form::validate::Check;
use bingux_ui::controls::form_field;
use bingux_ui::controls::pending_bar;

use super::Page;
use super::draft::{Draft, Values, use_draft};
use super::store;
use crate::pages::connection_editor::{
    ADDR_GEN_CHOICES, BAND_CHOICES, Changes, Choices, Kind, MULTICAST_DNS_CHOICES, ProfileData,
    RETRY_CHOICES, dhcp_hostname, diff, metric,
};
use crate::ui::{Group, choice_row, page_column, switch_row};

/// A dropdown's options, with the stored value added as it is when the list does not know it, so a
/// value set elsewhere (`nmcli`, another tool) shows as itself and is not mistaken for a choice.
pub(super) fn options_with(choices: &[(&str, &str)], current: &str) -> Vec<(String, String)> {
    let mut options = super::options(choices);
    if !options.iter().any(|(key, _)| key == current) {
        options.push((current.to_string(), current.to_string()));
    }
    options
}

/// The DHCP client ID: any text, without spaces at either end.
fn client_id(text: &str) -> Check {
    if text.trim() != text || text.len() > 255 {
        Err(
            "Enter mac, duid or a hex string, up to 255 characters, without spaces at either end."
                .to_string(),
        )
    } else {
        Ok(())
    }
}

/// A route metric: a whole number. Empty stands for automatic, which NetworkManager calls -1, so
/// -1 itself is not accepted; the field would not match what it reads back.
fn route_metric(text: &str) -> Check {
    metric(text)?;
    match text.parse::<u32>() {
        Ok(number) if number.to_string() == text => Ok(()),
        _ => Err("Enter a whole number, or leave the field empty for automatic.".to_string()),
    }
}

/// What the form holds, as typed.
#[derive(Clone, Debug, PartialEq)]
struct Form {
    hostname: String,
    client_id: String,
    metric4: String,
    metric6: String,
}

/// Which fields this profile has. A VPN has no DHCP of its own, and IPv4 only asks for a host name
/// and client ID when it is automatic.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    kind: Kind,
    dhcp: bool,
    addr_gen: bool,
}

fn layout(saved: &ProfileData) -> Layout {
    let kind = saved.kind();
    let method = |key: &str| match saved.get(key) {
        "" => "auto".to_string(),
        other => other.to_string(),
    };
    Layout {
        kind,
        dhcp: kind != Kind::Vpn && method("ipv4.method") == "auto",
        addr_gen: kind != Kind::Vpn
            && !matches!(method("ipv6.method").as_str(), "disabled" | "ignore"),
    }
}

/// The metric as the field shows it: empty when NetworkManager chooses.
fn stored_metric(saved: &ProfileData, key: &str) -> String {
    match saved.get(key) {
        "-1" => String::new(),
        other => other.to_string(),
    }
}

impl Form {
    fn from_saved(saved: &ProfileData) -> Self {
        Self {
            hostname: saved.get("ipv4.dhcp-hostname").to_string(),
            client_id: saved.get("ipv4.dhcp-client-id").to_string(),
            metric4: stored_metric(saved, "ipv4.route-metric"),
            metric6: stored_metric(saved, "ipv6.route-metric"),
        }
    }

    fn from_draft(draft: &Draft) -> Self {
        Self {
            hostname: draft.get("hostname"),
            client_id: draft.get("client_id"),
            metric4: draft.get("metric4"),
            metric6: draft.get("metric6"),
        }
    }

    fn values(self) -> Values {
        Values {
            texts: vec![
                ("hostname", self.hostname),
                ("client_id", self.client_id),
                ("metric4", self.metric4),
                ("metric6", self.metric6),
            ],
            lists: Vec::new(),
        }
    }

    /// Whether Apply may write the form: every field that is shown and not empty passes its check.
    fn valid(&self, layout: Layout) -> bool {
        let passes = |text: &str, check: fn(&str) -> Check| text.is_empty() || check(text).is_ok();
        (!layout.dhcp
            || (passes(&self.hostname, dhcp_hostname) && passes(&self.client_id, client_id)))
            && passes(&self.metric4, route_metric)
            && passes(&self.metric6, route_metric)
    }
}

fn read(saved: &ProfileData) -> Values {
    Form::from_saved(saved).values()
}

/// The values to write: what differs, for the fields this profile shows. An empty metric is written
/// as -1, and an empty host name or client ID clears the stored one.
fn changes(form: &Form, saved: &ProfileData, layout: Layout) -> Changes {
    let mut out = Changes::default();
    if layout.dhcp {
        diff(
            &mut out,
            "ipv4.dhcp-hostname",
            saved.get("ipv4.dhcp-hostname"),
            form.hostname.trim(),
        );
        diff(
            &mut out,
            "ipv4.dhcp-client-id",
            saved.get("ipv4.dhcp-client-id"),
            form.client_id.trim(),
        );
    }
    for (key, typed) in [
        ("ipv4.route-metric", &form.metric4),
        ("ipv6.route-metric", &form.metric6),
    ] {
        let typed = typed.trim();
        if stored_metric(saved, key) != typed {
            out.set(key, if typed.is_empty() { "-1" } else { typed });
        }
    }
    out
}

/// Whether a ternary switch such as `ipv4.dhcp-send-hostname` is on. nmcli prints `-1` for the
/// default, which sends the name, and `1` and `0` for yes and no.
fn sends_hostname(saved: &ProfileData) -> bool {
    !matches!(saved.get("ipv4.dhcp-send-hostname"), "0" | "no")
}

/// A dropdown that writes `key` as soon as a choice is made. `unset` is what an empty stored value means.
fn choice_now(
    page: &Page,
    title: &'static str,
    key: &'static str,
    choices: Choices,
    unset: &'static str,
    caption: &'static str,
) -> Element {
    let current = match page.saved.get(key) {
        "" => unset,
        other => other,
    }
    .to_string();
    let page = page.clone();
    choice_row(
        title,
        options_with(choices, &current),
        &current,
        move |value: String| page.apply_now(Changes::one(key, value)),
    )
    .subtitle(caption)
    .into()
}

/// A switch that writes `key` as soon as it is flipped: `values.0` when it turns on, `values.1` when off.
fn switch_now(
    page: &Page,
    title: &'static str,
    key: &'static str,
    on: bool,
    values: (&'static str, &'static str),
    caption: &'static str,
) -> Element {
    let page = page.clone();
    switch_row(title, on, move |on: bool| {
        page.apply_now(Changes::one(key, if on { values.0 } else { values.1 }))
    })
    .subtitle(caption)
    .into()
}

/// The Advanced page of a connection.
#[derive(PartialEq)]
pub struct AdvancedBody {
    pub page: Page,
}

impl Component for AdvancedBody {
    fn render(&self) -> impl IntoElement {
        let page = self.page.clone();
        let draft = use_draft(&page.saved, read);
        let busy = use_state(|| false);

        let saved = &page.saved;
        let layout = layout(saved);
        let dirty = draft.dirty(saved);
        let ok = Form::from_draft(&draft).valid(layout);

        let apply = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| {
                let changes = changes(&Form::from_draft(&draft), &page.saved, layout);
                if changes.is_empty() {
                    return;
                }
                let after = page.saved.with(&changes);
                let draft = draft.clone();
                store::apply_batch(
                    page.ctx,
                    &page.owner,
                    page.state,
                    busy,
                    changes,
                    move || draft.reset(&after),
                );
            }
        };
        let revert = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| draft.reset(&page.saved)
        };

        let mut connection = Group::titled("Connection")
            .row(choice_now(
                &page,
                "Connection retries",
                "connection.autoconnect-retries",
                RETRY_CHOICES,
                "-1",
                "How often to try again when connecting automatically fails.",
            ))
            .row(choice_now(
                &page,
                "Multicast DNS",
                "connection.mdns",
                MULTICAST_DNS_CHOICES,
                "-1",
                "Finds devices on this network by name, such as printer.local.",
            ))
            .row(choice_now(
                &page,
                "LLMNR",
                "connection.llmnr",
                MULTICAST_DNS_CHOICES,
                "-1",
                "An older way to find computers on this network by name.",
            ));
        if layout.kind == Kind::Wifi {
            connection = connection
                .row(switch_now(
                    &page,
                    "Hidden network",
                    "802-11-wireless.hidden",
                    saved.yes("802-11-wireless.hidden"),
                    ("yes", "no"),
                    "Turn on when the network does not show up in the list of networks.",
                ))
                .row(choice_now(
                    &page,
                    "Frequency band",
                    "802-11-wireless.band",
                    BAND_CHOICES,
                    "",
                    "Connect on one band only. Automatic uses either.",
                ));
        }

        let mut ipv4 = Group::titled("IPv4").row(switch_now(
            &page,
            "Require IPv4 to connect",
            "ipv4.may-fail",
            saved.get("ipv4.may-fail") == "no",
            ("no", "yes"),
            "The connection counts as failed when IPv4 does not work.",
        ));
        if layout.dhcp {
            ipv4 = ipv4
                .row(switch_now(
                    &page,
                    "Send host name",
                    "ipv4.dhcp-send-hostname",
                    sends_hostname(saved),
                    ("yes", "no"),
                    "Tells the network this computer's name when asking for an address.",
                ))
                .row(
                    form_field("Host name", draft.text("hostname"))
                        .hint("Leave empty to send this computer's own name.")
                        .optional()
                        .validate(dhcp_hostname),
                )
                .row(
                    form_field("Client ID", draft.text("client_id"))
                        .hint("mac, duid, or a hex string")
                        .optional()
                        .monospace()
                        .validate(client_id),
                );
        }
        ipv4 = ipv4.row(
            form_field("Route metric", draft.text("metric4"))
                .placeholder("Automatic")
                .hint("Lower numbers are preferred.")
                .optional()
                .validate(route_metric),
        );

        let mut ipv6 = Group::titled("IPv6").row(switch_now(
            &page,
            "Require IPv6 to connect",
            "ipv6.may-fail",
            saved.get("ipv6.may-fail") == "no",
            ("no", "yes"),
            "The connection counts as failed when IPv6 does not work.",
        ));
        if layout.addr_gen {
            ipv6 = ipv6.row(choice_now(
                &page,
                "Address generation",
                "ipv6.addr-gen-mode",
                ADDR_GEN_CHOICES,
                "default",
                "How the connection makes its own IPv6 address.",
            ));
        }
        ipv6 = ipv6.row(
            form_field("Route metric", draft.text("metric6"))
                .placeholder("Automatic")
                .hint("Lower numbers are preferred.")
                .optional()
                .validate(route_metric),
        );

        page_column()
            .maybe_child(pending_bar(dirty, ok && !*busy.read(), apply, revert))
            .child(connection)
            .child(ipv4)
            .child(ipv6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(extra: &str) -> ProfileData {
        ProfileData::parse(&format!(
            "connection.type:802-3-ethernet\nipv4.method:auto\nipv6.method:auto\nipv4.route-metric:-1\nipv6.route-metric:-1\n{extra}"
        ))
    }

    #[test]
    fn defaults_write_nothing() {
        let profile = saved("");
        let form = Form::from_saved(&profile);
        assert!(changes(&form, &profile, layout(&profile)).is_empty());
        assert!(form.valid(layout(&profile)));
    }

    #[test]
    fn dhcp_fields_follow_the_ipv4_method_and_kind() {
        assert!(layout(&saved("")).dhcp);
        assert!(!layout(&saved("ipv4.method:manual\n")).dhcp);
        let vpn =
            ProfileData::parse("connection.type:wireguard\nipv4.method:auto\nipv6.method:auto\n");
        let layout = layout(&vpn);
        assert!(!layout.dhcp && !layout.addr_gen);
        assert!(!super::layout(&saved("ipv6.method:ignore\n")).addr_gen);
        assert!(super::layout(&saved("ipv6.method:manual\n")).addr_gen);
    }

    #[test]
    fn host_name_and_client_id_are_written_trimmed_and_cleared_when_empty() {
        let profile = saved("");
        let mut form = Form::from_saved(&profile);
        form.hostname = " desk ".into();
        form.client_id = "duid".into();
        assert_eq!(
            changes(&form, &profile, layout(&profile)).commands("Office"),
            vec![vec![
                "connection",
                "modify",
                "Office",
                "ipv4.dhcp-hostname",
                "desk",
                "ipv4.dhcp-client-id",
                "duid"
            ]]
        );
        let set = saved("ipv4.dhcp-hostname:desk\nipv4.dhcp-client-id:duid\n");
        let mut cleared = Form::from_saved(&set);
        cleared.hostname.clear();
        cleared.client_id.clear();
        assert_eq!(
            changes(&cleared, &set, layout(&set)).set,
            vec![
                ("ipv4.dhcp-hostname".to_string(), String::new()),
                ("ipv4.dhcp-client-id".to_string(), String::new())
            ]
        );
    }

    #[test]
    fn dhcp_fields_are_left_alone_when_ipv4_is_not_automatic() {
        let profile = saved("ipv4.method:manual\nipv4.dhcp-hostname:desk\n");
        let mut form = Form::from_saved(&profile);
        form.hostname = "other".into();
        assert!(changes(&form, &profile, layout(&profile)).is_empty());
        form.hostname = "two words".into();
        assert!(form.valid(layout(&profile)));
    }

    #[test]
    fn metrics_write_minus_one_for_empty() {
        let profile = saved("");
        let mut form = Form::from_saved(&profile);
        form.metric4 = "200".into();
        form.metric6 = " 300 ".into();
        assert_eq!(
            changes(&form, &profile, layout(&profile)).set,
            vec![
                ("ipv4.route-metric".to_string(), "200".to_string()),
                ("ipv6.route-metric".to_string(), "300".to_string())
            ]
        );
        let ranked = saved("ipv4.route-metric:200\n");
        let mut back = Form::from_saved(&ranked);
        assert_eq!(back.metric4, "200");
        back.metric4.clear();
        assert_eq!(
            changes(&back, &ranked, layout(&ranked)).set,
            vec![("ipv4.route-metric".to_string(), "-1".to_string())]
        );
    }

    #[test]
    fn validity() {
        let profile = saved("");
        let layout = layout(&profile);
        let mut form = Form::from_saved(&profile);
        form.hostname = "has space".into();
        assert!(!form.valid(layout));
        form.hostname = "desk".into();
        form.client_id = " lead".into();
        assert!(!form.valid(layout));
        form.client_id = "01:aa:bb".into();
        form.metric4 = "-1".into();
        assert!(!form.valid(layout), "empty means automatic");
        form.metric4 = "12x".into();
        assert!(!form.valid(layout));
        form.metric4 = "100".into();
        form.metric6 = "007".into();
        assert!(!form.valid(layout));
        form.metric6 = "600".into();
        assert!(form.valid(layout));
    }

    #[test]
    fn send_host_name_reads_nmcli_numbers() {
        assert!(sends_hostname(&saved("ipv4.dhcp-send-hostname:-1\n")));
        assert!(sends_hostname(&saved("ipv4.dhcp-send-hostname:1\n")));
        assert!(sends_hostname(&saved("")));
        assert!(!sends_hostname(&saved("ipv4.dhcp-send-hostname:0\n")));
        assert!(!sends_hostname(&saved("ipv4.dhcp-send-hostname:no\n")));
    }

    #[test]
    fn unknown_stored_values_show_as_themselves() {
        let options = options_with(ADDR_GEN_CHOICES, "default-or-eui64");
        assert_eq!(options.last().unwrap().0, "default-or-eui64");
        assert_eq!(
            options_with(ADDR_GEN_CHOICES, "eui64").len(),
            ADDR_GEN_CHOICES.len()
        );
    }
}
