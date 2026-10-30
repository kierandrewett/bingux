//! Adding a VPN: the chooser and the form for each kind.
//!
//! The kinds on offer are found when the page opens: WireGuard, which
//! NetworkManager has built in, and every plugin whose `.name` file is in a
//! NetworkManager VPN directory and whose program is installed
//! (`vpn::plugins`). A new profile is made by `nmcli`: a WireGuard tunnel from
//! a wg-quick file written for the one call, a plugin VPN with
//! `connection add type vpn`, and an imported file with `connection import`.
//! The message of a failed `nmcli` is shown in the form as it came.

use std::path::Path;

use freya::prelude::*;
use bingux_ui::style::S10;

use super::model::{self, NetCtx};
use super::vpn::data::PluginProfile;
use super::vpn::plugins::{self, Plugin};
use super::vpn::wireguard::{self, Interface, NewTunnel, Peer, check};
use super::vpn::{data, nm};
use super::vpn_tunnel::{
    Extra, FieldStates, compose, field_error, field_rows, interface_error, peer_error,
};
use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::routes;
use crate::system;
use crate::ui::files::{ChooserRequest, Filter, choose};
use crate::ui::{
    Group, Status, action_button, action_row, choice_row, form_field, page_column, primary_button,
    row, status, switch_row,
};

/// The header title of the add-VPN flow for `kind`.
pub fn add_vpn_title(kind: Option<&str>) -> String {
    match kind {
        None => "Add VPN".to_string(),
        Some("import") => "Import VPN".to_string(),
        Some(kind) => format!("Add {}", plugins::display_name(kind)),
    }
}

/// The installed plugins, once they are found. Call from `render`.
fn use_plugins() -> State<Option<Vec<Plugin>>> {
    let found = use_state(|| None::<Vec<Plugin>>);
    use_hook(move || {
        let mut found = found;
        system::background_then(plugins::discover, move |list| found.set(Some(list)));
    });
    found
}

/// The names of the connections that exist, so a new one can be refused a
/// name that is taken. `None` until they are read.
fn use_taken_names() -> State<Option<Vec<String>>> {
    let taken = use_state(|| None::<Vec<String>>);
    use_hook(move || {
        let mut taken = taken;
        system::background_then(nm::connection_names, move |names| {
            // A failed read leaves no names to compare; nmcli itself refuses
            // nothing here, so the form then only checks the rest.
            taken.set(Some(names.unwrap_or_default()))
        });
    });
    taken
}

/// Why `name` cannot be the new VPN's name, or `None`.
fn name_problem(name: &str, taken: &[String]) -> Option<String> {
    if name.is_empty() {
        None
    } else if taken.iter().any(|other| other == name) {
        Some("A connection with this name already exists.".to_string())
    } else {
        None
    }
}

/// Run `work`, which makes a VPN and returns its name, then open that VPN's
/// screen. The overview is read again first, so the screen finds the new
/// profile. A failure is shown in the form.
fn create_then_open(
    ctx: NetCtx,
    busy: State<bool>,
    error: State<Option<String>>,
    work: impl FnOnce() -> Result<String, String> + Send + 'static,
) {
    let (mut busy, mut error) = (busy, error);
    busy.set(true);
    error.set(None);
    system::background_then(
        move || {
            let name = work()?;
            let overview = system::load_persistent("network-overview", model::load_overview);
            Ok::<_, String>((name, overview))
        },
        move |result| {
            busy.set(false);
            match result {
                Ok((name, overview)) => {
                    let mut shown = ctx.overview;
                    shown.set(Some(overview));
                    ctx.router.open(routes::vpn(&name));
                }
                Err(message) => error.set(Some(message)),
            }
        },
    );
}

/// The bar at the end of a form: the action that makes the VPN.
fn submit_row(
    label: &str,
    note: Option<&str>,
    enabled: bool,
    busy: bool,
    on_press: impl Fn() + 'static,
) -> Element {
    Group::new(None::<String>)
        .row(action_row(
            note,
            primary_button(label, move |_| on_press())
                .enabled(enabled && !busy)
                .loading(busy),
        ))
        .into()
}

/// Add a VPN: the chooser, or the form for one kind.
#[derive(PartialEq)]
pub struct AddVpnPage {
    pub kind: Option<String>,
}

impl Component for AddVpnPage {
    fn render(&self) -> impl IntoElement {
        match self.kind.as_deref() {
            None => Element::from(Chooser),
            Some("wireguard") => Element::from(WireguardForm),
            Some("import") => Element::from(ImportForm),
            Some(short) => Element::from(PluginAdd {
                short: short.to_string(),
            }),
        }
    }
}

/// The types of VPN to choose from, and importing a file.
#[derive(PartialEq)]
struct Chooser;

impl Component for Chooser {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let found = use_plugins();
        let installed = found.read().clone();
        let loading = installed.is_none();
        let installed = installed.unwrap_or_default();
        let open = move |kind: &str| {
            let kind = kind.to_string();
            move |_: ()| {
                ctx.router.open(routes::add_vpn(Some(&kind.clone())))
            }
        };

        let mut types = Group::titled("Type").row(
            row("WireGuard")
                .leading(Icon::NetworkVpn)
                .navigates()
                .on_press(open("wireguard")),
        );
        for plugin in &installed {
            types = types.row(
                row(plugin.name.clone())
                    .leading(Icon::NetworkVpn)
                    .navigates()
                    .on_press(open(&plugin.short)),
            );
        }
        let import = Group::titled("From a file").row(
            row("Import from file…")
                .leading(Icon::DocumentOpen)
                .navigates()
                .on_press(open("import")),
        );
        veiled(page_column().child(types).child(import), loading)
    }
}

/// A WireGuard tunnel made by hand: interface, one peer, and where traffic goes.
#[derive(PartialEq)]
struct WireguardForm;

impl Component for WireguardForm {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let taken = use_taken_names();
        let name = use_state(String::new);
        let private_key = use_state(String::new);
        let addresses = use_state(String::new);
        let dns = use_state(String::new);
        let public_key = use_state(String::new);
        let preshared = use_state(String::new);
        let endpoint = use_state(String::new);
        let allowed = use_state(String::new);
        let keepalive = use_state(|| "25".to_string());
        let busy = use_state(|| false);
        let error = use_state(|| None::<String>);
        let has_wg = wireguard::use_has_wg();

        let read = move || {
            let keepalive_text = keepalive.read().trim().to_string();
            let tunnel = NewTunnel {
                interface: Interface {
                    private_key: private_key.read().trim().to_string(),
                    addresses: wireguard::parse_list(&addresses.read()),
                    dns: wireguard::parse_list(&dns.read()),
                    ..Interface::default()
                },
                peer: Peer {
                    public_key: public_key.read().trim().to_string(),
                    preshared_key: preshared.read().trim().to_string(),
                    endpoint: endpoint.read().trim().to_string(),
                    allowed_ips: wireguard::parse_list(&allowed.read()),
                    keepalive: keepalive_text.parse().unwrap_or(0),
                },
            };
            (name.read().trim().to_string(), tunnel, keepalive_text)
        };
        let taken_names = taken.read().clone().unwrap_or_default();
        let (tunnel_name, tunnel, keepalive_text) = read();
        let problem = if tunnel_name.is_empty() {
            Some("Give the VPN a name.".to_string())
        } else {
            name_problem(&tunnel_name, &taken_names)
                .or_else(|| interface_error(&tunnel.interface))
                .or_else(|| peer_error(&tunnel.peer, &keepalive_text, &[]))
        };

        let create = move || {
            let (tunnel_name, tunnel, keepalive_text) = read();
            let names = taken.read().clone().unwrap_or_default();
            if *busy.peek()
                || tunnel_name.is_empty()
                || name_problem(&tunnel_name, &names).is_some()
                || interface_error(&tunnel.interface).is_some()
                || peer_error(&tunnel.peer, &keepalive_text, &[]).is_some()
            {
                return;
            }
            create_then_open(ctx, busy, error, move || {
                wireguard::import_text(&wireguard::compose(&tunnel), &tunnel_name)
            });
        };

        let generate = move |_: ()| {
            system::background_then(
                wireguard::generate_private_key,
                move |result| match result {
                    Ok(key) => {
                        let mut private_key = private_key;
                        private_key.set(key);
                    }
                    Err(message) => ctx.overlays.toast(message),
                },
            );
        };

        let all = wireguard::is_all_traffic(&wireguard::parse_list(&allowed.read()));
        let mut interface = Group::titled("Interface").row(
            form_field("Private key", private_key)
                .password()
                .monospace()
                .hint(if has_wg {
                    "Stays on this computer. Generate one, or paste your own."
                } else {
                    "Paste the key from your VPN provider, or make one with the wg genkey command."
                })
                .validate(check::key),
        );
        if has_wg {
            interface = interface.row(action_row(
                None,
                action_button("Generate key", generate).compact(),
            ));
        }
        interface = interface
            .row(
                form_field("Addresses", addresses)
                    .multiline(2)
                    .monospace()
                    .hint("This computer's addresses inside the tunnel, one per line, with the prefix length.")
                    .placeholder("10.8.0.2/32")
                    .validate(check::cidr_list),
            )
            .row(
                form_field("DNS servers", dns)
                    .multiline(2)
                    .monospace()
                    .optional()
                    .hint("Used while the tunnel is up, one per line.")
                    .validate(check::dns_list),
            );
        let peer = Group::titled("Peer")
            .row(
                form_field("Public key", public_key)
                    .monospace()
                    .hint("The other side's public key.")
                    .validate(check::key),
            )
            .row(
                form_field("Endpoint", endpoint)
                    .optional()
                    .hint("Where to reach it, as host:port.")
                    .placeholder("vpn.example.com:51820")
                    .validate(check::endpoint),
            )
            .row(
                form_field("Preshared key", preshared)
                    .password()
                    .monospace()
                    .optional()
                    .hint("An extra secret both sides share, if they use one.")
                    .validate(check::key),
            )
            .row(
                form_field("Keepalive", keepalive)
                    .optional()
                    .suffix("seconds")
                    .hint("Keeps the connection open through a router. 25 is usual.")
                    .validate(check::keepalive),
            );
        let mut traffic = Group::titled("Traffic")
            .row(switch_row("Route all traffic", all, move |on: bool| {
                let mut allowed = allowed;
                allowed.set(if on {
                    wireguard::ALL_TRAFFIC.join("\n")
                } else {
                    String::new()
                });
            }));
        if !all {
            traffic = traffic.row(
                form_field("Allowed addresses", allowed)
                    .multiline(2)
                    .monospace()
                    .hint("Networks that go through this peer, one per line.")
                    .placeholder("10.8.0.0/24")
                    .validate(check::cidr_list),
            );
        }

        page_column()
            .child(
                Group::titled("VPN").row(
                    form_field("Name", name)
                        .hint("How this VPN is listed.")
                        .placeholder("Work")
                        .error(name_problem(&tunnel_name, &taken_names)),
                ),
            )
            .child(interface)
            .child(peer)
            .child(traffic)
            .child(bingux_ui::motion::reveal_block_if(
                S10,
                error
                    .read()
                    .clone()
                    .map(|message| Element::from(status(Status::Error(message)))),
            ))
            .child(submit_row(
                "Add VPN",
                Some("It does not connect until you turn it on."),
                problem.is_none(),
                *busy.read(),
                create,
            ))
    }
}

/// Split `text` into `name = value` lines for the key and value list of a
/// plugin this app has no fields for.
fn parse_lines(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut entries = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match line.split_once('=') {
            Some((key, value)) if !key.trim().is_empty() && !key.contains(',') => {
                entries.push((key.trim().to_string(), value.trim().to_string()))
            }
            _ => {
                return Err(format!("Line {} needs the form name = value.", number + 1));
            }
        }
    }
    Ok(entries)
}

fn check_lines(text: &str) -> Result<(), String> {
    parse_lines(text).map(|_| ())
}

/// A plugin VPN made by hand: its friendly fields, or a key and value list.
#[derive(PartialEq)]
struct PluginAdd {
    short: String,
}

impl Component for PluginAdd {
    fn render(&self) -> impl IntoElement {
        let found = use_plugins();
        let installed = found.read().clone();
        let Some(list) = installed else {
            return veiled(page_column().child(placeholder_form()), true);
        };
        match list.into_iter().find(|plugin| plugin.short == self.short) {
            Some(plugin) => page_column().child(PluginAddForm { plugin }),
            None => page_column().child(status(Status::Empty(format!(
                "{} is not installed on this computer.",
                plugins::display_name(&self.short)
            )))),
        }
    }
}

fn placeholder_form() -> Element {
    Group::titled("VPN")
        .row(row("Name").value("—"))
        .row(row("Server").value("—"))
        .into()
}

#[derive(PartialEq)]
struct PluginAddForm {
    plugin: Plugin,
}

impl Component for PluginAddForm {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let taken = use_taken_names();
        let short = self.plugin.short.clone();
        let fields = plugins::fields(&short);
        let known = plugins::is_known(&short);
        let blank = PluginProfile::default();
        let states = FieldStates::new(fields, &blank);
        let name = use_state(String::new);
        let raw = use_state(String::new);
        let busy = use_state(|| false);
        let error = use_state(|| None::<String>);

        let texts = states.read();
        let taken_names = taken.read().clone().unwrap_or_default();
        let vpn_name = name.read().trim().to_string();
        let lines = parse_lines(&raw.read());
        let problem = if vpn_name.is_empty() {
            Some("Give the VPN a name.".to_string())
        } else {
            name_problem(&vpn_name, &taken_names)
                .or_else(|| {
                    field_error(fields, &texts.values, &|field| texts.visible(fields, field))
                })
                .or_else(|| lines.clone().err())
                .or_else(|| {
                    (!known && lines.as_ref().is_ok_and(|entries| entries.is_empty())).then(|| {
                        "Add the settings this VPN needs, one name = value per line.".to_string()
                    })
                })
        };

        let create = {
            let short = short.clone();
            let states = states.clone();
            move || {
                let texts = states.read();
                let Ok(extras) = parse_lines(&raw.read()) else {
                    return;
                };
                let vpn_name = name.read().trim().to_string();
                if *busy.peek() || vpn_name.is_empty() {
                    return;
                }
                let extras: Vec<_> = extras
                    .into_iter()
                    .map(|(key, value)| Extra {
                        key,
                        value,
                        secret: false,
                    })
                    .collect();
                let profile = compose(
                    &short,
                    &PluginProfile::default(),
                    plugins::fields(&short),
                    &texts.values,
                    &texts.flags,
                    &extras,
                );
                let short = short.clone();
                create_then_open(ctx, busy, error, move || {
                    add_plugin_profile(&short, &vpn_name, &profile)
                });
            }
        };

        let mut group = Group::titled(&self.plugin.name);
        if known {
            group = field_rows(group, fields, &states, &texts);
        } else {
            group = field_rows(group, fields, &states, &texts)
                .row(
                    form_field("Settings", raw)
                        .multiline(4)
                        .monospace()
                        .placeholder("gateway = vpn.example.com")
                        .hint("One name = value per line.")
                        .validate(check_lines),
                );
        }

        page_column()
            .child(
                Group::titled("VPN").row(
                    form_field("Name", name)
                        .hint("How this VPN is listed.")
                        .placeholder("Work")
                        .error(name_problem(&vpn_name, &taken_names)),
                ),
            )
            .child(group)
            .child(bingux_ui::motion::reveal_block_if(
                S10,
                error
                    .read()
                    .clone()
                    .map(|message| Element::from(status(Status::Error(message)))),
            ))
            .child(submit_row(
                "Add VPN",
                Some("It does not connect until you turn it on."),
                problem.is_none(),
                *busy.read(),
                create,
            ))
    }
}

/// Make a plugin VPN with `nmcli connection add type vpn`. Returns its name.
fn add_plugin_profile(short: &str, name: &str, profile: &PluginProfile) -> Result<String, String> {
    let data_text = data::serialise(&profile.data);
    let secrets_text = data::serialise(&profile.secrets);
    let mut args = vec![
        "connection",
        "add",
        "type",
        "vpn",
        "vpn-type",
        short,
        "con-name",
        name,
        // A new VPN does not connect by itself until the person says so.
        "connection.autoconnect",
        "no",
    ];
    if !data_text.is_empty() {
        args.extend(["vpn.data", &data_text]);
    }
    if !secrets_text.is_empty() {
        args.extend(["vpn.secrets", &secrets_text]);
    }
    if !profile.user_name.is_empty() {
        args.extend(["vpn.user-name", &profile.user_name]);
    }
    nm::run(&args)?;
    Ok(name.to_string())
}

/// Which type of VPN the file at `path` is, from its extension and its first
/// lines `head`, or `None` when it cannot tell.
fn detect_kind(path: &str, head: &str) -> Option<&'static str> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("ovpn") => return Some("openvpn"),
        Some("pcf") => return Some("vpnc"),
        Some("sh") => return Some("ssh"),
        _ => {}
    }
    let lower = head.to_ascii_lowercase();
    let has_line = |start: &str| {
        lower
            .lines()
            .any(|line| line.trim_start().starts_with(start))
    };
    if has_line("[interface]") {
        Some("wireguard")
    } else if has_line("remote ") || has_line("client") {
        Some("openvpn")
    } else if has_line("ipsec gateway") {
        Some("vpnc")
    } else {
        None
    }
}

/// The name a file's profile gets: the file's name without its extension.
fn stem_of(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("VPN")
        .to_string()
}

/// Import the file at `path`, as VPN type `kind` or the one it looks like.
/// Returns the new profile's name.
fn import_file(path: &str, kind: &str, installed: &[Plugin]) -> Result<String, String> {
    let text = crate::system::read_to_string(path)
        .map_err(|error| format!("The file could not be read: {error}."))?;
    let head: String = text.lines().take(60).collect::<Vec<_>>().join("\n");
    let kind = if kind == "auto" {
        detect_kind(path, &head)
            .ok_or_else(|| {
                "This file does not say what kind of VPN it is. Choose the type above.".to_string()
            })?
            .to_string()
    } else {
        kind.to_string()
    };
    if kind == "wireguard" {
        return wireguard::import_text(&text, &stem_of(path));
    }
    let plugin = installed
        .iter()
        .find(|plugin| plugin.short == kind)
        .ok_or_else(|| {
            format!(
                "{} is not installed on this computer.",
                plugins::display_name(&kind)
            )
        })?;
    let existing = nm::connection_names()?;
    let added = nm::run(&["connection", "import", "type", &plugin.short, "file", path])?;
    let (made, uuid) = nm::added(&added)
        .ok_or_else(|| "NetworkManager did not say which profile it made.".to_string())?;
    // Two profiles with one name cannot be told apart by `nmcli ... id`.
    let name = wireguard::unique_name(&made, &existing);
    nm::run(&[
        "connection",
        "modify",
        "uuid",
        &uuid,
        "connection.id",
        &name,
        "connection.autoconnect",
        "no",
    ])?;
    Ok(name)
}

/// Let the person pick the file with the system chooser; `chosen` gets its
/// path. Cancelling does nothing.
fn browse(ctx: NetCtx, chosen: impl FnOnce(String) + 'static) {
    let configuration = ["*.conf", "*.ovpn", "*.pcf", "*.sh"]
        .into_iter()
        .fold(Filter::new("VPN configuration"), |filter, pattern| {
            filter.glob(pattern)
        });
    choose(
        ctx.window,
        ChooserRequest::open()
            .title("Choose a VPN file")
            .filter(configuration)
            .filter(Filter::new("All files").glob("*")),
        move |answer| {
            if let Some(path) = answer.and_then(|answer| answer.paths.into_iter().next()) {
                chosen(path.to_string_lossy().into_owned());
            }
        },
    );
}

/// A path that is plausible before the file is read.
fn check_path(text: &str) -> Result<(), String> {
    if text.starts_with('/') {
        Ok(())
    } else {
        Err("Enter the full path of the file, starting with /.".to_string())
    }
}

/// Import a VPN from a file.
#[derive(PartialEq)]
struct ImportForm;

impl Component for ImportForm {
    fn render(&self) -> impl IntoElement {
        let ctx = model::use_net_ctx();
        let found = use_plugins();
        let path = use_state(String::new);
        let kind = use_state(|| "auto".to_string());
        let busy = use_state(|| false);
        let error = use_state(|| None::<String>);

        let installed = found.read().clone().unwrap_or_default();
        let mut options = vec![
            ("auto".to_string(), "Detect from the file".to_string()),
            ("wireguard".to_string(), "WireGuard".to_string()),
        ];
        options.extend(
            installed
                .iter()
                .map(|plugin| (plugin.short.clone(), plugin.name.clone())),
        );
        let chosen_path = path.read().trim().to_string();
        let chosen_kind = kind.read().clone();
        let valid = check_path(&chosen_path).is_ok();

        let import = {
            let installed = installed.clone();
            move || {
                let file = path.read().trim().to_string();
                if *busy.peek() || check_path(&file).is_err() {
                    return;
                }
                let kind = kind.read().clone();
                let installed = installed.clone();
                create_then_open(ctx, busy, error, move || {
                    import_file(&file, &kind, &installed)
                });
            }
        };

        let hint = {
            let mut extensions: Vec<String> = vec![".conf (WireGuard)".to_string()];
            for plugin in &installed {
                let list = plugins::import_extensions(&plugin.short);
                if !list.is_empty() {
                    extensions.push(format!(".{} ({})", list[0], plugin.name));
                }
            }
            format!("Reads {}.", extensions.join(", "))
        };
        let file_group = Group::titled("File")
            .row(
                form_field("File", path)
                    .monospace()
                    .hint(hint)
                    .placeholder("/home/you/work.ovpn")
                    .validate(check_path)
                    .on_submit({
                        let import = import.clone();
                        move |_| import()
                    }),
            )
            .row(action_row(
                Some("Or choose the file from your folders."),
                action_button("Browse…", move |_| {
                    browse(ctx, move |file| {
                        let mut path = path;
                        path.set(file);
                    })
                })
                .compact(),
            ))
            .row(choice_row(
                "Type",
                options,
                &chosen_kind,
                move |key: String| {
                    let mut kind = kind;
                    kind.set(key);
                },
            ));

        page_column()
            .child(file_group)
            .child(bingux_ui::motion::reveal_block_if(
                S10,
                error
                    .read()
                    .clone()
                    .map(|message| Element::from(status(Status::Error(message)))),
            ))
            .child(submit_row(
                "Import",
                Some(
                    "The VPN opens when it is imported. It does not connect until you turn it on.",
                ),
                valid,
                *busy.read(),
                import,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(add_vpn_title(None), "Add VPN");
        assert_eq!(add_vpn_title(Some("import")), "Import VPN");
        assert_eq!(add_vpn_title(Some("wireguard")), "Add WireGuard");
        assert_eq!(add_vpn_title(Some("openvpn")), "Add OpenVPN");
        assert_eq!(add_vpn_title(Some("wibble")), "Add Wibble");
    }

    #[test]
    fn detects_the_kind_of_a_file() {
        assert_eq!(detect_kind("/a/work.ovpn", ""), Some("openvpn"));
        assert_eq!(detect_kind("/a/cisco.pcf", ""), Some("vpnc"));
        assert_eq!(detect_kind("/a/tunnel.sh", ""), Some("ssh"));
        assert_eq!(
            detect_kind("/a/wg0.conf", "# comment\n[Interface]\nPrivateKey = x\n"),
            Some("wireguard")
        );
        assert_eq!(
            detect_kind(
                "/a/x.conf",
                "client\ndev tun\nremote vpn.example.com 1194\n"
            ),
            Some("openvpn")
        );
        assert_eq!(
            detect_kind("/a/x.conf", "IPSec gateway 1.2.3.4\n"),
            Some("vpnc")
        );
        assert_eq!(detect_kind("/a/notes.txt", "hello"), None);
    }

    #[test]
    fn profile_names_come_from_the_file() {
        assert_eq!(stem_of("/home/k/work vpn.ovpn"), "work vpn");
        assert_eq!(stem_of("/home/k/wg0.conf"), "wg0");
    }

    #[test]
    fn names_that_exist_are_refused() {
        let taken = vec!["Work".to_string()];
        assert_eq!(name_problem("", &taken), None);
        assert_eq!(name_problem("Home", &taken), None);
        assert!(name_problem("Work", &taken).is_some());
    }

    #[test]
    fn settings_lines_are_parsed() {
        assert_eq!(
            parse_lines("gateway = vpn.example.com\n\n user = bob ").unwrap(),
            vec![
                ("gateway".to_string(), "vpn.example.com".to_string()),
                ("user".to_string(), "bob".to_string())
            ]
        );
        assert_eq!(
            parse_lines("gateway = a\nnonsense\n").unwrap_err(),
            "Line 2 needs the form name = value."
        );
        assert!(parse_lines("= value").is_err());
        assert!(check_lines("").is_ok());
    }

    #[test]
    fn paths_must_be_absolute() {
        assert!(check_path("/home/k/a.ovpn").is_ok());
        assert!(check_path("a.ovpn").is_err());
    }
}
