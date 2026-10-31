//! Wired Ethernet: the device's page, its profiles, and the pages below them.
//!
//! A wired device has one page, and it is the connection page of the profile the device uses (see
//! [`Owner::Wired`] with `in_use`). A device with more than one profile adds a Profile dropdown and
//! a Manage profiles row that opens the list. A profile that is not in use has the same page below
//! that list; the one in use opens the device's page instead.

use std::cell::RefCell;
use std::collections::HashMap;

use freya::prelude::*;

use super::model::{
    CONNECT, NetCtx, Overview, QUICK, capitalise, hero, nmcli, pending_for, split_terse,
    toggle_link, use_net_ctx,
};
use super::profile::{ProfileSettings, SectionPage, speed_text};
use super::usage::{UsagePage, connection_usage_row};
use super::{Device, Owner, Section, UsageScope, device_icon, failure_page};
use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::routes;
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    CANCEL_ID, Group, MenuItem, PageHead, PopupAction, PopupActions, PopupFlags, ResponseKind,
    Status, StripAction, Window, WindowLike, WindowParent, action_strip, choice_row, dialog,
    expander_row, form_field, menu_item, menu_separator, page_column, popup, popup_column, row,
    row_button, status, strip_action, switch, use_popup_actions, use_tokens, use_window,
};
use bingux_ui::controls::form::validate::Check;
use bingux_ui::prelude::Target;

/// The title of a wired device's row and page: its profile's name, or `Ethernet (enp42s0)`. A
/// profile name that two devices share gets the interface too, so no two rows read alike.
pub fn title_of(ov: &Overview, device: &Device) -> String {
    if !device.has_connection() {
        return format!("Ethernet ({})", device.name);
    }
    let shared = ov
        .ethernet_devices()
        .filter(|other| other.connection == device.connection)
        .count()
        > 1;
    if shared {
        format!("{} ({})", device.connection, device.name)
    } else {
        device.connection.clone()
    }
}

/// The header title of the wired device `device` from the overview loaded at
/// start-up, so a page opened by address shows the title its row shows.
pub fn cached_ethernet_title(device: &str) -> Option<String> {
    let model = system::cached_persistent::<Overview, system::CommandError>("network-overview");
    let loaded = model.read();
    let ov = loaded.as_ref()?.as_ref().ok()?;
    let found = ov.ethernet_devices().find(|known| known.name == device)?;
    Some(title_of(ov, found))
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

/// A wired device row of the Connections group.
fn device_row(ctx: NetCtx, ov: &Overview, device: &Device) -> Element {
    let title = title_of(ov, device);
    let route = routes::ethernet(&device.name.clone(), &title.clone());
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
            .on_press(move |_| ctx.router.open(open.clone()))
            .context_menu(device_menu(ctx, device, route)),
    )
}

/// The rows of the Connections group for wired devices: one per device that is plugged in or has
/// a profile, and one muted row that holds the unplugged adapters nothing is set up on.
pub fn ethernet_rows(ctx: NetCtx, ov: &Overview) -> Vec<Element> {
    // Whether a device has a profile is read once for each device that could be spare.
    let candidates: Vec<(String, String)> = ov
        .ethernet_devices()
        .filter(|device| unplugged(device) && !device.has_connection())
        .map(|device| (device.name.clone(), device.hwaddr.clone()))
        .collect();
    use_side_effect_with_deps(&candidates, |candidates| {
        for (name, hwaddr) in candidates {
            reload_profiles(name, hwaddr);
        }
    });
    let mut shown = Vec::new();
    let mut spare: Vec<&Device> = Vec::new();
    for device in ov.ethernet_devices() {
        let unused = unplugged(device)
            && !device.has_connection()
            && matches!(&*list_state(&device.name).read(), Some(Ok(list)) if list.is_empty());
        if unused {
            spare.push(device);
        } else {
            shown.push(device_row(ctx, ov, device));
        }
    }
    if !spare.is_empty() {
        let plural = if spare.len() == 1 { "" } else { "s" };
        let children: Vec<Element> = spare
            .iter()
            .map(|device| {
                let route = routes::ethernet(&device.name.clone(), &title_of(ov, device));
                Element::from(
                    row(title_of(ov, device))
                        .leading(device_icon(device.kind))
                        .subtitle("Cable unplugged")
                        .navigates()
                        .on_press(move |_| ctx.router.open(route.clone())),
                )
            })
            .collect();
        shown.push(Element::from(
            expander_row(
                format!("{} other wired adapter{plural}", spare.len()),
                children,
            )
            .subtitle("Cable unplugged"),
        ));
    }
    shown
}

/// The right-click menu of a wired device: connect or disconnect, and its settings.
fn device_menu(ctx: NetCtx, device: &Device, route: Target) -> Vec<MenuItem> {
    let name = device.name.clone();
    let connected = shown_connected(device);
    let toggle = if connected {
        menu_item("Disconnect", move |_| set_link(ctx, name.clone(), false)).icon(Icon::WindowClose)
    } else {
        menu_item("Connect", move |_| set_link(ctx, name.clone(), true)).icon(Icon::NetworkWired)
    };
    vec![
        toggle,
        menu_item("Settings", move |_| ctx.router.open(route.clone()))
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

/// Snapshot builds only: the profiles of the fake wired devices. `enp42s0` has the profile the
/// fake network runs; `BINGUX_SETTINGS_FAKE_WIRED=many` gives it a second one and the fake network
/// two unplugged adapters, which have no profiles.
#[cfg(feature = "snapshot")]
fn fake_profiles(device: &str) -> Option<Vec<WiredProfile>> {
    let scenario = super::model::fake_scenario()?;
    let up = !matches!(scenario.as_str(), "offline" | "airplane" | "wifi-only");
    let mut profiles = Vec::new();
    if device == "enp42s0" {
        profiles.push(WiredProfile {
            name: bingux_ui::stress::data("Wired connection 1"),
            uuid: "fake-wired-1".to_string(),
            active: up,
        });
        if std::env::var("BINGUX_SETTINGS_FAKE_WIRED").as_deref() == Ok("many") {
            profiles.push(WiredProfile {
                name: bingux_ui::stress::data("Office desk"),
                uuid: "fake-wired-2".to_string(),
                active: false,
            });
        }
    }
    Some(profiles)
}

/// Every wired profile that can run on `device`, by name.
fn load_profiles(device: &str, hwaddr: &str) -> Result<Vec<WiredProfile>, CommandError> {
    #[cfg(feature = "snapshot")]
    if let Some(mut profiles) = fake_profiles(device) {
        profiles.sort_by_key(|profile| profile.name.to_lowercase());
        return Ok(profiles);
    }
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

/// The profile a device uses, which is the page of the device: the one it runs, else the one the
/// list marks as running, else the first.
fn in_use_profile<'a>(device: &Device, profiles: &'a [WiredProfile]) -> Option<&'a WiredProfile> {
    profiles
        .iter()
        .find(|profile| device.connected() && profile.name == device.connection)
        .or_else(|| profiles.iter().find(|profile| profile.active))
        .or_else(|| profiles.first())
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
            ctx.router.pop();
        }
    });
}

/// What every page of one wired device reads.
struct Wired {
    device: Device,
    /// The overview has not been read yet, so `device` is a blank.
    loading: bool,
    /// The profiles that can run on the device, once read.
    list: Option<Result<Vec<WiredProfile>, CommandError>>,
    /// The title the device's row has.
    title: String,
}

/// The device and its profiles, read again whenever what the device runs changes. `Err` is a
/// finished page for a device that is gone or an overview that failed. Call from `render`, before
/// anything that returns.
fn use_wired(ctx: NetCtx, name: &str) -> Result<Wired, Element> {
    use_pop_when_gone(ctx, name);
    let found = find_device(ctx, name);
    let (device, loading) = match &found {
        Found::Loading(device) => (Some(device.clone()), true),
        Found::Device(device) => (Some(device.clone()), false),
        _ => (None, false),
    };
    let deps = (
        name.to_string(),
        device.as_ref().map(|device| device.hwaddr.clone()),
        device.as_ref().map(|device| device.connection.clone()),
        device.as_ref().map(|device| device.state.clone()),
        loading,
    );
    use_side_effect_with_deps(&deps, |(name, hwaddr, _, _, loading)| {
        if let (Some(hwaddr), false) = (hwaddr, loading) {
            reload_profiles(name, hwaddr);
        }
    });
    let list = list_state(name).read().clone();
    match (found, device) {
        (Found::Failed(error), _) => Err(failure_page(error, move || ctx.reload())),
        (_, Some(device)) => {
            let title = match &*ctx.overview.read() {
                Some(Ok(ov)) => title_of(ov, &device),
                _ => device.name.clone(),
            };
            Ok(Wired {
                device,
                loading,
                list,
                title,
            })
        }
        _ => Err(page_column().into()),
    }
}

/// A page drawn blank under the veil while the device's profiles are read.
fn loading_page(tokens: crate::ui::Tokens, wired: &Wired) -> Element {
    veiled(
        page_column()
            .child(hero(
                tokens,
                Icon::NetworkWired,
                "—".to_string(),
                link_state(&wired.device),
            ))
            .child(
                Group::new(None::<String>)
                    .row(row("IP address").mono_value("—"))
                    .row(row("Speed").value("—")),
            ),
        true,
    )
    .into()
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
        .default_action(CANCEL_ID)
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
                        ctx.router.pop();
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

/// The facts of the link on the device's page: the address and the speed, then this
/// connection's data usage today.
fn facts_group(ctx: NetCtx, device: &Device, owner: Owner) -> Group {
    let mut group = Group::new(None::<String>);
    if device.connected() && !unplugged(device) {
        let address = device.address4();
        if !address.is_empty() {
            group = group.row(row("IP address").mono_value(address));
        }
        if let Some(speed) = device.speed {
            group = group.row(row("Speed").value(speed_text(speed)));
        }
    }
    let connection = owner.connection_name().to_string();
    let open = routes::section(&owner, &Section::Usage);
    group.row(connection_usage_row(ctx, connection, open))
}

/// The wired device's page: the connection page of the profile the device uses.
#[derive(PartialEq)]
pub struct EthernetPage {
    pub device: String,
}

impl Component for EthernetPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let tokens = use_tokens();
        let wired = match use_wired(ctx, &self.device) {
            Ok(wired) => wired,
            Err(page) => return page,
        };
        let device = wired.device.clone();
        let profiles = match &wired.list {
            None => return loading_page(tokens, &wired),
            Some(Err(error)) => {
                let error = error.clone();
                let (name, hwaddr) = (device.name.clone(), device.hwaddr.clone());
                return failure_page(error, move || reload_profiles(&name, &hwaddr));
            }
            Some(Ok(profiles)) => profiles.clone(),
        };
        let Some(current) = in_use_profile(&device, &profiles).cloned() else {
            return unconfigured_page(ctx, tokens, &wired);
        };
        let owner = Owner::Wired {
            device: device.name.clone(),
            profile: current.name.clone(),
            in_use: true,
        };
        let usable = !unplugged(&device);

        let link = {
            let name = device.name.clone();
            if shown_connected(&device) {
                strip_action(Icon::WindowClose, "Disconnect", move |_| {
                    set_link(ctx, name.clone(), false)
                })
            } else {
                let (device, uuid) = (device.clone(), current.uuid.clone());
                strip_action(Icon::NetworkWired, "Connect", move |_| {
                    use_profile_now(ctx, &device, "uuid", uuid.clone())
                })
                .enabled(usable)
            }
        };
        let mut actions: Vec<StripAction> = vec![link];
        if profiles.len() > 1 {
            let (device, name, uuid) = (device.clone(), current.name.clone(), current.uuid.clone());
            actions.push(strip_action(Icon::UserTrash, "Delete", move |_| {
                confirm_delete(ctx, device.clone(), name.clone(), Some(uuid.clone()), false)
            }));
        }

        let mut blocks = vec![
            hero(
                tokens,
                Icon::NetworkWired,
                current.name.clone(),
                link_state(&device),
            ),
            Element::from(action_strip(actions)),
        ];
        if profiles.len() > 1 {
            blocks.push(profile_group(ctx, &device, &profiles, &current));
        }
        blocks.push(Element::from(facts_group(ctx, &device, owner.clone())));
        veiled(
            page_column()
                .children(blocks)
                .child(ProfileSettings { owner }),
            wired.loading,
        )
        .into()
    }
}

/// The Profile dropdown that chooses what the device runs, and the row that opens the list.
fn profile_group(
    ctx: NetCtx,
    device: &Device,
    profiles: &[WiredProfile],
    current: &WiredProfile,
) -> Element {
    let options: Vec<(String, String)> = profiles
        .iter()
        .map(|profile| (profile.name.clone(), profile.name.clone()))
        .collect();
    let choose = {
        let (device, profiles) = (device.clone(), profiles.to_vec());
        move |name: String| {
            if let Some(profile) = profiles.iter().find(|profile| profile.name == name) {
                use_profile_now(ctx, &device, "uuid", profile.uuid.clone());
            }
        }
    };
    let list = routes::ethernet_profiles(&device.name.clone());
    Element::from(
        Group::new(None::<String>)
            .row(choice_row("Profile", options, &current.name, choose).enabled(!unplugged(device)))
            .row(
                row("Manage profiles")
                    .navigates()
                    .on_press(move |_| ctx.router.open(list.clone())),
            ),
    )
}

/// The page of a device nothing is set up on: its state, and the way to set it up.
fn unconfigured_page(ctx: NetCtx, tokens: crate::ui::Tokens, wired: &Wired) -> Element {
    let device = wired.device.clone();
    let connect = {
        let name = device.name.clone();
        strip_action(Icon::NetworkWired, "Connect", move |_| {
            set_link(ctx, name.clone(), true)
        })
        .enabled(!unplugged(&device) && !shown_connected(&device))
    };
    let mut facts = Group::new(None::<String>);
    if device.connected() {
        let address = device.address4();
        if !address.is_empty() {
            facts = facts.row(row("IP address").mono_value(address));
        }
        if let Some(speed) = device.speed {
            facts = facts.row(row("Speed").value(speed_text(speed)));
        }
    }
    let mut page = page_column()
        .child(hero(
            tokens,
            Icon::NetworkWired,
            wired.title.clone(),
            link_state(&device),
        ))
        .child(action_strip(vec![connect]));
    if device.connected() {
        page = page.child(facts);
    }
    page.child(Group::new(None::<String>).row(Element::from(AddProfileRow {
        device: device.name.clone(),
    })))
    .into()
}

/// One profile of the device in the list: marked while it runs, opens its page, and has the
/// actions of a menu. The one in use is the device's page, which this list is below, so it only
/// says so.
fn profile_row(ctx: NetCtx, device: &Device, profile: &WiredProfile, in_use: bool) -> Element {
    let route = routes::profile(&device.name.clone(), &profile.name.clone());
    let usable = !unplugged(device);
    let connect = {
        let (device, uuid) = (device.clone(), profile.uuid.clone());
        move |_: ()| use_profile_now(ctx, &device, "uuid", uuid.clone())
    };
    let menu = {
        let mut items = Vec::new();
        if !in_use {
            items.push(
                menu_item("Use this profile", {
                    let connect = connect.clone();
                    move |_| connect(())
                })
                .icon(Icon::NetworkWired),
            );
            let open = route.clone();
            items.push(
                menu_item("Open", move |_| ctx.router.open(open.clone()))
                    .icon(Icon::PreferencesSystem),
            );
            items.push(menu_separator());
        }
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
        .selected(in_use)
        .context_menu(menu);
    if in_use {
        line = line.subtitle("In use");
    } else {
        line = line
            .navigates()
            .on_press(move |_| ctx.router.open(route.clone()));
        if usable {
            line = line.trailing(row_button("Use", connect));
        }
    }
    line.into()
}

/// The profiles of a device that has more than one: which one is in use, and the way to the page of
/// each other one.
#[derive(PartialEq)]
pub struct ProfilesPage {
    pub device: String,
}

impl Component for ProfilesPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let wired = match use_wired(ctx, &self.device) {
            Ok(wired) => wired,
            Err(page) => return page,
        };
        let device = wired.device.clone();
        let head = PageHead::new().subtitle(wired.title.clone());
        let profiles = match &wired.list {
            None => {
                return veiled(page_column().child(head), true).into();
            }
            Some(Err(error)) => {
                return page_column()
                    .child(head)
                    .child(status(Status::Error(error.0.clone())))
                    .into();
            }
            Some(Ok(profiles)) => profiles.clone(),
        };
        let current = in_use_profile(&device, &profiles).map(|profile| profile.name.clone());
        let rows: Vec<Element> = profiles
            .iter()
            .map(|profile| {
                profile_row(
                    ctx,
                    &device,
                    profile,
                    current.as_deref() == Some(profile.name.as_str()),
                )
            })
            .collect();
        veiled(
            page_column()
                .child(head)
                .child(Group::new(None::<String>).rows(rows))
                .child(Group::new(None::<String>).row(Element::from(AddProfileRow {
                    device: device.name.clone(),
                }))),
            wired.loading,
        )
        .into()
    }
}

/// The Add profile row. It asks for a name in a small popup, creates the profile and opens it.
#[derive(PartialEq)]
struct AddProfileRow {
    device: String,
}

impl Component for AddProfileRow {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let new_name = use_state(String::new);
        let new_error = use_state(|| None::<String>);
        let found = find_device(ctx, &self.device);
        let taken: Vec<String> = match list_state(&self.device).read().clone() {
            Some(Ok(profiles)) => profiles
                .iter()
                .map(|profile| profile.name.clone())
                .collect(),
            _ => Vec::new(),
        };
        let device = match found {
            Found::Device(device) | Found::Loading(device) => device,
            _ => placeholder_device(&self.device),
        };
        row("Add profile…")
            .leading(Icon::ListAdd)
            .on_press(move |_| open_new_profile(ctx, &device, taken.clone(), new_name, new_error))
    }
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
    /// Create the profile and open its page, or say why not in the field.
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
        let route = routes::profile(&device.name.clone(), &name.clone());
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
            move || ctx.router.open(route),
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

/// One wired profile that is not the one in use: its state, what you can do with it and its
/// settings. The profile in use is the device's page, which this opens instead.
#[derive(PartialEq)]
pub struct ProfilePage {
    pub device: String,
    pub name: String,
}

impl Component for ProfilePage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let tokens = use_tokens();
        let wired = match use_wired(ctx, &self.device) {
            Ok(wired) => wired,
            Err(page) => return page,
        };
        let device = wired.device.clone();
        let profiles = match &wired.list {
            Some(Ok(profiles)) => Some(profiles.clone()),
            _ => None,
        };
        let name = self.name.clone();
        let current = profiles
            .as_deref()
            .and_then(|profiles| in_use_profile(&device, profiles))
            .map(|profile| profile.name.clone());
        let uuid = profiles
            .as_deref()
            .and_then(|profiles| profiles.iter().find(|profile| profile.name == name))
            .map(|profile| profile.uuid.clone());
        // The profile in use is the device's page.
        let redirect = (!wired.loading && current.as_deref() == Some(name.as_str()))
            .then(|| device.name.clone());
        use_side_effect_with_deps(&redirect, move |redirect| {
            if let Some(device) = redirect {
                ctx.router
                    .open(routes::ethernet(&device.clone(), &device.clone()));
            }
        });
        if redirect.is_some() || profiles.is_none() {
            return loading_page(tokens, &wired);
        }

        let usable = !unplugged(&device);
        let use_action = {
            let (device, name) = (device.clone(), name.clone());
            strip_action(Icon::NetworkWired, "Use this profile", move |_| {
                use_profile_now(ctx, &device, "id", name.clone())
            })
            .enabled(usable)
        };
        let delete = {
            let (device, name, uuid) = (device.clone(), name.clone(), uuid.clone());
            strip_action(Icon::UserTrash, "Delete", move |_| {
                confirm_delete(ctx, device.clone(), name.clone(), uuid.clone(), true)
            })
        };
        let owner = Owner::Wired {
            device: self.device.clone(),
            profile: name.clone(),
            in_use: false,
        };
        let state = if usable {
            "Not in use".to_string()
        } else {
            "Cable unplugged".to_string()
        };
        let open = routes::section(&owner.clone(), &Section::Usage);
        veiled(
            page_column()
                .child(hero(tokens, Icon::NetworkWired, name.clone(), state))
                .child(action_strip(vec![use_action, delete]))
                .child(Group::new(None::<String>).row(connection_usage_row(ctx, name, open)))
                .child(ProfileSettings { owner }),
            wired.loading,
        )
        .into()
    }
}

/// A page below a wired connection, which may have to work out which profile it is first: the
/// device's own pages name no profile, and a profile's pages move to the device's when the profile
/// turns out to be the one in use.
#[derive(PartialEq)]
pub struct WiredSection {
    pub owner: Owner,
    pub section: Section,
}

impl Component for WiredSection {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let tokens = use_tokens();
        let Owner::Wired {
            device,
            profile,
            in_use,
        } = self.owner.clone()
        else {
            return page_column().into();
        };
        let wired = match use_wired(ctx, &device) {
            Ok(wired) => wired,
            Err(page) => return page,
        };
        let list = match &wired.list {
            Some(Ok(list)) => Some(list.clone()),
            _ => None,
        };
        let current = list
            .as_deref()
            .and_then(|list| in_use_profile(&wired.device, list))
            .map(|profile| profile.name.clone());
        let section = self.section.clone();
        let moves = (!in_use && !wired.loading && current.as_deref() == Some(profile.as_str()))
            .then(|| (device.clone(), section.clone()));
        use_side_effect_with_deps(&moves, move |moves| {
            if let Some((device, section)) = moves {
                ctx.router.open(routes::section(
                    &Owner::Wired {
                        device: device.clone(),
                        profile: String::new(),
                        in_use: true,
                    },
                    &section.clone(),
                ));
            }
        });
        let resolved = if in_use { current } else { Some(profile) };
        let (true, Some(name), false) = (list.is_some(), resolved, moves.is_some()) else {
            return loading_page(tokens, &wired);
        };
        let owner = Owner::Wired {
            device: device.clone(),
            profile: name.clone(),
            in_use,
        };
        if section == Section::Usage {
            return UsagePage {
                scope: UsageScope::Connection(name),
            }
            .into();
        }
        let extra = (section == Section::Advanced).then(|| {
            Element::from(Group::titled("Profiles").row(Element::from(AddProfileRow { device })))
        });
        SectionPage {
            owner,
            section,
            extra,
        }
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

    fn profile(name: &str, active: bool) -> WiredProfile {
        WiredProfile {
            name: name.to_string(),
            uuid: format!("uuid-{name}"),
            active,
        }
    }

    #[test]
    fn the_device_uses_the_profile_it_runs_else_the_first() {
        let profiles = vec![profile("Home", false), profile("Office", true)];
        let mut running = device("connected", true);
        running.connection = "Home".to_string();
        assert_eq!(in_use_profile(&running, &profiles).unwrap().name, "Home");
        let mut idle = device("disconnected", true);
        idle.connection = String::new();
        assert_eq!(in_use_profile(&idle, &profiles).unwrap().name, "Office");
        let none = vec![profile("Home", false), profile("Spare", false)];
        assert_eq!(in_use_profile(&idle, &none).unwrap().name, "Home");
        assert!(in_use_profile(&idle, &[]).is_none());
    }

    fn overview_of(devices: Vec<Device>) -> Overview {
        Overview {
            state: "connected".to_string(),
            connectivity: "full".to_string(),
            wifi_hw: "missing".to_string(),
            wifi_on: false,
            wwan_hw: "missing".to_string(),
            wwan_on: false,
            airplane: false,
            devices,
            vpns: Vec::new(),
        }
    }

    #[test]
    fn every_wired_device_has_its_own_name() {
        let named = |name: &str, connection: &str| Device {
            name: name.to_string(),
            connection: connection.to_string(),
            ..placeholder_device(name)
        };
        let ov = overview_of(vec![
            named("enp1s0", "Wired connection 1"),
            named("enp2s0", "Wired connection 1"),
            named("enp3s0", "Office"),
            named("enp4s0", ""),
        ]);
        let title = |index: usize| title_of(&ov, &ov.devices[index]);
        assert_eq!(title(0), "Wired connection 1 (enp1s0)");
        assert_eq!(title(1), "Wired connection 1 (enp2s0)");
        assert_eq!(title(2), "Office");
        assert_eq!(title(3), "Ethernet (enp4s0)");
    }
}
