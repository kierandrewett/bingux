//! The DNS page: whether the network's DNS servers are used, the servers and search domains added
//! by hand, and DNS over TLS. This is the per-connection override; NetworkManager has no other.

use freya::prelude::*;

use bingux_ui::controls::pending_bar;

use super::draft::{Draft, ListLook, Values, list_rows, use_draft};
use super::ip::method_is_bare;
use super::store;
use super::{Page, options};
use crate::pages::connection_editor::{
    Changes, DNS_OVER_TLS_CHOICES, ProfileData, diff, dns_server, dns_servers, join_list,
    search_domain, split_dns, yes_no,
};
use crate::ui::{Group, Status, choice_row, page_column, status, switch_row};

/// The IP method of a family as stored, `auto` when none is.
fn method_of(saved: &ProfileData, prefix: &str) -> String {
    let method = saved.get(&format!("{prefix}.method"));
    if method.is_empty() { "auto" } else { method }.to_string()
}

/// Every search domain, IPv4's first, without repeats.
fn search_domains(saved: &ProfileData) -> Vec<String> {
    let mut domains = saved.list("ipv4.dns-search");
    for domain in saved.list("ipv6.dns-search") {
        if !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    domains
}

fn read(saved: &ProfileData) -> Values {
    Values {
        texts: Vec::new(),
        lists: vec![
            ("servers", dns_servers(saved)),
            ("search", search_domains(saved)),
        ],
    }
}

/// The DNS page of a connection.
#[derive(PartialEq)]
pub struct DnsBody {
    pub page: Page,
}

/// Whether IPv4 and IPv6 can hold DNS settings under their methods.
fn families_allowed(saved: &ProfileData) -> (bool, bool) {
    (
        !method_is_bare(&method_of(saved, "ipv4")),
        !method_is_bare(&method_of(saved, "ipv6")),
    )
}

/// What the form holds, as plain values.
#[derive(Clone, Debug, PartialEq)]
struct Form {
    servers: Vec<String>,
    search: Vec<String>,
}

impl Form {
    fn of(draft: &Draft) -> Self {
        Self {
            servers: draft.list("servers").texts(),
            search: draft.list("search").texts(),
        }
    }

    /// Whether every typed server and domain is valid and belongs to a family that can have DNS settings.
    fn valid(&self, saved: &ProfileData) -> bool {
        let (v4, v6) = split_dns(&self.servers);
        let (v4_allowed, v6_allowed) = families_allowed(saved);
        self.servers.iter().all(|server| dns_server(server).is_ok())
            && (v4.is_empty() || v4_allowed)
            && (v6.is_empty() || v6_allowed)
            && self
                .search
                .iter()
                .all(|domain| search_domain(domain).is_ok())
            && (self.search.is_empty() || v4_allowed || v6_allowed)
    }

    /// The values to write: servers by family, and the search domains in the first family that takes them.
    fn changes(&self, saved: &ProfileData) -> Changes {
        let (v4, v6) = split_dns(&self.servers);
        let mut out = Changes::default();
        diff(
            &mut out,
            "ipv4.dns",
            &join_list(&saved.list("ipv4.dns")),
            &join_list(&v4),
        );
        diff(
            &mut out,
            "ipv6.dns",
            &join_list(&saved.list("ipv6.dns")),
            &join_list(&v6),
        );
        let (v4_allowed, _) = families_allowed(saved);
        let joined = join_list(&self.search);
        let (domains_v4, domains_v6) = if v4_allowed {
            (joined, String::new())
        } else {
            (String::new(), joined)
        };
        diff(
            &mut out,
            "ipv4.dns-search",
            &join_list(&saved.list("ipv4.dns-search")),
            &domains_v4,
        );
        diff(
            &mut out,
            "ipv6.dns-search",
            &join_list(&saved.list("ipv6.dns-search")),
            &domains_v6,
        );
        out
    }
}

impl Component for DnsBody {
    fn render(&self) -> impl IntoElement {
        let page = self.page.clone();
        let draft = use_draft(&page.saved, read);
        let busy = use_state(|| false);

        let (v4_allowed, v6_allowed) = families_allowed(&page.saved);
        if !v4_allowed && !v6_allowed {
            return page_column().child(status(Status::Empty(
                "IPv4 and IPv6 are both set so that this connection has no DNS servers. Change a method to use them."
                    .into(),
            )));
        }

        let dirty = draft.dirty(&page.saved);
        let ok = Form::of(&draft).valid(&page.saved);
        let apply = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| {
                let changes = Form::of(&draft).changes(&page.saved);
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

        let automatic =
            !(page.saved.yes("ipv4.ignore-auto-dns") || page.saved.yes("ipv6.ignore-auto-dns"));
        let automatic_row = {
            let page = page.clone();
            switch_row("Automatic DNS", automatic, move |on: bool| {
                let mut changes = Changes::one("ipv4.ignore-auto-dns", yes_no(!on));
                changes.set("ipv6.ignore-auto-dns", yes_no(!on));
                page.apply_now(changes);
            })
        };
        let tls_row = {
            let page = page.clone();
            let selected = page.saved.get("connection.dns-over-tls").to_string();
            choice_row(
                "DNS over TLS",
                options(DNS_OVER_TLS_CHOICES),
                &selected,
                move |value: String| page.apply_now(Changes::one("connection.dns-over-tls", value)),
            )
        };

        let servers = list_rows(
            draft.list("servers"),
            ListLook {
                title: "Server",
                placeholder: "1.1.1.1",
                check: dns_server,
                add: "Add server",
                monospace: true,
            },
        );
        let search = list_rows(
            draft.list("search"),
            ListLook {
                title: "Domain",
                placeholder: "example.com",
                check: search_domain,
                add: "Add search domain",
                monospace: false,
            },
        );
        page_column()
            .child(pending_bar(dirty, ok && !*busy.read(), apply, revert))
            .child(Group::titled("DNS").row(automatic_row).row(tls_row))
            .child(
                Group::titled("Servers")
                    .rows(servers),
            )
            .child(
                Group::titled("Search domains")
                    .rows(search),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(servers: &[&str], search: &[&str]) -> Form {
        Form {
            servers: servers.iter().map(|text| text.to_string()).collect(),
            search: search.iter().map(|text| text.to_string()).collect(),
        }
    }

    #[test]
    fn servers_are_written_by_family() {
        let saved = ProfileData::parse("ipv4.method:auto\nipv6.method:auto\n");
        let changes = form(
            &["1.1.1.1", "9.9.9.9", "2001:4860:4860::8888"],
            &["lan.example"],
        )
        .changes(&saved);
        assert_eq!(
            changes.commands("P")[0],
            [
                "connection",
                "modify",
                "P",
                "ipv4.dns",
                "1.1.1.1, 9.9.9.9",
                "ipv6.dns",
                "2001:4860:4860::8888",
                "ipv4.dns-search",
                "lan.example",
            ]
        );
    }

    #[test]
    fn removing_the_last_server_clears_the_key() {
        let saved = ProfileData::parse("ipv4.dns:1.1.1.1\nipv4.dns-search:lan.example\n");
        let changes = form(&[], &[]).changes(&saved);
        assert_eq!(
            changes.set,
            [
                ("ipv4.dns".to_string(), String::new()),
                ("ipv4.dns-search".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn search_domains_move_to_ipv6_when_ipv4_cannot_hold_them() {
        let saved = ProfileData::parse("ipv4.method:disabled\nipv6.method:auto\n");
        let changes = form(&[], &["lan.example"]).changes(&saved);
        assert_eq!(
            changes.set,
            [("ipv6.dns-search".to_string(), "lan.example".to_string())]
        );
    }

    #[test]
    fn servers_need_a_family_that_takes_them() {
        let saved = ProfileData::parse("ipv4.method:link-local\nipv6.method:auto\n");
        assert!(!form(&["1.1.1.1"], &[]).valid(&saved));
        assert!(form(&["2001:4860:4860::8888"], &[]).valid(&saved));
        assert!(!form(&["dns.example"], &[]).valid(&saved));
        assert!(!form(&[], &["bad domain"]).valid(&saved));
    }
}
