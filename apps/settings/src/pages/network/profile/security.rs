//! The Security page. A Wi-Fi network picks its key type here and types the password or key; a
//! wired profile turns on 802.1X, the sign-in some company and campus networks ask for before a
//! device may connect. A Wi-Fi network with enterprise security uses the same 802.1X form.
//!
//! Everything on the page waits for Apply: a password or credential is only useful complete, and a
//! half-typed one must never reach NetworkManager. The pure parts (what to write, whether the form
//! is complete) are plain functions below the page.

use freya::prelude::*;

use bingux_ui::controls::form::validate::{self, Check};
use bingux_ui::controls::{form_field, pending_bar};

use super::draft::{Draft, Values, use_draft};
use super::store;
use super::{Page, options};
use crate::pages::connection_editor::{
    Changes, EAP_CHOICES, Kind, ProfileData, WIFI_SECURITY_CHOICES, diff, eap_uses_password,
    file_path, phase2_choices, wep_key, wifi_security_kind,
};
use crate::ui::files::Filter;
use crate::ui::{
    ChooserRequest, Group, Status, Window, choice_row, files, page_column, row_button, status,
    switch_row,
};

const SECURITY: &str = "802-11-wireless-security";
const KEY_MGMT: &str = "802-11-wireless-security.key-mgmt";
const PSK: &str = "802-11-wireless-security.psk";
const WEP_KEY: &str = "802-11-wireless-security.wep-key0";
const WEP_TYPE: &str = "802-11-wireless-security.wep-key-type";
const DOT1X: &str = "802-1x";

/// How a WEP key is typed: the key itself, or a passphrase NetworkManager turns into one.
const WEP_TYPE_CHOICES: &[(&str, &str)] = &[("1", "Key"), ("2", "Passphrase")];

/// Every value the page edits, as typed. A wired page uses only the 802.1X ones.
#[derive(Clone, Debug, Default, PartialEq)]
struct Form {
    /// The Wi-Fi key type, one of `WIFI_SECURITY_CHOICES`.
    kind: String,
    psk: String,
    wep_key: String,
    /// `1` for a key, `2` for a passphrase: the numbers `nmcli` takes.
    wep_type: String,
    /// The EAP method. Empty means 802.1X is off.
    eap: String,
    identity: String,
    anonymous: String,
    password: String,
    phase2: String,
    /// `yes` or `no`.
    system_ca: String,
    ca: String,
    client_cert: String,
    private_key: String,
    key_password: String,
}

impl Form {
    /// What the page starts with, from the saved profile.
    fn of(saved: &ProfileData) -> Self {
        let kind = wifi_security_kind(saved);
        let mut eap = stored_eap(saved);
        // A Wi-Fi profile that says enterprise but names no method is incomplete: show the method
        // Apply would write, so the form can be completed.
        if eap.is_empty() && kind == "wpa-eap" {
            eap = "peap".to_string();
        }
        let phase2 = phase2_for(&eap, saved.get("802-1x.phase2-auth"));
        let own = |key: &str| saved.get(key).to_string();
        Self {
            kind: kind.to_string(),
            psk: own(PSK),
            wep_key: own(WEP_KEY),
            wep_type: wep_type_of(saved.get(WEP_TYPE)).to_string(),
            eap,
            identity: own("802-1x.identity"),
            anonymous: own("802-1x.anonymous-identity"),
            password: own("802-1x.password"),
            phase2,
            system_ca: if saved.yes("802-1x.system-ca-certs") {
                "yes"
            } else {
                "no"
            }
            .to_string(),
            ca: own("802-1x.ca-cert"),
            client_cert: own("802-1x.client-cert"),
            private_key: own("802-1x.private-key"),
            key_password: own("802-1x.private-key-password"),
        }
    }

    fn values(&self) -> Values {
        let text = |name: &'static str, value: &String| (name, value.clone());
        Values {
            texts: vec![
                text("kind", &self.kind),
                text("psk", &self.psk),
                text("wep_key", &self.wep_key),
                text("wep_type", &self.wep_type),
                text("eap", &self.eap),
                text("identity", &self.identity),
                text("anonymous", &self.anonymous),
                text("password", &self.password),
                text("phase2", &self.phase2),
                text("system_ca", &self.system_ca),
                text("ca", &self.ca),
                text("client_cert", &self.client_cert),
                text("private_key", &self.private_key),
                text("key_password", &self.key_password),
            ],
            lists: Vec::new(),
        }
    }

    fn from_draft(draft: &Draft) -> Self {
        Self {
            kind: draft.get("kind"),
            psk: draft.get("psk"),
            wep_key: draft.get("wep_key"),
            wep_type: draft.get("wep_type"),
            eap: draft.get("eap"),
            identity: draft.get("identity"),
            anonymous: draft.get("anonymous"),
            password: draft.get("password"),
            phase2: draft.get("phase2"),
            system_ca: draft.get("system_ca"),
            ca: draft.get("ca"),
            client_cert: draft.get("client_cert"),
            private_key: draft.get("private_key"),
            key_password: draft.get("key_password"),
        }
    }
}

fn read(saved: &ProfileData) -> Values {
    Form::of(saved).values()
}

/// The EAP method a profile stores. NetworkManager keeps a list; the first one is the one tried.
fn stored_eap(saved: &ProfileData) -> String {
    saved
        .list("802-1x.eap")
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// `1` or `2` from what `nmcli` prints for the key type (`key`, `passphrase`, or `unknown` when unset).
fn wep_type_of(printed: &str) -> &'static str {
    if printed.contains("passphrase") || printed.trim() == "2" {
        "2"
    } else {
        "1"
    }
}

/// The inner authentication to show for `eap` when `current` is what the form has: `current` when the
/// method offers it, else the method's first, and none for TLS, which has no inner method.
fn phase2_for(eap: &str, current: &str) -> String {
    let choices = phase2_choices(eap);
    match choices.iter().find(|(key, _)| *key == current) {
        Some(_) => current.to_string(),
        None => choices
            .first()
            .map(|(key, _)| key.to_string())
            .unwrap_or_default(),
    }
}

/// A path as `nmcli` prints it, without the `file://` a person may paste.
fn bare(path: &str) -> &str {
    path.strip_prefix("file://").unwrap_or(path)
}

/// A WEP passphrase: printable characters, at most 64.
fn wep_phrase(text: &str) -> Check {
    let printable = text.bytes().all(|byte| (0x20..=0x7e).contains(&byte));
    if (1..=64).contains(&text.len()) && printable {
        Ok(())
    } else {
        Err("A WEP passphrase is 1 to 64 letters, digits or symbols.".to_string())
    }
}

/// Whether the 802.1X form has what its method needs and nothing it shows is invalid.
fn dot1x_valid(form: &Form) -> bool {
    if form.eap.is_empty() || form.identity.trim().is_empty() {
        return false;
    }
    let methods_ok = if eap_uses_password(&form.eap) {
        !form.password.is_empty()
    } else {
        file_path(&form.client_cert).is_ok() && file_path(&form.private_key).is_ok()
    };
    methods_ok && (form.ca.is_empty() || file_path(&form.ca).is_ok())
}

/// Whether Apply may write the form.
fn valid(form: &Form, kind: Kind) -> bool {
    match kind {
        Kind::Wired => form.eap.is_empty() || dot1x_valid(form),
        Kind::Vpn => true,
        Kind::Wifi => match form.kind.as_str() {
            "wpa-psk" | "sae" => validate::wpa_passphrase(&form.psk).is_ok(),
            "wep" => wep_check(&form.wep_type)(&form.wep_key).is_ok(),
            "wpa-eap" => dot1x_valid(form),
            _ => true,
        },
    }
}

/// The check for a WEP key of the chosen type.
fn wep_check(wep_type: &str) -> fn(&str) -> Check {
    if wep_type == "2" { wep_phrase } else { wep_key }
}

/// The 802.1X values to write for `form`: only those that differ from the saved profile, and only the
/// ones its method uses. Order matters to `nmcli`: a private key is checked against the password that
/// is already set, so the password goes first.
fn dot1x_changes(changes: &mut Changes, form: &Form, saved: &ProfileData) {
    let tls = !eap_uses_password(&form.eap);
    let stored = |key: &str| saved.get(&format!("802-1x.{key}")).to_string();
    diff(changes, "802-1x.eap", &stored_eap(saved), &form.eap);
    diff(
        changes,
        "802-1x.identity",
        &stored("identity"),
        &form.identity,
    );
    diff(
        changes,
        "802-1x.anonymous-identity",
        &stored("anonymous-identity"),
        &form.anonymous,
    );
    if tls {
        // TLS has no inner method and signs in with its certificate, not a password. A leftover
        // inner method is cleared so the profile does not mix the two.
        diff(changes, "802-1x.phase2-auth", &stored("phase2-auth"), "");
    } else {
        diff(
            changes,
            "802-1x.password",
            &stored("password"),
            &form.password,
        );
        diff(
            changes,
            "802-1x.phase2-auth",
            &stored("phase2-auth"),
            &phase2_for(&form.eap, &form.phase2),
        );
    }
    // An unset switch reads as `no`, so a profile that never had the key is not rewritten.
    let stored_ca = if saved.yes("802-1x.system-ca-certs") {
        "yes"
    } else {
        "no"
    };
    diff(
        changes,
        "802-1x.system-ca-certs",
        stored_ca,
        &form.system_ca,
    );
    diff(
        changes,
        "802-1x.ca-cert",
        bare(&stored("ca-cert")),
        bare(&form.ca),
    );
    if tls {
        diff(
            changes,
            "802-1x.client-cert",
            bare(&stored("client-cert")),
            bare(&form.client_cert),
        );
        diff(
            changes,
            "802-1x.private-key-password",
            &stored("private-key-password"),
            &form.key_password,
        );
        diff(
            changes,
            "802-1x.private-key",
            bare(&stored("private-key")),
            bare(&form.private_key),
        );
    }
    // A new FAST profile asks the server to provision its credentials, the usual way it is set up.
    if form.eap == "fast" && stored_eap(saved) != "fast" {
        changes.set("802-1x.phase1-fast-provisioning", "1");
    }
}

/// What a Wi-Fi network's Security page writes.
fn wifi_changes(form: &Form, saved: &ProfileData) -> Changes {
    let mut changes = Changes::default();
    // Changing the key type always writes the key management, even when no value differs.
    let moved = form.kind != wifi_security_kind(saved);
    match form.kind.as_str() {
        "wpa-psk" | "sae" => {
            if moved {
                changes.set(KEY_MGMT, &form.kind);
            }
            diff(&mut changes, PSK, saved.get(PSK), &form.psk);
        }
        "wep" => {
            let stored_type = wep_type_of(saved.get(WEP_TYPE));
            let retyped = moved || stored_type != form.wep_type;
            if moved {
                changes.set(KEY_MGMT, "none");
            }
            // The key type first: `nmcli` checks the key against the type that is set by then.
            if retyped {
                changes.set(WEP_TYPE, &form.wep_type);
            }
            if retyped || saved.get(WEP_KEY) != form.wep_key {
                changes.set(WEP_KEY, &form.wep_key);
            }
        }
        "wpa-eap" => {
            if moved {
                changes.set(KEY_MGMT, "wpa-eap");
            }
            dot1x_changes(&mut changes, form, saved);
        }
        _ => {
            if saved.has_setting(SECURITY) {
                changes.remove(SECURITY);
            }
        }
    }
    if form.kind != "wpa-eap" && saved.has_setting(DOT1X) {
        changes.remove(DOT1X);
    }
    changes
}

/// What a wired profile's 802.1X page writes.
fn wired_changes(form: &Form, saved: &ProfileData) -> Changes {
    let mut changes = Changes::default();
    if form.eap.is_empty() {
        if saved.has_setting(DOT1X) {
            changes.remove(DOT1X);
        }
    } else {
        dot1x_changes(&mut changes, form, saved);
    }
    changes
}

/// The `nmcli` writes Apply makes, in order; empty when nothing differs.
///
/// NetworkManager discards a password or key that is set while the profile still has an `802-1x`
/// setting, and `Changes` removes settings after it sets values. So when 802.1X is removed, the
/// secrets of the new key type are written by a second step, after the first has gone through.
fn plan(form: &Form, saved: &ProfileData, kind: Kind) -> Vec<Changes> {
    let mut changes = match kind {
        Kind::Wifi => wifi_changes(form, saved),
        Kind::Wired => wired_changes(form, saved),
        Kind::Vpn => return Vec::new(),
    };
    if changes.is_empty() {
        return Vec::new();
    }
    if !changes.remove.iter().any(|setting| setting == DOT1X) {
        return vec![changes];
    }
    let (secrets, now): (Vec<_>, Vec<_>) = std::mem::take(&mut changes.set)
        .into_iter()
        .partition(|(key, _)| key == PSK || key == WEP_KEY);
    changes.set = now;
    let mut steps = vec![changes];
    if !secrets.is_empty() {
        steps.push(Changes {
            set: secrets,
            remove: Vec::new(),
        });
    }
    steps
}

/// Run `steps` one after another; a step starts only once the one before it has been saved.
fn run_steps(page: Page, busy: State<bool>, mut steps: Vec<Changes>) {
    if steps.is_empty() {
        return;
    }
    let first = steps.remove(0);
    let (ctx, owner, state) = (page.ctx, page.owner.clone(), page.state);
    store::apply_batch(ctx, &owner, state, busy, first, move || {
        run_steps(page, busy, steps)
    });
}

/// Turn 802.1X on with PEAP, the method most networks use, and its first inner method.
fn enter_eap(draft: &Draft) {
    pick_eap(draft, "peap");
}

/// Choose an EAP method, and an inner authentication the method offers.
fn pick_eap(draft: &Draft, eap: &str) {
    let phase2 = phase2_for(eap, &draft.get("phase2"));
    draft.set("eap", eap);
    draft.set("phase2", phase2);
}

/// A button that opens the file chooser and puts the file the person picks into `target`. A
/// cancelled chooser leaves the field as it was.
fn browse_button(
    window: Window,
    title: &'static str,
    filter: Filter,
    target: State<String>,
) -> Element {
    row_button("Browse…", move |_| {
        let request = ChooserRequest::open()
            .title(title)
            .filter(filter.clone())
            .filter(Filter::new("All files"));
        let mut target = target;
        files::choose(window, request, move |chosen| {
            if let Some(path) = chosen.and_then(|chosen| chosen.paths.into_iter().next()) {
                target.set(path.to_string_lossy().into_owned());
            }
        });
    })
    .into()
}

/// The 802.1X form: the method and sign-in, then the certificates. Used by the wired page and by a
/// Wi-Fi network with enterprise security.
fn dot1x_groups(draft: &Draft, window: Window) -> Vec<Element> {
    let eap = draft.get("eap");
    let tls = !eap_uses_password(&eap);
    let certificate = || {
        Filter::new("Certificates")
            .glob("*.pem")
            .glob("*.crt")
            .glob("*.cer")
            .glob("*.der")
            .glob("*.p12")
    };
    let key = || {
        Filter::new("Private keys")
            .glob("*.pem")
            .glob("*.key")
            .glob("*.der")
            .glob("*.p12")
    };

    let method = {
        let draft = draft.clone();
        choice_row(
            "Method",
            options(EAP_CHOICES),
            &eap,
            move |value: String| pick_eap(&draft, &value),
        )
    };
    let mut sign_in = Group::titled("Authentication").row(method);
    if !tls {
        let phase2 = draft.get("phase2");
        let draft = draft.clone();
        sign_in = sign_in.row(choice_row(
            "Inner authentication",
            options(phase2_choices(&eap)),
            &phase2,
            move |value: String| draft.set("phase2", value),
        ));
    }
    sign_in = sign_in
        .row(form_field("Identity", draft.text("identity")).wide())
        .row(
            form_field("Anonymous identity", draft.text("anonymous"))
                .hint("Sent before the secure connection starts. Leave empty unless the network asks for it.")
                .optional()
                .wide(),
        );
    if !tls {
        sign_in = sign_in.row(
            form_field("Password", draft.text("password"))
                .password()
                .wide(),
        );
    }

    let system_ca = {
        let draft = draft.clone();
        switch_row(
            "Use the system's trusted certificates",
            draft.get("system_ca") == "yes",
            move |on: bool| draft.set("system_ca", if on { "yes" } else { "no" }),
        )
    };
    let mut certificates = Group::titled("Certificates").row(system_ca).row(
        form_field("CA certificate", draft.text("ca"))
            .placeholder("/etc/ssl/certs/ca.pem")
            // "Optional" is said in the hint: next to the Browse button the title column is too
            // narrow for the word after the title.
            .hint("Optional. The certificate that signed the network's own. Leave empty to skip this check.")
            .monospace()
            .validate(file_path)
            .trailing(browse_button(
                window,
                "CA certificate",
                certificate(),
                draft.text("ca"),
            ))
            .wide(),
    );
    if tls {
        certificates = certificates
            .row(
                form_field("Client certificate", draft.text("client_cert"))
                    .placeholder("/home/you/client.pem")
                    .monospace()
                    .validate(file_path)
                    .trailing(browse_button(
                        window,
                        "Client certificate",
                        certificate(),
                        draft.text("client_cert"),
                    ))
                    .wide(),
            )
            .row(
                form_field("Private key", draft.text("private_key"))
                    .placeholder("/home/you/client.key")
                    .monospace()
                    .validate(file_path)
                    .trailing(browse_button(
                        window,
                        "Private key",
                        key(),
                        draft.text("private_key"),
                    ))
                    .wide(),
            )
            .row(
                form_field("Private key password", draft.text("key_password"))
                    .password()
                    .optional()
                    .wide(),
            );
    }
    vec![sign_in.into(), certificates.into()]
}

/// The Security page of a Wi-Fi network, a wired profile or a VPN.
#[derive(PartialEq)]
pub struct SecurityBody {
    pub page: Page,
}

impl Component for SecurityBody {
    fn render(&self) -> impl IntoElement {
        let page = self.page.clone();
        let kind = page.kind();
        let draft = use_draft(&page.saved, read);
        let busy = use_state(|| false);

        if kind == Kind::Vpn {
            return page_column().child(status(Status::Empty(
                "This connection has no security settings here.".into(),
            )));
        }

        let form = Form::from_draft(&draft);
        let steps = plan(&form, &page.saved, kind);
        let dirty = !steps.is_empty();
        let ok = valid(&form, kind);
        let apply = {
            let page = page.clone();
            move |_: ()| run_steps(page.clone(), busy, plan(&form, &page.saved, kind))
        };
        let revert = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| draft.reset(&page.saved)
        };
        let column =
            page_column().child(pending_bar(dirty, ok && !*busy.read(), apply, revert));

        if kind == Kind::Wired {
            wired_page(column, &draft, page.ctx.window)
        } else {
            wifi_page(column, &draft, page.ctx.window)
        }
    }
}

/// The wired 802.1X page: the switch, and the form while it is on.
fn wired_page(column: Rect, draft: &Draft, window: Window) -> Rect {
    let on = !draft.get("eap").is_empty();
    let toggle = {
        let draft = draft.clone();
        switch_row("Use 802.1X authentication", on, move |on: bool| {
            if on {
                enter_eap(&draft);
            } else {
                draft.set("eap", "");
            }
        })
    };
    let mut column = column.child(Group::titled("802.1X").row(toggle));
    if on {
        for group in dot1x_groups(draft, window) {
            column = column.child(group);
        }
    }
    column
}

/// The Wi-Fi Security page: the key type, and what that type needs.
fn wifi_page(column: Rect, draft: &Draft, window: Window) -> Rect {
    let kind = draft.get("kind");
    let kind_row = {
        let draft = draft.clone();
        choice_row(
            "Security",
            options(WIFI_SECURITY_CHOICES),
            &kind,
            move |value: String| {
                if value == "wpa-eap" && draft.get("eap").is_empty() {
                    enter_eap(&draft);
                }
                draft.set("kind", value);
            },
        )
    };
    let mut group = Group::titled("Security");
    match kind.as_str() {
        "none" => group = group.description("Open network: traffic is not encrypted."),
        "wep" => group = group.description("WEP is easy to break."),
        _ => {}
    }
    // What the key type needs slides in under it.
    let needs: Vec<Element> = match kind.as_str() {
        "wpa-psk" | "sae" => vec![
            form_field("Password", draft.text("psk"))
                .hint("8 to 63 characters, or 64 hexadecimal digits.")
                .password()
                .validate(validate::wpa_passphrase)
                .into(),
        ],
        "wep" => {
            let wep_type = draft.get("wep_type");
            let type_row = {
                let draft = draft.clone();
                choice_row(
                    "Key type",
                    options(WEP_TYPE_CHOICES),
                    &wep_type,
                    move |value: String| draft.set("wep_type", value),
                )
            };
            vec![
                type_row.into(),
                form_field("WEP key", draft.text("wep_key"))
                    .password()
                    .validate(wep_check(&wep_type))
                    .into(),
            ]
        }
        _ => Vec::new(),
    };
    group = group.row(kind_row.nested(!needs.is_empty(), needs));
    let mut column = column.child(group);
    if kind == "wpa-eap" {
        for group in dot1x_groups(draft, window) {
            column = column.child(group);
        }
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(text: &str) -> ProfileData {
        ProfileData::parse(text)
    }

    fn wifi(text: &str) -> ProfileData {
        saved(&format!("connection.type:802-11-wireless\n{text}"))
    }

    fn wired(text: &str) -> ProfileData {
        saved(&format!("connection.type:802-3-ethernet\n{text}"))
    }

    fn form_of(data: &ProfileData) -> Form {
        Form::of(data)
    }

    /// Every command of a plan for `name`, one `nmcli` argument list per step and command.
    fn commands(steps: &[Changes], name: &str) -> Vec<Vec<String>> {
        steps.iter().flat_map(|step| step.commands(name)).collect()
    }

    fn args(command: &[&str]) -> Vec<String> {
        command.iter().map(|part| part.to_string()).collect()
    }

    #[test]
    fn nothing_changed_writes_nothing() {
        let open = wifi("");
        assert!(plan(&form_of(&open), &open, Kind::Wifi).is_empty());
        let psk = wifi(
            "802-11-wireless-security.key-mgmt:wpa-psk\n802-11-wireless-security.psk:correct horse\n",
        );
        assert!(plan(&form_of(&psk), &psk, Kind::Wifi).is_empty());
        let wep = wifi(
            "802-11-wireless-security.key-mgmt:none\n802-11-wireless-security.wep-key0:abcde\n802-11-wireless-security.wep-key-type:key\n",
        );
        assert!(plan(&form_of(&wep), &wep, Kind::Wifi).is_empty());
        let off = wired("");
        assert!(plan(&form_of(&off), &off, Kind::Wired).is_empty());
    }

    #[test]
    fn open_to_personal_writes_key_management_and_password() {
        let open = wifi("");
        let mut form = form_of(&open);
        form.kind = "wpa-psk".into();
        form.psk = "correct horse".into();
        let steps = plan(&form, &open, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-11-wireless-security.key-mgmt",
                "wpa-psk",
                "802-11-wireless-security.psk",
                "correct horse",
            ])]
        );
    }

    #[test]
    fn personal_to_wpa3_writes_only_the_key_management() {
        let psk = wifi(
            "802-11-wireless-security.key-mgmt:wpa-psk\n802-11-wireless-security.psk:correct horse\n",
        );
        let mut form = form_of(&psk);
        form.kind = "sae".into();
        let steps = plan(&form, &psk, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-11-wireless-security.key-mgmt",
                "sae"
            ])]
        );
    }

    #[test]
    fn a_new_password_alone_writes_only_the_password() {
        let psk = wifi(
            "802-11-wireless-security.key-mgmt:wpa-psk\n802-11-wireless-security.psk:correct horse\n",
        );
        let mut form = form_of(&psk);
        form.psk = "another horse".into();
        let steps = plan(&form, &psk, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-11-wireless-security.psk",
                "another horse"
            ])]
        );
    }

    #[test]
    fn wep_writes_the_key_type_before_the_key() {
        let psk = wifi(
            "802-11-wireless-security.key-mgmt:sae\n802-11-wireless-security.psk:correct horse\n",
        );
        let mut form = form_of(&psk);
        form.kind = "wep".into();
        form.wep_type = "2".into();
        form.wep_key = "my long passphrase".into();
        let steps = plan(&form, &psk, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-11-wireless-security.key-mgmt",
                "none",
                "802-11-wireless-security.wep-key-type",
                "2",
                "802-11-wireless-security.wep-key0",
                "my long passphrase",
            ])]
        );
    }

    #[test]
    fn changing_only_the_wep_key_type_rewrites_the_key_after_it() {
        let wep = wifi(
            "802-11-wireless-security.key-mgmt:none\n802-11-wireless-security.wep-key0:abcde\n802-11-wireless-security.wep-key-type:key\n",
        );
        assert_eq!(form_of(&wep).wep_type, "1");
        let mut form = form_of(&wep);
        form.wep_type = "2".into();
        let steps = plan(&form, &wep, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-11-wireless-security.wep-key-type",
                "2",
                "802-11-wireless-security.wep-key0",
                "abcde",
            ])]
        );
    }

    #[test]
    fn open_removes_the_security_and_the_802_1x_settings() {
        let eap = wifi(
            "802-11-wireless-security.key-mgmt:wpa-eap\n802-1x.eap:peap\n802-1x.identity:bob\n",
        );
        let mut form = form_of(&eap);
        form.kind = "none".into();
        let steps = plan(&form, &eap, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![
                args(&[
                    "connection",
                    "modify",
                    "n",
                    "remove",
                    "802-11-wireless-security"
                ]),
                args(&["connection", "modify", "n", "remove", "802-1x"]),
            ]
        );
    }

    #[test]
    fn leaving_enterprise_writes_the_password_after_removing_802_1x() {
        let eap = wifi(
            "802-11-wireless-security.key-mgmt:wpa-eap\n802-1x.eap:peap\n802-1x.identity:bob\n",
        );
        let mut form = form_of(&eap);
        form.kind = "wpa-psk".into();
        form.psk = "correct horse".into();
        let steps = plan(&form, &eap, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![
                args(&[
                    "connection",
                    "modify",
                    "n",
                    "802-11-wireless-security.key-mgmt",
                    "wpa-psk"
                ]),
                args(&["connection", "modify", "n", "remove", "802-1x"]),
                args(&[
                    "connection",
                    "modify",
                    "n",
                    "802-11-wireless-security.psk",
                    "correct horse"
                ]),
            ]
        );
        assert_eq!(steps.len(), 2);

        form.kind = "wep".into();
        form.wep_key = "abcde".into();
        let steps = plan(&form, &eap, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![
                args(&[
                    "connection",
                    "modify",
                    "n",
                    "802-11-wireless-security.key-mgmt",
                    "none",
                    "802-11-wireless-security.wep-key-type",
                    "1",
                ]),
                args(&["connection", "modify", "n", "remove", "802-1x"]),
                args(&[
                    "connection",
                    "modify",
                    "n",
                    "802-11-wireless-security.wep-key0",
                    "abcde"
                ]),
            ]
        );
    }

    #[test]
    fn enterprise_wifi_writes_key_management_and_the_802_1x_keys() {
        let open = wifi("");
        let mut form = form_of(&open);
        form.kind = "wpa-eap".into();
        form.eap = "peap".into();
        form.phase2 = "mschapv2".into();
        form.identity = "bob".into();
        form.password = "secret".into();
        let steps = plan(&form, &open, Kind::Wifi);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-11-wireless-security.key-mgmt",
                "wpa-eap",
                "802-1x.eap",
                "peap",
                "802-1x.identity",
                "bob",
                "802-1x.password",
                "secret",
                "802-1x.phase2-auth",
                "mschapv2",
            ])]
        );
    }

    #[test]
    fn turning_802_1x_on_writes_the_method_and_every_filled_field() {
        let off = wired("");
        let mut form = form_of(&off);
        form.eap = "peap".into();
        form.phase2 = "mschapv2".into();
        form.identity = "bob".into();
        form.anonymous = "anon".into();
        form.password = "secret".into();
        form.ca = "/etc/ssl/certs/ca-bundle.crt".into();
        let steps = plan(&form, &off, Kind::Wired);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-1x.eap",
                "peap",
                "802-1x.identity",
                "bob",
                "802-1x.anonymous-identity",
                "anon",
                "802-1x.password",
                "secret",
                "802-1x.phase2-auth",
                "mschapv2",
                "802-1x.ca-cert",
                "/etc/ssl/certs/ca-bundle.crt",
            ])]
        );
    }

    #[test]
    fn switching_to_tls_clears_the_inner_method_and_writes_the_certificates() {
        let peap = wired(
            "802-1x.eap:peap\n802-1x.identity:bob\n802-1x.password:secret\n802-1x.phase2-auth:mschapv2\n802-1x.ca-cert:/etc/ssl/certs/ca-bundle.crt\n802-1x.system-ca-certs:no\n",
        );
        let mut form = form_of(&peap);
        form.eap = "tls".into();
        form.phase2 = phase2_for("tls", &form.phase2);
        form.client_cert = "/tmp/cert.pem".into();
        form.private_key = "/tmp/key.pem".into();
        let steps = plan(&form, &peap, Kind::Wired);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-1x.eap",
                "tls",
                "802-1x.phase2-auth",
                "",
                "802-1x.client-cert",
                "/tmp/cert.pem",
                "802-1x.private-key",
                "/tmp/key.pem",
            ])]
        );
        form.key_password = "hunter22".into();
        let steps = plan(&form, &peap, Kind::Wired);
        let command = &commands(&steps, "n")[0];
        let at = |key: &str| command.iter().position(|part| part == key).unwrap();
        assert!(at("802-1x.private-key-password") < at("802-1x.private-key"));
    }

    #[test]
    fn fast_asks_for_provisioning_only_when_the_method_changes_to_it() {
        let peap = wired(
            "802-1x.eap:peap\n802-1x.identity:bob\n802-1x.password:s\n802-1x.phase2-auth:mschapv2\n",
        );
        let mut form = form_of(&peap);
        form.eap = "fast".into();
        form.phase2 = "mschapv2".into();
        let steps = plan(&form, &peap, Kind::Wired);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&[
                "connection",
                "modify",
                "n",
                "802-1x.eap",
                "fast",
                "802-1x.phase1-fast-provisioning",
                "1",
            ])]
        );
        let fast = wired(
            "802-1x.eap:fast\n802-1x.identity:bob\n802-1x.password:s\n802-1x.phase2-auth:mschapv2\n",
        );
        assert!(plan(&form_of(&fast), &fast, Kind::Wired).is_empty());
    }

    #[test]
    fn a_cleared_path_writes_an_empty_string() {
        let peap = wired(
            "802-1x.eap:peap\n802-1x.identity:bob\n802-1x.password:s\n802-1x.phase2-auth:mschapv2\n802-1x.ca-cert:/etc/ssl/certs/ca-bundle.crt\n",
        );
        let mut form = form_of(&peap);
        form.ca = String::new();
        let steps = plan(&form, &peap, Kind::Wired);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&["connection", "modify", "n", "802-1x.ca-cert", ""])]
        );
    }

    #[test]
    fn a_pasted_file_url_is_the_same_path() {
        let peap = wired(
            "802-1x.eap:peap\n802-1x.identity:bob\n802-1x.password:s\n802-1x.phase2-auth:mschapv2\n802-1x.ca-cert:/etc/ca.pem\n",
        );
        let mut form = form_of(&peap);
        form.ca = "file:///etc/ca.pem".into();
        assert!(plan(&form, &peap, Kind::Wired).is_empty());
    }

    #[test]
    fn turning_802_1x_off_removes_the_setting_only_when_it_was_on() {
        let peap = wired(
            "802-1x.eap:peap\n802-1x.identity:bob\n802-1x.password:s\n802-1x.phase2-auth:mschapv2\n",
        );
        let mut form = form_of(&peap);
        form.eap = String::new();
        let steps = plan(&form, &peap, Kind::Wired);
        assert_eq!(
            commands(&steps, "n"),
            vec![args(&["connection", "modify", "n", "remove", "802-1x"])]
        );
        let off = wired("");
        assert!(plan(&form, &off, Kind::Wired).is_empty());
    }

    #[test]
    fn the_inner_method_follows_the_eap_method() {
        assert_eq!(phase2_for("peap", ""), "mschapv2");
        assert_eq!(phase2_for("peap", "gtc"), "gtc");
        assert_eq!(phase2_for("ttls", "pap"), "pap");
        // PAP is not a PEAP method, so the first one is taken.
        assert_eq!(phase2_for("peap", "pap"), "mschapv2");
        assert_eq!(phase2_for("fast", "md5"), "mschapv2");
        assert_eq!(phase2_for("tls", "mschapv2"), "");
        assert_eq!(phase2_for("", "mschapv2"), "");
    }

    #[test]
    fn wep_type_reads_what_nmcli_prints() {
        assert_eq!(wep_type_of("key"), "1");
        assert_eq!(wep_type_of("passphrase"), "2");
        assert_eq!(wep_type_of("unknown"), "1");
        assert_eq!(wep_type_of(""), "1");
        assert_eq!(wep_type_of("2 (passphrase)"), "2");
    }

    #[test]
    fn wifi_forms_need_what_their_key_type_needs() {
        let mut form = Form {
            kind: "none".into(),
            ..Form::default()
        };
        assert!(valid(&form, Kind::Wifi));
        form.kind = "wpa-psk".into();
        assert!(!valid(&form, Kind::Wifi));
        form.psk = "short".into();
        assert!(!valid(&form, Kind::Wifi));
        form.psk = "correct horse".into();
        assert!(valid(&form, Kind::Wifi));
        form.kind = "wep".into();
        form.wep_type = "1".into();
        form.wep_key = "abc".into();
        assert!(!valid(&form, Kind::Wifi));
        form.wep_key = "abcde".into();
        assert!(valid(&form, Kind::Wifi));
        form.wep_type = "2".into();
        form.wep_key = "any phrase at all".into();
        assert!(valid(&form, Kind::Wifi));
        form.wep_key = "x".repeat(65);
        assert!(!valid(&form, Kind::Wifi));
    }

    #[test]
    fn dot1x_needs_an_identity_and_what_the_method_signs_in_with() {
        let mut form = Form {
            eap: "peap".into(),
            phase2: "mschapv2".into(),
            ..Form::default()
        };
        assert!(!valid(&form, Kind::Wired));
        form.identity = "bob".into();
        assert!(
            !valid(&form, Kind::Wired),
            "a password method needs a password"
        );
        form.password = "secret".into();
        assert!(valid(&form, Kind::Wired));
        form.ca = "relative.pem".into();
        assert!(!valid(&form, Kind::Wired), "a CA path that is not a path");
        form.ca = String::new();
        form.identity = "  ".into();
        assert!(!valid(&form, Kind::Wired));
        form.identity = "bob".into();

        form.eap = "tls".into();
        form.password = String::new();
        assert!(!valid(&form, Kind::Wired), "TLS needs both files");
        form.client_cert = "/tmp/cert.pem".into();
        assert!(!valid(&form, Kind::Wired));
        form.private_key = "/tmp/key.pem".into();
        assert!(valid(&form, Kind::Wired));
        form.private_key = "key.pem".into();
        assert!(!valid(&form, Kind::Wired));
    }

    #[test]
    fn a_wired_form_with_802_1x_off_is_always_valid() {
        let form = Form::default();
        assert!(valid(&form, Kind::Wired));
    }

    #[test]
    fn a_saved_enterprise_profile_without_a_method_shows_peap() {
        let data = wifi("802-11-wireless-security.key-mgmt:wpa-eap\n");
        let form = form_of(&data);
        assert_eq!(form.eap, "peap");
        assert_eq!(form.phase2, "mschapv2");
    }

    #[test]
    fn the_vpn_page_writes_nothing() {
        let vpn = saved("connection.type:vpn\n");
        assert!(plan(&Form::default(), &vpn, Kind::Vpn).is_empty());
    }
}
