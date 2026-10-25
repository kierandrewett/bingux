//! The connection editor popup: one saved profile, edited on up to five tabs. Apply sends every changed
//! value in one `nmcli connection modify` call. The editor's state lives in the page that opens it,
//! so the popup body and tabs read it and redraw as values change.

use std::time::Duration;

use freya::prelude::*;

use super::connection_editor::{
    CLONED_MAC_CHOICES, Edit, FIELDS, IP6_PRIVACY_CHOICES, IPV4_METHODS, IPV6_METHODS, Ip,
    MAC_RANDOMIZATION_CHOICES, METERED_CHOICES, Profile, SECURITY_CHOICES, WAKE_ON_LAN_CHOICES,
    cloned_mac_caption, mac_randomization_caption, method_caption, modify_args, parse_profile,
    security_args, shown, unescape,
};
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, Overlays, PopupAction, PopupActions, ResponseKind, Status, Window, caption, choice_row,
    dialog, form_row, popup, popup_column, row, segmented, skeleton, status, stepper, switch_row,
    text_field, use_overlays, use_popup_actions, use_window,
};

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
    id: State<String>,
    autoconnect: State<bool>,
    priority: State<i64>,
    everyone: State<bool>,
    metered: State<String>,
    cloned_mac: State<String>,
    cloned_mac_address: State<String>,
    mac_randomization: State<String>,
    wake_on_lan: State<String>,
    mtu: State<String>,
    key_mgmt: State<String>,
    psk: State<String>,
    ip6_privacy: State<i64>,
    ipv4: IpFields,
    ipv6: IpFields,
}

/// The states of one IP family: the method and the fields and switches it can show.
#[derive(Clone, Copy, PartialEq)]
struct IpFields {
    method: State<String>,
    addresses: State<String>,
    gateway: State<String>,
    dns: State<String>,
    dns_search: State<String>,
    ignore_auto_dns: State<bool>,
    ignore_auto_routes: State<bool>,
    never_default: State<bool>,
}

fn use_ip_fields() -> IpFields {
    IpFields {
        method: use_state(String::new),
        addresses: use_state(String::new),
        gateway: use_state(String::new),
        dns: use_state(String::new),
        dns_search: use_state(String::new),
        ignore_auto_dns: use_state(|| false),
        ignore_auto_routes: use_state(|| false),
        never_default: use_state(|| false),
    }
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
        id: use_state(String::new),
        autoconnect: use_state(|| false),
        priority: use_state(|| 0_i64),
        everyone: use_state(|| true),
        metered: use_state(|| "unknown".to_string()),
        cloned_mac: use_state(String::new),
        cloned_mac_address: use_state(String::new),
        mac_randomization: use_state(|| "default".to_string()),
        wake_on_lan: use_state(|| "default".to_string()),
        mtu: use_state(|| "auto".to_string()),
        key_mgmt: use_state(|| "none".to_string()),
        psk: use_state(String::new),
        ip6_privacy: use_state(|| -1_i64),
        ipv4: use_ip_fields(),
        ipv6: use_ip_fields(),
    }
}

fn options(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(key, label)| (key.to_string(), label.to_string()))
        .collect()
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

/// A saved profile standing in for NetworkManager. Snapshot builds only: setting
/// `BINGUX_SETTINGS_FAKE_WIFI_PROFILE` makes every profile a WPA2 profile with these values.
#[cfg(feature = "snapshot")]
fn fake_loaded(name: &str) -> Option<Loaded> {
    std::env::var_os("BINGUX_SETTINGS_FAKE_WIFI_PROFILE")?;
    Some(Loaded {
        profile: Profile {
            id: name.to_string(),
            wireless: true,
            autoconnect: true,
            priority: 0,
            everyone: true,
            metered: "unknown".to_string(),
            mac_randomization: "default".to_string(),
            wake_on_lan: "default".to_string(),
            mtu: "auto".to_string(),
            key_mgmt: "wpa-psk".to_string(),
            psk: "correct horse battery".to_string(),
            ip6_privacy: -1,
            ipv4: Ip {
                method: "auto".to_string(),
                ..Ip::default()
            },
            ipv6: Ip {
                method: "auto".to_string(),
                ..Ip::default()
            },
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

fn fill_ip(fields: IpFields, ip: &Ip) {
    put(fields.method, ip.method.clone());
    put(fields.addresses, ip.addresses.clone());
    put(fields.gateway, ip.gateway.clone());
    put(fields.dns, ip.dns.clone());
    put(fields.dns_search, ip.dns_search.clone());
    put(fields.ignore_auto_dns, ip.ignore_auto_dns);
    put(fields.ignore_auto_routes, ip.ignore_auto_routes);
    put(fields.never_default, ip.never_default);
}

fn read_ip(fields: IpFields) -> Ip {
    Ip {
        method: fields.method.read().clone(),
        addresses: fields.addresses.read().clone(),
        gateway: fields.gateway.read().clone(),
        dns: fields.dns.read().clone(),
        dns_search: fields.dns_search.read().clone(),
        ignore_auto_dns: *fields.ignore_auto_dns.read(),
        ignore_auto_routes: *fields.ignore_auto_routes.read(),
        never_default: *fields.never_default.read(),
    }
}

impl Editor {
    /// Open the editor for `target`. The popup shows a loading line until the profile arrives.
    pub fn open(self, target: Target) {
        self.open_popup(target);
    }

    /// Open the popup for `target` and start loading its profile. Returns the popup's window.
    fn open_popup(self, target: Target) -> Window {
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
        let window = self.window.open(popup(name.clone(), body).tabs(strip));
        background_then(
            move || load_profile(&name),
            move |result| match result {
                Ok(loaded) => self.fill(loaded),
                Err(error) => put(self.error, Some(error.0)),
            },
        );
        window
    }

    /// Snapshot only: after the page has loaded, open `target`, set the fields named by
    /// `BINGUX_SETTINGS_SNAPSHOT_SET` (`key=value,...`), select the tab named by
    /// `BINGUX_SETTINGS_SNAPSHOT_TAB`, and press Apply when `BINGUX_SETTINGS_SNAPSHOT_APPLY`
    /// is set. Lets a real `nmcli connection modify` run without a pointer.
    #[cfg(feature = "snapshot")]
    pub fn snapshot_open(self, target: Target) {
        let set = std::env::var("BINGUX_SETTINGS_SNAPSHOT_SET").ok();
        let tab = std::env::var("BINGUX_SETTINGS_SNAPSHOT_TAB").ok();
        let apply = std::env::var_os("BINGUX_SETTINGS_SNAPSHOT_APPLY").is_some();
        spawn(async move {
            timer(Duration::from_millis(2500)).await;
            let window = self.open_popup(target);
            timer(Duration::from_millis(2500)).await;
            if let Some(spec) = set {
                self.snapshot_set(&spec);
            }
            if let Some(tab) = tab {
                put(self.tab, tab);
            }
            if apply {
                timer(Duration::from_millis(500)).await;
                self.apply(window);
            }
        });
    }

    /// Snapshot only: set editor fields from `key=value` pairs. Unknown keys are reported.
    #[cfg(feature = "snapshot")]
    fn snapshot_set(self, spec: &str) {
        for pair in spec.split(',') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            let flag = value == "1";
            match key {
                "mtu" => put(self.mtu, value.to_string()),
                "priority" => put(self.priority, value.parse().unwrap_or(0)),
                "ip6-privacy" => put(self.ip6_privacy, value.parse().unwrap_or(-1)),
                "wake-on-lan" => put(self.wake_on_lan, value.to_string()),
                "cloned-mac" => put(self.cloned_mac, value.to_string()),
                "mac-randomization" => put(self.mac_randomization, value.to_string()),
                "ipv4" => put(self.ipv4.method, value.to_string()),
                "ipv6" => put(self.ipv6.method, value.to_string()),
                "ipv4.never-default" => put(self.ipv4.never_default, flag),
                "ipv4.ignore-auto-dns" => put(self.ipv4.ignore_auto_dns, flag),
                "ipv6.ignore-auto-dns" => put(self.ipv6.ignore_auto_dns, flag),
                "ipv4.ignore-auto-routes" => put(self.ipv4.ignore_auto_routes, flag),
                other => eprintln!("[snapshot] unknown editor setting: {other}"),
            }
        }
    }

    fn fill(self, loaded: Loaded) {
        let profile = loaded.profile.clone();
        put(self.id, profile.id.clone());
        put(self.autoconnect, profile.autoconnect);
        put(self.priority, profile.priority);
        put(self.everyone, profile.everyone);
        put(self.metered, profile.metered.clone());
        put(self.cloned_mac, profile.cloned_mac.clone());
        put(self.cloned_mac_address, profile.cloned_mac_address.clone());
        put(self.mac_randomization, profile.mac_randomization.clone());
        put(self.wake_on_lan, profile.wake_on_lan.clone());
        put(self.mtu, profile.mtu.clone());
        put(self.key_mgmt, profile.key_mgmt.clone());
        put(self.psk, profile.psk.clone());
        put(self.ip6_privacy, profile.ip6_privacy);
        fill_ip(self.ipv4, &profile.ipv4);
        fill_ip(self.ipv6, &profile.ipv6);
        put(self.loaded, Some(loaded));
    }

    /// The values as the form shows them now.
    fn edit(self) -> Edit {
        Edit {
            id: self.id.read().clone(),
            autoconnect: *self.autoconnect.read(),
            priority: *self.priority.read(),
            everyone: *self.everyone.read(),
            metered: self.metered.read().clone(),
            cloned_mac: self.cloned_mac.read().clone(),
            cloned_mac_address: self.cloned_mac_address.read().clone(),
            mac_randomization: self.mac_randomization.read().clone(),
            wake_on_lan: self.wake_on_lan.read().clone(),
            mtu: self.mtu.read().clone(),
            key_mgmt: self.key_mgmt.read().clone(),
            psk: self.psk.read().clone(),
            ip6_privacy: *self.ip6_privacy.read(),
            ipv4: read_ip(self.ipv4),
            ipv6: read_ip(self.ipv6),
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
                    Err(error) => self.overlays.toast(error.0),
                }
            },
        );
    }

    /// Ask first: forgetting drops the saved password.
    fn confirm_forget(self) {
        let Some(target) = self.target.read().clone() else {
            return;
        };
        let name = target.name.clone();
        let window = self.window;
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
                    &[
                        "connection".to_string(),
                        "delete".to_string(),
                        "id".to_string(),
                        name,
                    ],
                    QUICK,
                )
            },
            move |result| {
                put(self.busy, false);
                match result {
                    Ok(()) => window.close(),
                    Err(error) => self.overlays.toast(error.0),
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
        let problems = editor.edit().problems();
        let pending = editor.pending();
        let ready =
            !busy && editor.loaded.read().is_some() && problems.is_empty() && pending.is_some();
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
            _ => details_tab(editor, &loaded),
        };
        column = column.child(content);

        for problem in problems {
            column = column.child(caption(problem));
        }
        if let Some(args) = pending.as_ref()
            && needs_reconnect(args)
        {
            column = column.child(caption(
                "Applying reconnects the network to save name, address, DNS or security changes.",
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
        let offered = tab_options(self.wireless);
        let selected = selected_tab(&editor.tab.read(), &offered);
        segmented(offered, &selected, move |key: String| put(editor.tab, key)).fill()
    }
}

/// The Details tab: what the network is, then the switches and priority that decide when it connects.
fn details_tab(editor: Editor, loaded: &Loaded) -> Element {
    let Some(target) = editor.target.read().clone() else {
        return popup_column().into();
    };
    let mut facts = Group::titled("Network");
    for (title, value) in target.facts.iter().chain(loaded.live.iter()) {
        facts = facts.row(row(title.clone()).value(value.clone()));
    }
    let settings = Group::new(None::<String>)
        .row(switch_row(
            "Connect automatically",
            *editor.autoconnect.read(),
            move |value: bool| put(editor.autoconnect, value),
        ))
        .row(
            row("Priority")
                .subtitle(
                    "Higher priorities connect first when several saved networks are in range.",
                )
                .trailing(
                    stepper(*editor.priority.read(), move |value: i64| {
                        put(editor.priority, value)
                    })
                    .range(i64::from(i32::MIN), i64::from(i32::MAX)),
                ),
        )
        .row(switch_row(
            "Available to other users",
            *editor.everyone.read(),
            move |value: bool| put(editor.everyone, value),
        ))
        .row(choice_row(
            "Metered connection",
            options(METERED_CHOICES),
            &editor.metered.read(),
            move |value: String| put(editor.metered, value),
        ));
    let forget = Group::new(None::<String>).row(
        row("Forget network")
            .destructive()
            .on_press(move |_| editor.confirm_forget()),
    );
    popup_column()
        .child(facts)
        .child(settings)
        .child(forget)
        .into()
}

/// The Identity tab: the name and BSSID, then the hardware group. The hardware group holds the MAC
/// address choices, the MTU, and the Wi-Fi and wired-only options.
fn identity_tab(editor: Editor, loaded: &Loaded, wireless: bool) -> Element {
    let mut name =
        Group::titled("Identity").row(form_row("Name", text_field(editor.id, |_: String| {})));
    if wireless && !loaded.profile.bssid.is_empty() {
        name = name.row(row("BSSID").value(loaded.profile.bssid.clone()));
    }

    let cloned = editor.cloned_mac.read().clone();
    let mut hardware = Group::titled("Hardware").row(
        choice_row(
            "Cloned MAC address",
            options(CLONED_MAC_CHOICES),
            &cloned,
            move |value: String| put(editor.cloned_mac, value),
        )
        .subtitle(cloned_mac_caption(&cloned)),
    );
    if cloned == "address" {
        hardware = hardware.row(form_row(
            "MAC address",
            text_field(editor.cloned_mac_address, |_: String| {}).placeholder("02:00:00:00:00:01"),
        ));
    }
    if wireless {
        let randomization = editor.mac_randomization.read().clone();
        hardware = hardware.row(
            choice_row(
                "Random MAC address",
                options(MAC_RANDOMIZATION_CHOICES),
                &randomization,
                move |value: String| put(editor.mac_randomization, value),
            )
            .subtitle(mac_randomization_caption(&randomization)),
        );
    }

    let automatic_mtu = editor.mtu.read().as_str() == "auto";
    hardware = hardware.row(
        choice_row(
            "MTU",
            options(&[("auto", "Automatic"), ("custom", "Custom")]),
            if automatic_mtu { "auto" } else { "custom" },
            move |value: String| {
                if value == "auto" {
                    put(editor.mtu, "auto".to_string());
                } else if *editor.mtu.read() == "auto" {
                    put(editor.mtu, "1500".to_string());
                }
            },
        )
        .subtitle("Automatic uses the network's usual packet size. Custom sets it in bytes."),
    );
    if !automatic_mtu {
        hardware = hardware.row(form_row(
            "MTU in bytes",
            text_field(editor.mtu, |_: String| {}).placeholder("1500"),
        ));
    }

    if !wireless {
        let wake = editor.wake_on_lan.read().clone();
        hardware = hardware.row(
            choice_row(
                "Wake on LAN",
                options(WAKE_ON_LAN_CHOICES),
                &wake,
                move |value: String| put(editor.wake_on_lan, value),
            )
            .subtitle(wake_on_lan_caption(&wake)),
        );
    }

    popup_column().child(name).child(hardware).into()
}

/// What Wake on LAN does for each choice.
fn wake_on_lan_caption(mode: &str) -> &'static str {
    match mode {
        "magic" => "Wakes when it receives a magic packet from the network.",
        "none" => "Does not wake from the network.",
        _ => "Follows the system setting.",
    }
}

/// The IPv4 or IPv6 tab: the method, then only the fields and switches that method uses.
fn ip_tab(editor: Editor, version: u8) -> Element {
    let (fields, methods, title, placeholders) = if version == 4 {
        (
            editor.ipv4,
            IPV4_METHODS,
            "IPv4",
            ("192.168.1.20/24", "192.168.1.1", "1.1.1.1, 8.8.8.8"),
        )
    } else {
        (
            editor.ipv6,
            IPV6_METHODS,
            "IPv6",
            ("2001:db8::20/64", "2001:db8::1", "2606:4700:4700::1111"),
        )
    };
    let method = fields.method.read().clone();
    let visible = shown(version, &method);

    let mut group = Group::titled(title).row(
        choice_row("Method", options(methods), &method, move |value: String| {
            put(fields.method, value)
        })
        .subtitle(method_caption(version, &method)),
    );
    if visible.addresses {
        group = group.row(form_row(
            "Addresses",
            text_field(fields.addresses, |_: String| {}).placeholder(placeholders.0),
        ));
    }
    if visible.gateway {
        group = group.row(form_row(
            "Gateway",
            text_field(fields.gateway, |_: String| {}).placeholder(placeholders.1),
        ));
    }
    if visible.dns {
        group = group.row(form_row(
            "DNS servers",
            text_field(fields.dns, |_: String| {}).placeholder(placeholders.2),
        ));
    }
    if visible.dns_search {
        group = group.row(form_row(
            "Search domains",
            text_field(fields.dns_search, |_: String| {}).placeholder("example.com"),
        ));
    }
    if visible.auto_dns {
        group = group.row(switch_row(
            "Automatic DNS",
            !*fields.ignore_auto_dns.read(),
            move |value: bool| put(fields.ignore_auto_dns, !value),
        ));
    }
    if visible.auto_routes {
        group = group.row(switch_row(
            "Automatic routes",
            !*fields.ignore_auto_routes.read(),
            move |value: bool| put(fields.ignore_auto_routes, !value),
        ));
    }
    if visible.never_default {
        group = group.row(switch_row(
            "Use this connection only for resources on its network",
            *fields.never_default.read(),
            move |value: bool| put(fields.never_default, value),
        ));
    }

    let mut column = popup_column().child(group);
    if visible.privacy {
        let privacy = Group::titled("Privacy").row(
            choice_row(
                "Privacy extensions",
                options(IP6_PRIVACY_CHOICES),
                &editor.ip6_privacy.read().to_string(),
                move |value: String| {
                    if let Ok(parsed) = value.parse::<i64>() {
                        put(editor.ip6_privacy, parsed);
                    }
                },
            )
            .subtitle(
                "Temporary addresses make it harder to follow this computer between networks.",
            ),
        );
        column = column.child(privacy);
    }
    column.into()
}

/// The Security tab: the key type, then the password when the key type needs one.
fn security_tab(editor: Editor) -> Element {
    let key = editor.key_mgmt.read().clone();
    let mut group = Group::titled("Security").row(choice_row(
        "Security",
        options(SECURITY_CHOICES),
        &key,
        move |value: String| put(editor.key_mgmt, value),
    ));
    if key != "none" {
        group = group.row(form_row(
            "Password",
            text_field(editor.psk, |_: String| {})
                .placeholder("Password")
                .password(),
        ));
    }
    popup_column().child(group).into()
}
