//! The Proxy page: NetworkManager's own proxy setting for this connection, none or an automatic
//! configuration given as an address or a script.
//!
//! The method, the address and the script wait in a draft for Apply, because the fields only exist
//! for Automatic. "Only for web browsers" stands alone and applies at once.

use freya::prelude::*;

use bingux_ui::controls::form::validate;
use bingux_ui::controls::{form_field, pending_bar};

use super::draft::{Draft, Values, use_draft};
use super::store;
use super::{Page, options};
use crate::pages::connection_editor::{Changes, PROXY_CHOICES, ProfileData, diff, yes_no};
use crate::ui::{Group, choice_row, page_column, switch_row};

/// What the form holds, as typed.
#[derive(Clone, Debug, PartialEq)]
struct Form {
    method: String,
    pac_url: String,
    pac_script: String,
}

impl Form {
    fn from_saved(saved: &ProfileData) -> Self {
        Self {
            method: method_of(saved).to_string(),
            pac_url: saved.get("proxy.pac-url").to_string(),
            pac_script: saved.get("proxy.pac-script").to_string(),
        }
    }

    fn from_draft(draft: &Draft) -> Self {
        Self {
            method: draft.get("method"),
            pac_url: draft.get("pac_url"),
            pac_script: draft.get("pac_script"),
        }
    }

    fn values(self) -> Values {
        Values {
            texts: vec![
                ("method", self.method),
                ("pac_url", self.pac_url),
                ("pac_script", self.pac_script),
            ],
            lists: Vec::new(),
        }
    }

    fn automatic(&self) -> bool {
        self.method == "auto"
    }

    /// Whether Apply may write the form. The address is only checked while its field is shown.
    fn valid(&self) -> bool {
        !self.automatic() || self.pac_url.is_empty() || validate::url(&self.pac_url).is_ok()
    }
}

fn read(saved: &ProfileData) -> Values {
    Form::from_saved(saved).values()
}

/// The stored method as the page names it: `auto`, or `none` for everything else.
fn method_of(saved: &ProfileData) -> &'static str {
    if saved.get("proxy.method") == "auto" {
        "auto"
    } else {
        "none"
    }
}

/// The values to write: the method, and the address and script that differ. NetworkManager refuses
/// method None while an address or a script is stored, so switching to None clears them.
fn changes(form: &Form, saved: &ProfileData) -> Changes {
    let mut out = Changes::default();
    diff(&mut out, "proxy.method", method_of(saved), &form.method);
    let (url, script) = if form.automatic() {
        (form.pac_url.trim(), form.pac_script.as_str())
    } else {
        ("", "")
    };
    diff(&mut out, "proxy.pac-url", saved.get("proxy.pac-url"), url);
    diff(
        &mut out,
        "proxy.pac-script",
        saved.get("proxy.pac-script"),
        script,
    );
    out
}

/// What the method does, in plain words.
fn method_caption(method: &str) -> &'static str {
    match method {
        "auto" => "Applications get their proxy settings from an address or a script.",
        _ => "This connection does not set a proxy.",
    }
}

/// The Proxy page of a connection.
#[derive(PartialEq)]
pub struct ProxyBody {
    pub page: Page,
}

impl Component for ProxyBody {
    fn render(&self) -> impl IntoElement {
        let page = self.page.clone();
        let draft = use_draft(&page.saved, read);
        let busy = use_state(|| false);

        let form = Form::from_draft(&draft);
        let dirty = draft.dirty(&page.saved);
        let ok = form.valid();

        let apply = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| {
                let changes = changes(&Form::from_draft(&draft), &page.saved);
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

        let method_row = {
            let (draft, saved) = (draft.clone(), page.saved.clone());
            choice_row(
                "Proxy",
                options(PROXY_CHOICES),
                &form.method,
                move |value: String| {
                    // Text typed for Automatic must not keep the form dirty once it is hidden.
                    if value != "auto" {
                        let stored = Form::from_saved(&saved);
                        draft.set("pac_url", stored.pac_url);
                        draft.set("pac_script", stored.pac_script);
                    }
                    draft.set("method", value);
                },
            )
            .subtitle(method_caption(&form.method))
        };

        let mut column =
            page_column().maybe_child(pending_bar(dirty, ok && !*busy.read(), apply, revert));
        let mut proxy = Group::titled("Proxy").row(method_row);
        if form.automatic() {
            let page = page.clone();
            proxy = proxy.row(
                switch_row(
                    "Only for web browsers",
                    page.saved.yes("proxy.browser-only"),
                    move |on: bool| page.apply_now(Changes::one("proxy.browser-only", yes_no(on))),
                )
                .subtitle("Other applications connect directly."),
            );
        }
        column = column.child(proxy);
        if form.automatic() {
            column = column.child(
                Group::titled("Automatic configuration")
                    .description("Use an address or a script.")
                    .row(
                        form_field("Address", draft.text("pac_url"))
                            .placeholder("http://wpad.example.com/proxy.pac")
                            .hint("Where the proxy configuration file is.")
                            .optional()
                            .wide()
                            .monospace()
                            .validate(validate::url),
                    )
                    .row(
                        form_field("Script", draft.text("pac_script"))
                            .hint("The text of a proxy configuration file.")
                            .optional()
                            .wide()
                            .monospace()
                            .multiline(6),
                    ),
            );
        }
        column
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(extra: &str) -> ProfileData {
        ProfileData::parse(&format!("connection.type:802-3-ethernet\n{extra}"))
    }

    const SCRIPT: &str = "function FindProxyForURL(url, host) {\n  return \"DIRECT\";\n}";

    #[test]
    fn no_proxy_reads_as_none() {
        let form = Form::from_saved(&saved(
            "proxy.method:none\nproxy.pac-url:\nproxy.pac-script:\n",
        ));
        assert_eq!(form.method, "none");
        assert!(changes(&form, &saved("proxy.method:none\n")).is_empty());
    }

    #[test]
    fn automatic_with_an_address_writes_method_and_address() {
        let before = saved("proxy.method:none\nproxy.pac-url:\nproxy.pac-script:\n");
        let mut form = Form::from_saved(&before);
        form.method = "auto".into();
        form.pac_url = " http://wpad.example.com/proxy.pac ".into();
        assert_eq!(
            changes(&form, &before).commands("Office"),
            vec![vec![
                "connection",
                "modify",
                "Office",
                "proxy.method",
                "auto",
                "proxy.pac-url",
                "http://wpad.example.com/proxy.pac"
            ]]
        );
    }

    #[test]
    fn a_script_is_written_as_typed() {
        let before = saved("proxy.method:auto\nproxy.pac-url:\nproxy.pac-script:\n");
        let mut form = Form::from_saved(&before);
        form.pac_script = SCRIPT.into();
        assert_eq!(
            changes(&form, &before).set,
            vec![("proxy.pac-script".to_string(), SCRIPT.to_string())]
        );
    }

    #[test]
    fn a_stored_script_survives_a_read() {
        let before = saved(&format!(
            "proxy.method:auto\nproxy.pac-script:{SCRIPT}\nproxy.pac-url:\n"
        ));
        assert_eq!(Form::from_saved(&before).pac_script, SCRIPT);
    }

    #[test]
    fn switching_to_none_clears_what_networkmanager_would_refuse() {
        let before = saved(&format!(
            "proxy.method:auto\nproxy.pac-url:http://a.example/p.pac\nproxy.pac-script:{SCRIPT}\n"
        ));
        let mut form = Form::from_saved(&before);
        form.method = "none".into();
        assert_eq!(
            changes(&form, &before).commands("Office"),
            vec![vec![
                "connection",
                "modify",
                "Office",
                "proxy.method",
                "none",
                "proxy.pac-url",
                "",
                "proxy.pac-script",
                ""
            ]]
        );
    }

    #[test]
    fn the_address_is_checked_only_for_automatic() {
        let mut form = Form::from_saved(&saved(""));
        form.pac_url = "nonsense".into();
        assert!(form.valid());
        form.method = "auto".into();
        assert!(!form.valid());
        form.pac_url = String::new();
        assert!(form.valid(), "an address or a script is optional");
        form.pac_url = "http://wpad.example.com/proxy.pac".into();
        assert!(form.valid());
    }

    #[test]
    fn browser_only_is_a_yes_or_no() {
        assert_eq!(
            Changes::one("proxy.browser-only", yes_no(true)).commands("Office"),
            vec![vec![
                "connection",
                "modify",
                "Office",
                "proxy.browser-only",
                "yes"
            ]]
        );
    }
}
