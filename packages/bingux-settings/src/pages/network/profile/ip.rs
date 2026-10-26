//! The IPv4 and IPv6 pages: how the connection gets its address, and the routes it adds.

use freya::prelude::*;

use bingux_ui::controls::form::validate;
use bingux_ui::controls::{form_field, pending_bar};

use super::draft::{Draft, ListLook, Values, list_rows, remove_slot, use_draft};
use super::store;
use super::{Page, options};
use crate::pages::connection_editor::{
    Changes, IP6_PRIVACY_CHOICES, ProfileData, diff, ip_methods, join_list, method_caption, route4,
    route6, shown, yes_no,
};
use crate::ui::{Group, choice_row, page_column, switch_row};

/// What the form starts with, for the IP family whose settings start with `prefix`.
fn read(saved: &ProfileData, prefix: &str) -> Values {
    let key = |suffix: &str| format!("{prefix}.{suffix}");
    let method = saved.get(&key("method"));
    Values {
        texts: vec![
            (
                "method",
                if method.is_empty() { "auto" } else { method }.to_string(),
            ),
            ("gateway", saved.get(&key("gateway")).to_string()),
        ],
        lists: vec![
            ("addresses", saved.list(&key("addresses"))),
            ("routes", saved.list(&key("routes"))),
        ],
    }
}

fn read4(saved: &ProfileData) -> Values {
    read(saved, "ipv4")
}

fn read6(saved: &ProfileData) -> Values {
    read(saved, "ipv6")
}

/// The IPv4 or IPv6 page of a connection.
#[derive(PartialEq)]
pub struct IpBody {
    pub page: Page,
    pub version: u8,
}

impl IpBody {
    fn prefix(&self) -> &'static str {
        if self.version == 4 { "ipv4" } else { "ipv6" }
    }
}

/// Whether the family keeps no addresses, routes or DNS servers of its own under `method`:
/// NetworkManager refuses a profile that has them.
pub fn method_is_bare(method: &str) -> bool {
    matches!(method, "link-local" | "shared" | "disabled" | "ignore")
}

/// What the form holds, as plain values.
#[derive(Clone, Debug, PartialEq)]
struct Form {
    method: String,
    gateway: String,
    addresses: Vec<String>,
    routes: Vec<String>,
}

impl Form {
    fn of(draft: &Draft) -> Self {
        Self {
            method: draft.get("method"),
            gateway: draft.get("gateway"),
            addresses: draft.list("addresses").texts(),
            routes: draft.list("routes").texts(),
        }
    }

    /// Whether the form can be applied: a manual method needs a valid address, and everything typed is valid.
    fn valid(&self, version: u8) -> bool {
        let visible = shown(version, &self.method);
        let address_check = if version == 4 {
            validate::ipv4_cidr
        } else {
            validate::ipv6_cidr
        };
        let gateway_check = if version == 4 {
            validate::ipv4
        } else {
            validate::ipv6
        };
        let route_check = if version == 4 { route4 } else { route6 };
        if self.method == "manual" {
            if self.addresses.is_empty()
                || !self
                    .addresses
                    .iter()
                    .all(|address| address_check(address).is_ok())
            {
                return false;
            }
            let gateway = self.gateway.trim();
            if !gateway.is_empty() && gateway_check(gateway).is_err() {
                return false;
            }
        }
        !visible.routes || self.routes.iter().all(|route| route_check(route).is_ok())
    }

    /// The values to write. Fields the method does not use are cleared, so NetworkManager does not
    /// refuse the profile for keeping them.
    fn changes(&self, saved: &ProfileData, prefix: &str, version: u8) -> Changes {
        let key = |suffix: &str| format!("{prefix}.{suffix}");
        let visible = shown(version, &self.method);
        let manual = self.method == "manual";
        let mut out = Changes::default();
        let stored_method = if saved.get(&key("method")).is_empty() {
            "auto"
        } else {
            saved.get(&key("method"))
        };
        diff(&mut out, &key("method"), stored_method, &self.method);
        let addresses = if manual {
            join_list(&self.addresses)
        } else {
            String::new()
        };
        diff(
            &mut out,
            &key("addresses"),
            &join_list(&saved.list(&key("addresses"))),
            &addresses,
        );
        let gateway = if manual {
            self.gateway.trim().to_string()
        } else {
            String::new()
        };
        diff(
            &mut out,
            &key("gateway"),
            saved.get(&key("gateway")),
            &gateway,
        );
        let routes = if visible.routes {
            join_list(&self.routes)
        } else {
            String::new()
        };
        diff(
            &mut out,
            &key("routes"),
            &join_list(&saved.list(&key("routes"))),
            &routes,
        );
        if method_is_bare(&self.method) {
            for suffix in ["dns", "dns-search"] {
                diff(&mut out, &key(suffix), saved.get(&key(suffix)), "");
            }
        }
        out
    }
}

impl Component for IpBody {
    fn render(&self) -> impl IntoElement {
        let page = self.page.clone();
        let version = self.version;
        let prefix = self.prefix();
        let draft = use_draft(&page.saved, if version == 4 { read4 } else { read6 });
        let busy = use_state(|| false);

        let method = draft.get("method");
        let visible = shown(version, &method);
        let dirty = draft.dirty(&page.saved);
        let ok = Form::of(&draft).valid(version);

        let apply = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| {
                let changes = Form::of(&draft).changes(&page.saved, prefix, version);
                if changes.is_empty() {
                    return;
                }
                store::apply_batch(page.ctx, &page.owner, page.state, busy, changes, || {});
            }
        };
        let revert = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| draft.reset(&page.saved)
        };

        let methods = ip_methods(version, page.kind());
        let method_row = {
            let draft = draft.clone();
            choice_row("Method", options(methods), &method, move |value: String| {
                draft.set("method", value)
            })
            .subtitle(method_caption(version, &method))
        };

        let mut column = page_column()
            .maybe_child(pending_bar(dirty, ok && !*busy.read(), apply, revert))
            .child(Group::titled(if version == 4 { "IPv4" } else { "IPv6" }).row(method_row));

        if visible.addresses {
            let (placeholder, gateway_placeholder, address_check, gateway_check): (
                &str,
                &str,
                fn(&str) -> validate::Check,
                fn(&str) -> validate::Check,
            ) = if version == 4 {
                (
                    "192.168.1.20/24",
                    "192.168.1.1",
                    validate::ipv4_cidr,
                    validate::ipv4,
                )
            } else {
                (
                    "2001:db8::20/64",
                    "2001:db8::1",
                    validate::ipv6_cidr,
                    validate::ipv6,
                )
            };
            let addresses = list_rows(
                draft.list("addresses"),
                ListLook {
                    title: "Address",
                    placeholder,
                    check: address_check,
                    add: "Add address",
                    monospace: true,
                },
            );
            column = column.child(
                Group::titled("Addresses")
                    .description("Each address needs its prefix length, such as /24.")
                    .rows(addresses)
                    .row(
                        form_field("Gateway", draft.text("gateway"))
                            .placeholder(gateway_placeholder)
                            .optional()
                            .monospace()
                            .trailing(remove_slot())
                            .validate(gateway_check),
                    ),
            );
        }
        if visible.routes {
            let routes = list_rows(
                draft.list("routes"),
                ListLook {
                    title: "Route",
                    placeholder: if version == 4 {
                        "10.0.0.0/8 192.168.1.1"
                    } else {
                        "2001:db8::/32 fe80::1"
                    },
                    check: if version == 4 { route4 } else { route6 },
                    add: "Add route",
                    monospace: true,
                },
            );
            column = column.child(
                Group::titled("Static routes")
                    .description("Network and prefix, then the next hop and an optional metric.")
                    .rows(routes),
            );
        }

        let mut behaviour = Group::new(None::<String>);
        if visible.auto_routes {
            let page = page.clone();
            behaviour = behaviour.row(
                switch_row(
                    "Use automatic routes",
                    !page.saved.yes(&format!("{prefix}.ignore-auto-routes")),
                    move |on: bool| {
                        page.apply_now(Changes::one(
                            &format!("{prefix}.ignore-auto-routes"),
                            yes_no(!on),
                        ))
                    },
                )
                .subtitle("Off ignores the routes the network hands out."),
            );
        }
        if visible.never_default {
            let page = page.clone();
            behaviour = behaviour.row(
                switch_row(
                    "Use only for resources on this network",
                    page.saved.yes(&format!("{prefix}.never-default")),
                    move |on: bool| {
                        page.apply_now(Changes::one(&format!("{prefix}.never-default"), yes_no(on)))
                    },
                )
                .subtitle("Internet traffic goes through another connection."),
            );
        }
        if visible.privacy {
            let page = page.clone();
            let selected = page.saved.get("ipv6.ip6-privacy").to_string();
            behaviour = behaviour.row(
                choice_row(
                    "Privacy extensions",
                    options(IP6_PRIVACY_CHOICES),
                    &selected,
                    move |value: String| page.apply_now(Changes::one("ipv6.ip6-privacy", value)),
                )
                .subtitle(
                    "Temporary addresses make it harder to follow this computer between networks.",
                ),
            );
        }
        if visible.auto_routes || visible.never_default || visible.privacy {
            column = column.child(behaviour);
        }
        column
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(method: &str, gateway: &str, addresses: &[&str], routes: &[&str]) -> Form {
        Form {
            method: method.to_string(),
            gateway: gateway.to_string(),
            addresses: addresses.iter().map(|text| text.to_string()).collect(),
            routes: routes.iter().map(|text| text.to_string()).collect(),
        }
    }

    fn saved(text: &str) -> ProfileData {
        ProfileData::parse(text)
    }

    #[test]
    fn manual_writes_method_addresses_gateway_and_routes_in_one_call() {
        let changes = form(
            "manual",
            "192.168.77.1",
            &["192.168.77.5/24", "10.9.0.2/16"],
            &["10.50.0.0/16 192.168.77.254 200"],
        )
        .changes(&saved("ipv4.method:auto\n"), "ipv4", 4);
        assert_eq!(
            changes.commands("P")[0],
            [
                "connection",
                "modify",
                "P",
                "ipv4.method",
                "manual",
                "ipv4.addresses",
                "192.168.77.5/24, 10.9.0.2/16",
                "ipv4.gateway",
                "192.168.77.1",
                "ipv4.routes",
                "10.50.0.0/16 192.168.77.254 200",
            ]
        );
    }

    #[test]
    fn leaving_manual_clears_addresses_and_gateway() {
        let stored = saved(
            "ipv4.method:manual\nipv4.addresses:192.168.77.5/24\nipv4.gateway:192.168.77.1\nipv4.dns:1.1.1.1\n",
        );
        let to_auto =
            form("auto", "192.168.77.1", &["192.168.77.5/24"], &[]).changes(&stored, "ipv4", 4);
        assert_eq!(
            to_auto.set,
            [
                ("ipv4.method".to_string(), "auto".to_string()),
                ("ipv4.addresses".to_string(), String::new()),
                ("ipv4.gateway".to_string(), String::new()),
            ]
        );
        // A method without DNS also drops the servers NetworkManager would refuse.
        let to_disabled = form("disabled", "", &[], &[]).changes(&stored, "ipv4", 4);
        assert!(
            to_disabled
                .set
                .contains(&("ipv4.dns".to_string(), String::new()))
        );
    }

    #[test]
    fn nothing_changed_writes_nothing() {
        let stored = saved(
            "ipv4.method:manual\nipv4.addresses:192.168.77.5/24\nipv4.gateway:192.168.77.1\n",
        );
        let same = form("manual", "192.168.77.1", &["192.168.77.5/24"], &[]);
        assert!(same.changes(&stored, "ipv4", 4).is_empty());
    }

    #[test]
    fn manual_needs_a_valid_address_and_gateway() {
        assert!(!form("manual", "", &[], &[]).valid(4));
        assert!(!form("manual", "", &["192.168.1.20"], &[]).valid(4));
        assert!(!form("manual", "nope", &["192.168.1.20/24"], &[]).valid(4));
        assert!(form("manual", "", &["192.168.1.20/24"], &[]).valid(4));
        assert!(
            form(
                "manual",
                "192.168.1.1",
                &["192.168.1.20/24"],
                &["10.0.0.0/8 192.168.1.1"]
            )
            .valid(4)
        );
        assert!(!form("manual", "", &["192.168.1.20/24"], &["10.0.0.0/8 nope"]).valid(4));
        assert!(form("auto", "", &[], &[]).valid(4));
        assert!(form("manual", "2001:db8::1", &["2001:db8::20/64"], &[]).valid(6));
        assert!(!form("manual", "", &["192.168.1.20/24"], &[]).valid(6));
    }

    #[test]
    fn ipv6_uses_its_own_keys() {
        let changes =
            form("manual", "2001:db8::1", &["2001:db8::20/64"], &[]).changes(&saved(""), "ipv6", 6);
        assert_eq!(
            changes.set[0],
            ("ipv6.method".to_string(), "manual".to_string())
        );
        assert!(changes.set.iter().all(|(key, _)| key.starts_with("ipv6.")));
    }
}
