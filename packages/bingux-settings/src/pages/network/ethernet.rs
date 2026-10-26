//! Wired Ethernet: the device screen, its profiles, and one profile's settings screen.

use std::cell::RefCell;
use std::collections::HashMap;

use freya::prelude::*;

use super::model::{
    CONNECT, NetCtx, Overview, QUICK, capitalise, hero, nmcli, pending_for, split_terse,
    toggle_link, use_net_ctx,
};
use super::profile::{ProfileSettings, speed_text};
use super::{Device, Owner, UsageScope, device_icon, failure_page};
use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::routes::Route;
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, MenuItem, PopupAction, PopupActions, PopupFlags, ResponseKind, Status, StripAction,
    Window, WindowLike, WindowParent, action_strip, dialog, form_field, menu_item, menu_separator,
    page_column, popup, popup_column, row, row_button, status, strip_action, switch,
    use_popup_actions, use_tokens, use_window,
};
use bingux_ui::controls::form::validate::Check;

/// The title of a wired device's row and screen: its connection, or `Ethernet`.
fn device_title(device: &Device) -> String {
    if device.has_connection() {
        device.connection.clone()
    } else {
        "Ethernet".to_string()
    }
}

/// The header title of the wired device `device` from the overview loaded at
/// start-up, so a screen opened by address shows the title its row shows.
pub fn cached_ethernet_title(device: &str) -> Option<String> {
    let model = system::cached_persistent::<Overview, system::CommandError>("network-overview");
    let loaded = model.read();
    let ov = loaded.as_ref()?.as_ref().ok()?;
    let found = ov.ethernet_devices().find(|known| known.name == device)?;
    Some(device_title(found))
}

/// Whether no cable is plugged into the device.
fn unplugged(device: &Device) -> bool {
    !device.carrier || device.state == "unavailable"
}

/// The pending switch key of a wired device.
fn link_key(device: &str) -> String {
    format!("wired:{device}")
}

/// Whether the device is connected, or about to be while a change is in flight.
fn shown_connected(device: &Device) -> bool {
    pending_for(&link_key(&device.name)).unwrap_or(device.connected())
}

/// The link line: `Connected · 1 Gbit/s`, `Cable unplugged`, `Not connected`.
fn link_state(device: &Device) -> String {
    link_line(device, pending_for(&link_key(&device.name)))
}

/// [`link_state`] for a switch that is changing to `pending`, when one is.
fn link_line(device: &Device, pending: Option<bool>) -> String {
    let word = link_word(device, pending);
    match device.speed {
        Some(speed) if pending.is_none() && device.connected() && !unplugged(device) => {
            format!("{word} · {}", speed_text(speed))
        }
        _ => word,
    }
}

/// What the link is doing, in one word or two: `Connected`, `Cable unplugged`, `Connecting`.
fn link_word(device: &Device, pending: Option<bool>) -> String {
    if let Some(on) = pending {
        return if on { "Connecting" } else { "Disconnecting" }.to_string();
    }
    if device.state.starts_with("connecting") || device.state.starts_with("deactivating") {
        return capitalise(device.state.split(' ').next().unwrap_or(&device.state));
    }
    if unplugged(device) {
        return "Cable unplugged".to_string();
    }
    if device.connected() {
        return "Connected".to_string();
    }
    "Not connected".to_string()
}

/// A row's line: link state, then the address when there is one.
fn row_subtitle(device: &Device) -> String {
    let state = link_state(device);
    with_address(device, state)
}

fn with_address(device: &Device, state: String) -> String {
    let address = device.address4();
    if device.connected() && !address.is_empty() {
        format!("{state} · {address}")
    } else {
        state
    }
}

/// Connect or disconnect the device now; the switch shows the new state before the command ends.
fn set_link(ctx: NetCtx, device: String, on: bool) {
    toggle_link(ctx, link_key(&device), on, move || {
        let action = if on { "connect" } else { "disconnect" };
        nmcli(&["device", action, &device], CONNECT).map(|_| ())
    });
}

/// One row per wired device, for the Connections group.
pub fn ethernet_rows(ctx: NetCtx, ov: &Overview) -> Vec<Element> {
    ov.ethernet_devices()
        .map(|device| {
            let title = device_title(device);
            let route = Route::Ethernet {
                device: device.name.clone(),
                name: title.clone(),
            };
            let name = device.name.clone();
            let on = shown_connected(device);
            let usable = !unplugged(device);
            let open = route.clone();
            Element::from(
                row(title)
                    .leading(device_icon(device.kind))
                    .subtitle(row_subtitle(device))
                    .trailing(
                        switch(on, {
                            let name = name.clone();
                            move |next: bool| set_link(ctx, name.clone(), next)
                        })
                        .enabled(usable),
                    )
                    .navigates()
                    .on_press(move |_| ctx.navigator.push(open.clone()))
                    .context_menu(device_menu(ctx, device, route)),
            )
        })
        .collect()
}

/// The right-click menu of a wired device: connect or disconnect, and its settings.
fn device_menu(ctx: NetCtx, device: &Device, route: Route) -> Vec<MenuItem> {
    let name = device.name.clone();
    let connected = shown_connected(device);
    let toggle = if connected {
        menu_item("Disconnect", move |_| set_link(ctx, name.clone(), false)).icon(Icon::WindowClose)
    } else {
        menu_item("Connect", move |_| set_link(ctx, name.clone(), true)).icon(Icon::NetworkWired)
    };
    vec![
        toggle,
        menu_item("Settings", move |_| ctx.navigator.push(route.clone()))
            .icon(Icon::PreferencesSystem),
    ]
}

/// A device with the shape of a loaded one, drawn under the veil while the first read runs.
fn placeholder_device(name: &str) -> Device {
    Device {
        name: name.to_string(),
        kind: super::DeviceKind::Ethernet,
        state: "connected".to_string(),
        connection: "—".to_string(),
        uuid: String::new(),
        hwaddr: "—".to_string(),
        speed: Some(1000),
        carrier: true,
        mtu: Some(1500),
        ipv4: vec!["—".to_string()],
        ipv6: vec!["—".to_string()],
        gateway4: "—".to_string(),
        gateway6: String::new(),
        dns: vec!["—".to_string()],
    }
}

/// A saved wired profile that can run on a device.
#[derive(Clone, PartialEq)]
struct WiredProfile {
    name: String,
    uuid: String,
    /// It is the connection running on the device now.
    active: bool,
}

type ProfileList = State<Option<Result<Vec<WiredProfile>, CommandError>>>;

thread_local! {
    /// One list per device, alive for the whole window, so a read that ends after its screen is gone
    /// has somewhere to land and a screen opens with the list the last one read.
    static LISTS: RefCell<HashMap<String, ProfileList>> = RefCell::new(HashMap::new());
}

fn list_state(device: &str) -> ProfileList {
    LISTS.with(|lists| {
        *lists
            .borrow_mut()
            .entry(device.to_string())
            .or_insert_with(|| State::create_global(None))
    })
}

/// A profile of `list` output: `NAME,UUID,TYPE,DEVICE,ACTIVE`, wired profiles only.
fn parse_wired(output: &str) -> Vec<(String, String, String, bool)> {
    output
        .lines()
        .filter_map(|line| {
            let fields = split_terse(line);
            let field = |index: usize| fields.get(index).cloned().unwrap_or_default();
            (field(2) == "802-3-ethernet")
                .then(|| (field(0), field(1), field(3), field(4) == "yes"))
        })
        .collect()
}

/// Whether a profile can run on `device`: it is bound to this interface or to none, and to this
/// hardware address or to none, and it is not running on another device.
fn fits(interface: &str, mac: &str, running_on: &str, device: &str, hwaddr: &str) -> bool {
    (interface.is_empty() || interface == device)
        && (mac.is_empty() || mac.eq_ignore_ascii_case(hwaddr))
        && (running_on.is_empty() || running_on == device)
}

/// Every wired profile that can run on `device`, by name.
fn load_profiles(device: &str, hwaddr: &str) -> Result<Vec<WiredProfile>, CommandError> {
    let listed = nmcli(
        &[
            "-t",
            "-f",
            "NAME,UUID,TYPE,DEVICE,ACTIVE",
            "connection",
            "show",
        ],
        QUICK,
    )?;
    let mut profiles = Vec::new();
    for (name, uuid, running_on, active) in parse_wired(&listed) {
        let binding = nmcli(
            &[
                "-t",
                "-f",
                "connection.interface-name,802-3-ethernet.mac-address",
                "connection",
                "show",
                "uuid",
                &uuid,
            ],
            QUICK,
        )?;
        let mut interface = String::new();
        let mut mac = String::new();
        for line in binding.lines() {
            let fields = split_terse(line);
            let value = fields
                .get(1..)
                .map(|rest| rest.join(":"))
                .unwrap_or_default();
            let value = if value == "--" { String::new() } else { value };
            match fields.first().map(String::as_str) {
                Some("connection.interface-name") => interface = value,
                Some("802-3-ethernet.mac-address") => mac = value,
                _ => {}
            }
        }
        if fits(&interface, &mac, &running_on, device, hwaddr) {
            profiles.push(WiredProfile {
                name,
                uuid,
                active: active && running_on == device,
            });
        }
    }
    profiles.sort_by_key(|profile| profile.name.to_lowercase());
    Ok(profiles)
}

/// Read the device's profiles again.
fn reload_profiles(device: &str, hwaddr: &str) {
    let list = list_state(device);
    let (device, hwaddr) = (device.to_string(), hwaddr.to_string());
    background_then(
        move || load_profiles(&device, &hwaddr),
        move |result| {
            let mut list = list;
            list.set(Some(result));
        },
    );
}

/// What the screens of one device read: the device as the overview last read it, or why not.
enum Found {
    Loading(Device),
    Device(Device),
    Missing,
    Failed(CommandError),
}

fn find_device(ctx: NetCtx, name: &str) -> Found {
    match ctx.overview.read().clone() {
        None => Found::Loading(placeholder_device(name)),
        Some(Ok(ov)) => match ov.ethernet_devices().find(|device| device.name == name) {
            Some(device) => Found::Device(device.clone()),
            None => Found::Missing,
        },
        Some(Err(error)) => Found::Failed(error),
    }
}

/// Go back once the device is gone, as when its adapter is unplugged.
fn use_pop_when_gone(ctx: NetCtx, device: &str) {
    let gone = device.to_string();
    use_side_effect(move || {
        if let Some(Ok(ov)) = &*ctx.overview.read()
            && !ov.ethernet_devices().any(|known| known.name == gone)
        {
            ctx.navigator.pop();
        }
    });
}

/// Run `work` and read the overview and the device's profiles again, with the error in a toast.
fn act(
    ctx: NetCtx,
    device: &Device,
    work: impl FnOnce() -> Result<String, CommandError> + Send + 'static,
    then: impl FnOnce() + 'static,
) {
    let (name, hwaddr) = (device.name.clone(), device.hwaddr.clone());
    background_then(work, move |result| {
        match result {
            Ok(_) => then(),
            Err(error) => ctx.overlays.toast(error.0),
        }
        ctx.reload();
        reload_profiles(&name, &hwaddr);
    });
}

/// Ask first: deleting a profile removes its settings and saved passwords. `uuid` names the profile
/// exactly when it is known; otherwise the profile is found by its name.
fn confirm_delete(ctx: NetCtx, device: Device, name: String, uuid: Option<String>, leave: bool) {
    ctx.overlays.show_dialog(
        dialog(
            format!("Delete \"{name}\"?"),
            "The profile and its saved settings are removed from this computer.",
        )
        .cancel("Cancel")
        .response("Delete", ResponseKind::Destructive, move |_| {
            let (kind, target) = match &uuid {
                Some(uuid) => ("uuid", uuid.clone()),
                None => ("id", name.clone()),
            };
            act(
                ctx,
                &device,
                move || nmcli(&["connection", "delete", kind, &target], CONNECT),
                move || {
                    if leave {
                        ctx.navigator.pop();
                    }
                },
            );
        }),
    );
}

/// Make `profile` the connection running on `device`.
fn use_profile_now(ctx: NetCtx, device: &Device, kind: &'static str, target: String) {
    let interface = device.name.clone();
    act(
        ctx,
        device,
        move || {
            nmcli(
                &["connection", "up", kind, &target, "ifname", &interface],
                CONNECT,
            )
        },
        || {},
    );
}

/// The Status group: the facts of the link from the overview.
fn status_group(device: &Device) -> Element {
    let link = link_word(device, pending_for(&link_key(&device.name)));
    let mut group = Group::titled("Status").row(row("Link").value(link));
    let facts = [
        ("Speed", device.speed.map(speed_text).unwrap_or_default()),
        ("IPv4", device.ipv4.join(", ")),
        ("IPv6", device.ipv6.join(", ")),
        ("MAC address", device.hwaddr.clone()),
        ("Gateway", device.gateway4.clone()),
        ("DNS", device.dns.join(", ")),
        (
            "MTU",
            device.mtu.map(|mtu| mtu.to_string()).unwrap_or_default(),
        ),
    ];
    for (label, value) in facts {
        if !value.is_empty() {
            group = group.row(row(label).mono_value(value));
        }
    }
    group.into()
}

/// The first Connect or Disconnect action of a device's strip.
fn link_action(ctx: NetCtx, device: &Device) -> StripAction {
    let name = device.name.clone();
    if shown_connected(device) {
        strip_action(Icon::WindowClose, "Disconnect", move |_| {
            set_link(ctx, name.clone(), false)
        })
    } else {
        strip_action(Icon::NetworkWired, "Connect", move |_| {
            set_link(ctx, name.clone(), true)
        })
        .enabled(!unplugged(device))
    }
}

/// The wired device screen: its link, the profiles it can run, and where its details and usage are.
#[derive(PartialEq)]
pub struct EthernetPage {
    pub device: String,
    pub name: String,
}

impl Component for EthernetPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let tokens = use_tokens();
        use_pop_when_gone(ctx, &self.device);
        let new_name = use_state(String::new);
        let new_error = use_state(|| None::<String>);
        let (device, loading) = match find_device(ctx, &self.device) {
            Found::Loading(device) => (device, true),
            Found::Device(device) => (device, false),
            Found::Missing => return page_column().into(),
            Found::Failed(error) => return failure_page(error, move || ctx.reload()),
        };
        // The list is read again whenever what the device runs changes.
        let deps = (
            device.name.clone(),
            device.hwaddr.clone(),
            device.connection.clone(),
            device.state.clone(),
        );
        use_side_effect_with_deps(&deps, |(name, hwaddr, _, _)| reload_profiles(name, hwaddr));

        let list = list_state(&device.name).read().clone();
        let profiles = match &list {
            Some(Ok(profiles)) => profiles.clone(),
            _ => Vec::new(),
        };
        let mut profile_rows: Vec<Element> = profiles
            .iter()
            .map(|profile| profile_row(ctx, &device, profile))
            .collect();
        if let Some(Err(error)) = &list {
            profile_rows.push(Element::from(status(Status::Error(error.0.clone()))));
        }
        let taken: Vec<String> = profiles
            .iter()
            .map(|profile| profile.name.clone())
            .collect();
        {
            let device = device.clone();
            profile_rows.push(Element::from(
                row("Add profile…")
                    .leading(Icon::ListAdd)
                    .on_press(move |_| {
                        open_new_profile(ctx, &device, taken.clone(), new_name, new_error)
                    }),
            ));
        }

        let mut blocks = vec![
            hero(
                tokens,
                Icon::NetworkWired,
                device_title(&device),
                link_state(&device),
            ),
            Element::from(action_strip(vec![link_action(ctx, &device)])),
            status_group(&device),
            Element::from(Group::titled("Profiles").rows(profile_rows)),
        ];
        if device.has_connection() {
            let details = Route::Details {
                connection: device.connection.clone(),
            };
            let usage = Route::Usage {
                scope: UsageScope::Connection(device.connection.clone()),
            };
            blocks.push(Element::from(
                Group::new(None::<String>)
                    .row(
                        row("Connection details")
                            .navigates()
                            .on_press(move |_| ctx.navigator.push(details.clone())),
                    )
                    .row(
                        row("Data usage")
                            .navigates()
                            .on_press(move |_| ctx.navigator.push(usage.clone())),
                    ),
            ));
        }
        veiled(page_column().children(blocks), loading).into()
    }
}

/// One profile of the device: marked while it runs, opens its settings, and has the actions of a menu.
fn profile_row(ctx: NetCtx, device: &Device, profile: &WiredProfile) -> Element {
    let route = Route::Profile {
        device: device.name.clone(),
        name: profile.name.clone(),
    };
    let running = profile.active && device.connected();
    let usable = !unplugged(device);
    let connect = {
        let (device, uuid) = (device.clone(), profile.uuid.clone());
        move |_: ()| use_profile_now(ctx, &device, "uuid", uuid.clone())
    };
    let menu = {
        let mut items = Vec::new();
        if !running {
            items.push(
                menu_item("Use this profile", {
                    let connect = connect.clone();
                    move |_| connect(())
                })
                .icon(Icon::NetworkWired),
            );
        }
        let open = route.clone();
        items.push(
            menu_item("Edit", move |_| ctx.navigator.push(open.clone()))
                .icon(Icon::PreferencesSystem),
        );
        items.push(menu_separator());
        let (device, name, uuid) = (device.clone(), profile.name.clone(), profile.uuid.clone());
        items.push(
            menu_item("Delete", move |_| {
                confirm_delete(ctx, device.clone(), name.clone(), Some(uuid.clone()), false)
            })
            .icon(Icon::UserTrash)
            .destructive(),
        );
        items
    };
    let mut line = row(profile.name.clone())
        .leading(Icon::NetworkWired)
        .subtitle(if running { "In use" } else { "" })
        .selected(running)
        .navigates()
        .on_press(move |_| ctx.navigator.push(route.clone()))
        .context_menu(menu);
    if !running && usable {
        line = line.trailing(row_button("Connect", connect));
    }
    line.into()
}

/// The small popup that asks for the name of a new profile.
fn open_new_profile(
    ctx: NetCtx,
    device: &Device,
    taken: Vec<String>,
    name: State<String>,
    error: State<Option<String>>,
) {
    let (mut name, mut error) = (name, error);
    name.set(String::new());
    error.set(None);
    ctx.window.open(
        popup(
            "New profile",
            NewProfileBody {
                ctx,
                device: device.clone(),
                taken,
                name,
                error,
            },
        )
        .flags(PopupFlags::FORM),
    );
}

#[derive(Clone, PartialEq)]
struct NewProfileBody {
    ctx: NetCtx,
    device: Device,
    taken: Vec<String>,
    name: State<String>,
    error: State<Option<String>>,
}

impl NewProfileBody {
    /// Create the profile and open its settings, or say why not in the field.
    fn add(&self, window: Window) {
        let name = self.name.read().trim().to_string();
        let mut error = self.error;
        if name.is_empty() {
            return;
        }
        if self.taken.contains(&name) {
            error.set(Some("A profile with this name already exists.".to_string()));
            return;
        }
        window.close();
        let ctx = self.ctx;
        let device = self.device.clone();
        let route = Route::Profile {
            device: device.name.clone(),
            name: name.clone(),
        };
        let interface = device.name.clone();
        act(
            ctx,
            &device,
            move || {
                // Off by default: the profile is set up first, then used.
                nmcli(
                    &[
                        "connection",
                        "add",
                        "type",
                        "ethernet",
                        "ifname",
                        &interface,
                        "con-name",
                        &name,
                        "autoconnect",
                        "no",
                    ],
                    CONNECT,
                )
            },
            move || ctx.navigator.push(route),
        );
    }
}

impl Component for NewProfileBody {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let press = self.clone();
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Add".into(),
                enabled: !self.name.read().trim().is_empty(),
                on_press: EventHandler::new(move |_: ()| press.add(window)),
            }),
        });
        let submit = self.clone();
        popup_column().child(
            Group::new(None::<String>).row(
                form_field("Name", self.name)
                    .placeholder("Office")
                    .error(self.error.read().clone())
                    .validate(name_check)
                    .autofocus()
                    .on_submit(move |_: ()| submit.add(window)),
            ),
        )
    }
}

fn name_check(text: &str) -> Check {
    if text.trim().is_empty() {
        Err("Enter a name.".to_string())
    } else {
        Ok(())
    }
}

/// One wired profile: whether it runs, what you can do with it, and its settings.
#[derive(PartialEq)]
pub struct ProfilePage {
    pub device: String,
    pub name: String,
}

impl Component for ProfilePage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let tokens = use_tokens();
        use_pop_when_gone(ctx, &self.device);
        let (device, loading) = match find_device(ctx, &self.device) {
            Found::Loading(device) => (device, true),
            Found::Device(device) => (device, false),
            Found::Missing => return page_column().into(),
            Found::Failed(error) => return failure_page(error, move || ctx.reload()),
        };
        let running = device.connection == self.name && device.connected();
        let state = if running {
            link_state(&device)
        } else if unplugged(&device) {
            "Cable unplugged".to_string()
        } else {
            "Not in use".to_string()
        };

        let name = self.name.clone();
        let usable = !unplugged(&device);
        let use_action = {
            let (device, name) = (device.clone(), name.clone());
            strip_action(Icon::NetworkWired, "Use this profile", move |_| {
                use_profile_now(ctx, &device, "id", name.clone())
            })
            .enabled(!running && usable)
        };
        let disconnect = {
            let (device, name) = (device.clone(), name.clone());
            strip_action(Icon::WindowClose, "Disconnect", move |_| {
                act(
                    ctx,
                    &device,
                    {
                        let name = name.clone();
                        move || nmcli(&["connection", "down", "id", &name], CONNECT)
                    },
                    || {},
                )
            })
            .enabled(running)
        };
        let delete = {
            let (device, name) = (device.clone(), name.clone());
            strip_action(Icon::UserTrash, "Delete", move |_| {
                confirm_delete(ctx, device.clone(), name.clone(), None, true)
            })
        };
        let owner = Owner::Wired {
            device: self.device.clone(),
            profile: self.name.clone(),
        };
        veiled(
            page_column()
                .child(hero(tokens, Icon::NetworkWired, self.name.clone(), state))
                .child(action_strip(vec![use_action, disconnect, delete]))
                .child(ProfileSettings { owner }),
            loading,
        )
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(state: &str, carrier: bool) -> Device {
        Device {
            state: state.to_string(),
            carrier,
            speed: Some(1000),
            ipv4: vec!["192.168.0.72/24".to_string()],
            ..placeholder_device("enp42s0")
        }
    }

    #[test]
    fn link_line_says_what_the_cable_and_link_do() {
        assert_eq!(
            link_line(&device("connected", true), None),
            "Connected · 1 Gbit/s"
        );
        assert_eq!(
            link_line(&device("unavailable", false), None),
            "Cable unplugged"
        );
        assert_eq!(
            link_line(&device("unavailable", true), None),
            "Cable unplugged"
        );
        assert_eq!(
            link_line(&device("disconnected", true), None),
            "Not connected"
        );
        assert_eq!(
            link_line(&device("connecting (configuring)", true), None),
            "Connecting"
        );
        assert_eq!(
            link_line(&device("disconnected", true), Some(true)),
            "Connecting"
        );
        assert_eq!(
            link_line(&device("connected", true), Some(false)),
            "Disconnecting"
        );
    }

    #[test]
    fn row_line_adds_the_address_only_while_connected() {
        let up = device("connected", true);
        assert_eq!(
            with_address(&up, link_line(&up, None)),
            "Connected · 1 Gbit/s · 192.168.0.72"
        );
        let down = device("disconnected", true);
        assert_eq!(with_address(&down, link_line(&down, None)), "Not connected");
    }

    #[test]
    fn lists_only_wired_profiles() {
        let output = "Home:aaaa:802-3-ethernet:enp42s0:yes
Office:bbbb:802-11-wireless::no
Spare:cccc:802-3-ethernet::no
";
        let wired = parse_wired(output);
        assert_eq!(wired.len(), 2);
        assert_eq!(
            wired[0],
            ("Home".into(), "aaaa".into(), "enp42s0".into(), true)
        );
        assert_eq!(wired[1].0, "Spare");
    }

    #[test]
    fn a_profile_fits_unbound_or_matching_devices_only() {
        assert!(fits("", "", "", "enp42s0", "AA:BB:CC:DD:EE:FF"));
        assert!(fits(
            "enp42s0",
            "",
            "enp42s0",
            "enp42s0",
            "AA:BB:CC:DD:EE:FF"
        ));
        assert!(!fits("enp2s0f0u1", "", "", "enp42s0", "AA:BB:CC:DD:EE:FF"));
        assert!(fits(
            "",
            "aa:bb:cc:dd:ee:ff",
            "",
            "enp42s0",
            "AA:BB:CC:DD:EE:FF"
        ));
        assert!(!fits(
            "",
            "11:22:33:44:55:66",
            "",
            "enp42s0",
            "AA:BB:CC:DD:EE:FF"
        ));
        assert!(!fits("", "", "enp2s0f0u1", "enp42s0", "AA:BB:CC:DD:EE:FF"));
    }
}
