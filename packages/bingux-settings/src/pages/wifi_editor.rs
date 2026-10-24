//! The connection editor popup: one saved profile, edited on five tabs. Apply sends every changed
//! value in one `nmcli connection modify` call. The editor's state lives in the page that opens it,
//! so the popup body and tabs read it and redraw as values change.

use std::time::Duration;

use freya::prelude::*;

use super::connection_editor::{
    Edit, FIELDS, IPV4_METHODS, IPV6_METHODS, METERED_CHOICES, Profile, SECURITY_CHOICES,
    modify_args, parse_profile, security_args, unescape,
};
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, Overlays, PopupAction, PopupActions, ResponseKind, Status, Window, caption, choice_row,
    dialog, popup, popup_column, row, skeleton, status, switch_row, text_field, use_overlays,
    use_popup_actions, use_window,
};
use bingux_ui::controls::tabs;
use bingux_ui::group::form_row;

const QUICK: Duration = Duration::from_secs(5);
const APPLY: Duration = Duration::from_secs(30);

/// What the popup is about: the profile's name, whether it is Wi-Fi, and the facts
/// the caller already knows, such as signal and band.
#[derive(Clone, PartialEq)]
pub struct Target {
    pub name: String,
    pub wireless: bool,
    pub facts: Vec<(String, String)>,
}

/// A profile read from NetworkManager, with the facts about its live device.
#[derive(Clone, PartialEq)]
struct Loaded {
    profile: Profile,
    live: Vec<(String, String)>,
}

/// The editor's state. Every field is a `State` handle, so the popup body and its
/// event handlers can read and write the same values.
#[derive(Clone, Copy, PartialEq)]
pub struct Editor {
    overlays: Overlays,
    window: Window,
    target: State<Option<Target>>,
    loaded: State<Option<Loaded>>,
    error: State<Option<String>>,
    busy: State<bool>,
    tab: State<String>,
    autoconnect: State<bool>,
    everyone: State<bool>,
    metered: State<String>,
    key_mgmt: State<String>,
    ipv4_method: State<String>,
    ipv6_method: State<String>,
    id: State<String>,
    cloned_mac: State<String>,
    mtu: State<String>,
    psk: State<String>,
    ipv4_addresses: State<String>,
    ipv4_gateway: State<String>,
    ipv4_dns: State<String>,
    ipv6_addresses: State<String>,
    ipv6_gateway: State<String>,
    ipv6_dns: State<String>,
}

/// Create the editor state for a page. Call once per page, during render.
pub fn use_editor() -> Editor {
    Editor {
        overlays: use_overlays(),
        window: use_window(),
        target: use_state(|| None::<Target>),
        loaded: use_state(|| None::<Loaded>),
        error: use_state(|| None::<String>),
        busy: use_state(|| false),
        tab: use_state(|| "details".to_string()),
        autoconnect: use_state(|| false),
        everyone: use_state(|| true),
        metered: use_state(|| "unknown".to_string()),
        key_mgmt: use_state(|| "none".to_string()),
        ipv4_method: use_state(String::new),
        ipv6_method: use_state(String::new),
        id: use_state(String::new),
        cloned_mac: use_state(String::new),
        mtu: use_state(String::new),
        psk: use_state(String::new),
        ipv4_addresses: use_state(String::new),
        ipv4_gateway: use_state(String::new),
        ipv4_dns: use_state(String::new),
        ipv6_addresses: use_state(String::new),
        ipv6_gateway: use_state(String::new),
        ipv6_dns: use_state(String::new),
    }
}

/// The popup's tabs. Wired connections have no Security tab.
fn tab_options(wireless: bool) -> Vec<(String, String)> {
    let mut list = options(&[
        ("details", "Details"),
        ("identity", "Identity"),
        ("ipv4", "IPv4"),
        ("ipv6", "IPv6"),
    ]);
    if wireless {
        list.push(("security".to_string(), "Security".to_string()));
    }
    list
}

/// The stored tab when it is still offered, otherwise Details.
fn selected_tab(stored: &str, offered: &[(String, String)]) -> String {
    if offered.iter().any(|(key, _)| key == stored) {
        stored.to_string()
    } else {
        "details".to_string()
    }
}

/// Set one state; the popup body reads it and redraws.
fn put<T: 'static>(mut state: State<T>, value: T) {
    state.set(value);
}

fn options(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(key, label)| (key.to_string(), label.to_string()))
        .collect()
}

/// A saved profile standing in for NetworkManager. Snapshot builds only:
/// `BINGUX_SETTINGS_FAKE_WIFI_PROFILE` makes every profile a WPA2 profile with these values.
#[cfg(feature = "snapshot")]
fn fake_loaded(name: &str) -> Option<Loaded> {
    std::env::var_os("BINGUX_SETTINGS_FAKE_WIFI_PROFILE")?;
    Some(Loaded {
        profile: Profile {
            id: name.to_string(),
            autoconnect: true,
            everyone: true,
            metered: "unknown".to_string(),
            key_mgmt: "wpa-psk".to_string(),
            psk: "correct horse battery".to_string(),
            ipv4_method: "auto".to_string(),
            ipv6_method: "auto".to_string(),
            ..Profile::default()
        },
        live: vec![
            (
                "Hardware address".to_string(),
                "3C:52:82:11:9A:F0".to_string(),
            ),
            ("IP address".to_string(), "192.168.1.42/24".to_string()),
        ],
    })
}

#[cfg(not(feature = "snapshot"))]
fn fake_loaded(_name: &str) -> Option<Loaded> {
    None
}

/// Read one profile and the live facts of its device. Values come back as NetworkManager stores them.
fn load_profile(name: &str) -> Result<Loaded, CommandError> {
    if let Some(loaded) = fake_loaded(name) {
        return Ok(loaded);
    }
    let fields = FIELDS.join(",");
    let output = system::run(
        "nmcli",
        &[
            "-s",
            "-t",
            "-f",
            fields.as_str(),
            "connection",
            "show",
            "id",
            name,
        ],
        QUICK,
    )?;
    let mut profile = parse_profile(&output);
    // A profile without security has no key type. The editor shows it as "None".
    if profile.key_mgmt.is_empty() {
        profile.key_mgmt = "none".to_string();
    }
    Ok(Loaded {
        profile,
        live: live_facts(name),
    })
}

/// The hardware address and addresses of the device that runs this profile. Empty when it is not active.
fn live_facts(name: &str) -> Vec<(String, String)> {
    let Ok(devices) = system::run("nmcli", &["-t", "-f", "DEVICE,CONNECTION", "device"], QUICK)
    else {
        return Vec::new();
    };
    let device = devices
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(_, connection)| unescape(connection) == name)
        .map(|(device, _)| unescape(device));
    let Some(device) = device else {
        return Vec::new();
    };
    let Ok(text) = system::run(
        "nmcli",
        &[
            "-t",
            "-f",
            "GENERAL.HWADDR,IP4.ADDRESS,IP6.ADDRESS",
            "device",
            "show",
            device.as_str(),
        ],
        QUICK,
    ) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let value = unescape(value);
            let label = if key.starts_with("GENERAL.HWADDR") {
                "Hardware address"
            } else if key.starts_with("IP4.ADDRESS") {
                "IPv4 address"
            } else if key.starts_with("IP6.ADDRESS") {
                "IPv6 address"
            } else {
                return None;
            };
            (!value.is_empty()).then(|| (label.to_string(), value))
        })
        .collect()
}

/// The `nmcli connection modify` arguments for every change, in one call. `None` when nothing changed.
/// The security change is merged into the same call, so Apply runs one command.
fn modify_all(original: &Profile, edit: &Edit) -> Option<Vec<String>> {
    let mut args = modify_args(original, edit).unwrap_or_default();
    if let Some(security) = security_args(original, edit) {
        if args.is_empty() {
            args = security;
        } else {
            // Skip the shared `connection modify NAME` prefix.
            args.extend(security.into_iter().skip(3));
        }
    }
    (!args.is_empty()).then_some(args)
}

/// Whether a change takes effect only on the next connection: the name, addresses, DNS or security.
fn needs_reconnect(args: &[String]) -> bool {
    args.iter().skip(3).step_by(2).any(|key| {
        key == "connection.id"
            || key == "remove"
            || key.starts_with("ipv4.")
            || key.starts_with("ipv6.")
            || key.starts_with("802-11-wireless-security")
    })
}

fn run_nmcli(args: &[String], timeout: Duration) -> Result<(), CommandError> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    system::run("nmcli", &refs, timeout).map(|_| ())
}

impl Editor {
    /// Open the editor for `target`. The popup shows a loading line until the profile arrives.
    pub fn open(self, target: Target) {
        let name = target.name.clone();
        let wireless = target.wireless;
        put(self.target, Some(target));
        put(self.loaded, None);
        put(self.error, None);
        put(self.busy, false);
        put(self.tab, "details".to_string());
        let body = EditorBody {
            editor: self,
            wireless,
        };
        let strip = EditorTabs {
            editor: self,
            wireless,
        };
        self.window.open(popup(name.clone(), body).tabs(strip));
        background_then(
            move || load_profile(&name),
            move |result| match result {
                Ok(loaded) => self.fill(loaded),
                Err(error) => put(self.error, Some(error.0)),
            },
        );
    }

    fn fill(self, loaded: Loaded) {
        let profile = loaded.profile.clone();
        put(self.autoconnect, profile.autoconnect);
        put(self.everyone, profile.everyone);
        put(self.metered, profile.metered.clone());
        put(self.key_mgmt, profile.key_mgmt.clone());
        put(self.ipv4_method, profile.ipv4_method.clone());
        put(self.ipv6_method, profile.ipv6_method.clone());
        put(self.id, profile.id.clone());
        put(self.cloned_mac, profile.cloned_mac.clone());
        put(self.mtu, profile.mtu.clone());
        put(self.psk, profile.psk.clone());
        put(self.ipv4_addresses, profile.ipv4_addresses.clone());
        put(self.ipv4_gateway, profile.ipv4_gateway.clone());
        put(self.ipv4_dns, profile.ipv4_dns.clone());
        put(self.ipv6_addresses, profile.ipv6_addresses.clone());
        put(self.ipv6_gateway, profile.ipv6_gateway.clone());
        put(self.ipv6_dns, profile.ipv6_dns.clone());
        put(self.loaded, Some(loaded));
    }

    /// The values as the form shows them now.
    fn edit(self) -> Edit {
        Edit {
            id: self.id.read().clone(),
            autoconnect: *self.autoconnect.read(),
            everyone: *self.everyone.read(),
            metered: self.metered.read().clone(),
            cloned_mac: self.cloned_mac.read().clone(),
            mtu: self.mtu.read().clone(),
            key_mgmt: self.key_mgmt.read().clone(),
            psk: self.psk.read().clone(),
            ipv4_method: self.ipv4_method.read().clone(),
            ipv4_addresses: self.ipv4_addresses.read().clone(),
            ipv4_gateway: self.ipv4_gateway.read().clone(),
            ipv4_dns: self.ipv4_dns.read().clone(),
            ipv6_method: self.ipv6_method.read().clone(),
            ipv6_addresses: self.ipv6_addresses.read().clone(),
            ipv6_gateway: self.ipv6_gateway.read().clone(),
            ipv6_dns: self.ipv6_dns.read().clone(),
        }
    }

    /// The `nmcli` arguments for the pending changes, or `None` when there is nothing to apply.
    fn pending(self) -> Option<Vec<String>> {
        let loaded = self.loaded.read().clone()?;
        modify_all(&loaded.profile, &self.edit())
    }

    fn apply(self, window: Window) {
        let Some(args) = self.pending() else {
            window.close();
            return;
        };
        put(self.busy, true);
        put(self.error, None);
        background_then(
            move || run_nmcli(&args, APPLY),
            move |result| {
                put(self.busy, false);
                match result {
                    Ok(()) => window.close(),
                    Err(error) => {
                        self.overlays.toast(error.0);
                    }
                }
            },
        );
    }

    /// Ask first: forgetting drops the saved password.
    fn confirm_forget(self, window: Window) {
        let Some(target) = self.target.read().clone() else {
            return;
        };
        let name = target.name.clone();
        self.overlays.show_dialog(
            dialog(
                format!("Forget \"{}\"?", target.name),
                "The saved password is removed. Connecting again asks for it.",
            )
            .cancel("Cancel")
            .response("Forget", ResponseKind::Destructive, move |_| {
                self.forget(name.clone(), window)
            }),
        );
    }

    fn forget(self, name: String, window: Window) {
        put(self.busy, true);
        background_then(
            move || {
                run_nmcli(
                    &["connection".into(), "delete".into(), "id".into(), name],
                    QUICK,
                )
            },
            move |result| {
                put(self.busy, false);
                match result {
                    Ok(()) => window.close(),
                    Err(error) => {
                        self.overlays.toast(error.0);
                    }
                }
            },
        );
    }
}

/// The popup body. It reads the editor state, so it redraws as values change.
#[derive(Clone, PartialEq)]
struct EditorBody {
    editor: Editor,
    wireless: bool,
}

impl Component for EditorBody {
    fn render(&self) -> impl IntoElement {
        let editor = self.editor;
        let wireless = self.wireless;
        let window = use_window();

        let busy = *editor.busy.read();
        let ready = !busy
            && editor.loaded.read().is_some()
            && editor.edit().problems().is_empty()
            && editor.pending().is_some();
        let label = if busy { "Applying" } else { "Apply" };
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: label.into(),
                enabled: ready,
                on_press: EventHandler::new(move |_: ()| editor.apply(window)),
            }),
        });

        let mut column = popup_column();
        let Some(loaded) = editor.loaded.read().clone() else {
            match editor.error.read().clone() {
                Some(text) => column = column.child(status(Status::Error(text))),
                None => column = column.child(skeleton()),
            }
            return column;
        };

        let tab = selected_tab(&editor.tab.read(), &tab_options(wireless));
        let content = match tab.as_str() {
            "identity" => identity_tab(editor, &loaded, wireless),
            "ipv4" => ip_tab(editor, 4),
            "ipv6" => ip_tab(editor, 6),
            "security" => security_tab(editor),
            _ => details_tab(editor, &loaded, window),
        };
        column = column.child(content);

        let problems = editor.edit().problems();
        for problem in problems {
            column = column.child(caption(problem));
        }
        if let Some(args) = editor.pending()
            && needs_reconnect(&args)
        {
            column = column.child(caption(
                "The network reconnects to apply the name, address, DNS or security changes.",
            ));
        }
        column
    }
}

/// The popup's tab strip. It reads the live tab, so the selection follows the state.
#[derive(Clone, PartialEq)]
struct EditorTabs {
    editor: Editor,
    wireless: bool,
}

impl Component for EditorTabs {
    fn render(&self) -> impl IntoElement {
        let editor = self.editor;
        let options = tab_options(self.wireless);
        let selected = selected_tab(&editor.tab.read(), &options);
        Element::from(tabs(options, &selected, move |key: String| {
            put(editor.tab, key)
        }))
    }
}

fn details_tab(editor: Editor, loaded: &Loaded, window: Window) -> Element {
    let Some(target) = editor.target.read().clone() else {
        return Element::from(rect());
    };
    let mut facts = Group::titled("Network");
    for (title, value) in target.facts.iter().chain(loaded.live.iter()) {
        facts = facts.row(row(title.clone()).value(value.clone()));
    }
    let metered = options(METERED_CHOICES);
    let settings = Group::new(None::<String>)
        .row(switch_row(
            "Connect automatically",
            *editor.autoconnect.read(),
            move |value: bool| put(editor.autoconnect, value),
        ))
        .row(switch_row(
            "Available to other users",
            *editor.everyone.read(),
            move |value: bool| put(editor.everyone, value),
        ))
        .row(choice_row(
            "Metered connection",
            metered,
            &editor.metered.read(),
            move |value: String| put(editor.metered, value),
        ));
    let forget = Group::new(None::<String>).row(
        row("Forget network")
            .destructive()
            .on_press(move |_| editor.confirm_forget(window)),
    );
    Element::from(popup_column().child(facts).child(settings).child(forget))
}

fn identity_tab(editor: Editor, loaded: &Loaded, wireless: bool) -> Element {
    let mut group = Group::titled("Identity").row(field_row("Name", editor.id));
    if wireless {
        if !loaded.profile.bssid.is_empty() {
            group = group.row(row("BSSID").value(loaded.profile.bssid.clone()));
        }
        group = group
            .row(field_row("Cloned MAC address", editor.cloned_mac))
            .row(field_row("MTU", editor.mtu));
    }
    Element::from(group)
}

/// One label on the left and an editable field on the right.
fn field_row(title: &str, value: State<String>) -> Element {
    form_row(title, text_field(value, |_: String| {}))
}

/// The IPv4 or IPv6 tab: the method, then the addresses, gateway and DNS servers for that method.
fn ip_tab(editor: Editor, version: u8) -> Element {
    let (method, methods, addresses, gateway, dns, title) = if version == 4 {
        (
            editor.ipv4_method,
            IPV4_METHODS,
            editor.ipv4_addresses,
            editor.ipv4_gateway,
            editor.ipv4_dns,
            "IPv4",
        )
    } else {
        (
            editor.ipv6_method,
            IPV6_METHODS,
            editor.ipv6_addresses,
            editor.ipv6_gateway,
            editor.ipv6_dns,
            "IPv6",
        )
    };
    let selected = method.read().clone();
    let group = Group::titled(title)
        .row(choice_row(
            "Method",
            options(methods),
            &selected,
            move |value: String| put(method, value),
        ))
        .row(field_row("Addresses", addresses))
        .row(field_row("Gateway", gateway))
        .row(field_row("DNS servers", dns));
    Element::from(group)
}

/// The Security tab: the key type, the password and a reveal control.
fn security_tab(editor: Editor) -> Element {
    let key = editor.key_mgmt.read().clone();
    let mut group = Group::titled("Security").row(choice_row(
        "Security",
        options(SECURITY_CHOICES),
        &key,
        move |value: String| put(editor.key_mgmt, value),
    ));
    if key != "none" {
        let field = text_field(editor.psk, |_: String| {})
            .placeholder("Password")
            .password();
        group = group.row(form_row("Password", field));
    }
    Element::from(group)
}
