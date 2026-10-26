//! Reading and writing one connection's profile through `nmcli`: the shared loaded state of a profile,
//! and the two ways a screen writes it, at once (a switch or dropdown) or together (a form's Apply).

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use freya::prelude::*;

use crate::pages::connection_editor::{Changes, ProfileData, read_args};
use crate::pages::network::model::{CONNECT, NetCtx, Overview, nmcli};
use crate::pages::network::{Device, DeviceKind, Owner};
use crate::system::{CommandError, background_then};

const QUICK: Duration = Duration::from_secs(5);
const APPLY: Duration = Duration::from_secs(30);

/// A profile as last read: `None` before the first read of this connection finishes.
pub type ProfileState = State<Option<Result<ProfileData, CommandError>>>;

thread_local! {
    /// One state per connection name, alive for the whole window, so a screen opens with the values
    /// the last one read and a read that finishes after its screen is gone has a state to land in.
    static PROFILES: RefCell<HashMap<String, ProfileState>> = RefCell::new(HashMap::new());
}

fn profile_state(name: &str) -> ProfileState {
    PROFILES.with(|profiles| {
        *profiles
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| State::create_global(None))
    })
}

/// Read one profile, secrets included.
fn load(name: &str) -> Result<ProfileData, CommandError> {
    let args = read_args(name);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = nmcli(&refs, QUICK)?;
    Ok(ProfileData::parse(&output))
}

/// Read `name` again and keep the result in its state.
pub fn reload(name: &str) {
    let state = profile_state(name);
    let name = name.to_string();
    background_then(
        move || load(&name),
        move |result| {
            let mut state = state;
            state.set(Some(result));
        },
    );
}

/// The shared state of the owner's profile: the last value at once, read again whenever the screen
/// opens. Call from `render`.
pub fn use_profile(owner: &Owner) -> ProfileState {
    let name = owner.connection_name().to_string();
    let state = profile_state(&name);
    use_side_effect_with_deps(&name, |name| reload(name));
    state
}

/// The device the owner's connection is running on, from the overview, when it is active.
fn active_device<'a>(ov: &'a Overview, owner: &Owner) -> Option<&'a Device> {
    match owner {
        Owner::Wifi { ssid } => ov.devices.iter().find(|device| {
            device.kind == DeviceKind::Wifi && device.connection == *ssid && device.connected()
        }),
        Owner::Wired { profile, .. } => ov.devices.iter().find(|device| {
            device.kind == DeviceKind::Ethernet
                && device.connection == *profile
                && device.connected()
        }),
        Owner::Vpn { name } => {
            let vpn = ov.vpn(name).filter(|vpn| vpn.active)?;
            ov.devices.iter().find(|device| device.name == vpn.device)
        }
    }
}

/// The device the owner's connection is active on right now, as the overview last read it.
pub fn device_of(ctx: NetCtx, owner: &Owner) -> Option<Device> {
    let loaded = ctx.overview.read();
    let ov = loaded.as_ref()?.as_ref().ok()?;
    active_device(ov, owner).cloned()
}

/// Run `changes` on `name`, then push them into the running connection. `Ok(true)` means the profile
/// is saved but NetworkManager would not apply it to the live connection: only a reconnect does.
fn write(name: &str, device: Option<&str>, changes: &Changes) -> Result<bool, CommandError> {
    for command in changes.commands(name) {
        let refs: Vec<&str> = command.iter().map(String::as_str).collect();
        nmcli(&refs, APPLY)?;
    }
    let Some(device) = device.filter(|_| changes.affects_link()) else {
        return Ok(false);
    };
    Ok(nmcli(&["device", "reapply", device], APPLY).is_err())
}

/// Tell the person that a saved change waits for a reconnect, and offer it.
fn offer_reconnect(ctx: NetCtx, name: String) {
    ctx.overlays.toast_with_action(
        "Saved. Reconnect to apply the change.",
        "Reconnect",
        move |_| reconnect(ctx, name.clone()),
    );
}

/// Take the connection down and up again, so every saved value applies.
pub fn reconnect(ctx: NetCtx, name: String) {
    background_then(
        move || nmcli(&["connection", "up", "id", &name], CONNECT),
        move |result| {
            if let Err(error) = result {
                ctx.overlays.toast(error.0);
            }
            ctx.reload();
        },
    );
}

/// Write one switch or dropdown at once: the screen shows the new value before the command has run,
/// and goes back to the old one, with the error in a toast, if `nmcli` refuses.
pub fn apply_now(ctx: NetCtx, owner: &Owner, state: ProfileState, changes: Changes) {
    let Some(Ok(current)) = state.read().clone() else {
        return;
    };
    let mut shown = state;
    shown.set(Some(Ok(current.with(&changes))));
    let name = owner.connection_name().to_string();
    let device = device_of(ctx, owner).map(|device| device.name);
    background_then(
        move || write(&name, device.as_deref(), &changes).map(|reconnect| (name, reconnect)),
        move |result| match result {
            Ok((name, reconnect)) => {
                if reconnect {
                    offer_reconnect(ctx, name.clone());
                }
                self::reload(&name);
                ctx.reload();
            }
            Err(error) => {
                let mut shown = state;
                shown.set(Some(Ok(current)));
                ctx.overlays.toast(error.0);
            }
        },
    );
}

/// Set a state the screen may no longer own: a command finishing after the person left the page.
pub fn set_if_alive<T: 'static>(state: State<T>, value: T) {
    let mut state = state;
    if let Some(mut slot) = state.try_write() {
        *slot = value;
    }
}

/// Write a form's changes together. `busy` is true while the command runs. On success the profile
/// shows the new values at once and `on_saved` runs (a rename moves to the new name there); the
/// error of a refused change is shown verbatim in a toast and the form keeps what was typed.
pub fn apply_batch(
    ctx: NetCtx,
    owner: &Owner,
    state: ProfileState,
    busy: State<bool>,
    changes: Changes,
    on_saved: impl FnOnce() + 'static,
) {
    let Some(Ok(current)) = state.read().clone() else {
        return;
    };
    set_if_alive(busy, true);
    let name = owner.connection_name().to_string();
    let device = device_of(ctx, owner).map(|device| device.name);
    let saved = current.with(&changes);
    background_then(
        move || write(&name, device.as_deref(), &changes).map(|reconnect| (name, reconnect)),
        move |result| {
            set_if_alive(busy, false);
            match result {
                Ok((name, reconnect)) => {
                    let mut shown = profile_state(&name);
                    shown.set(Some(Ok(saved)));
                    if reconnect {
                        offer_reconnect(ctx, name.clone());
                    }
                    self::reload(&name);
                    ctx.reload();
                    on_saved();
                }
                Err(error) => ctx.overlays.toast(error.0),
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::network::model::{Vpn, VpnKind};

    fn device(name: &str, kind: DeviceKind, connection: &str, state: &str) -> Device {
        Device {
            name: name.to_string(),
            kind,
            state: state.to_string(),
            connection: connection.to_string(),
            uuid: String::new(),
            hwaddr: String::new(),
            speed: None,
            carrier: true,
            mtu: None,
            ipv4: Vec::new(),
            ipv6: Vec::new(),
            gateway4: String::new(),
            gateway6: String::new(),
            dns: Vec::new(),
        }
    }

    fn overview() -> Overview {
        Overview {
            state: "connected".to_string(),
            connectivity: "full".to_string(),
            wifi_hw: "enabled".to_string(),
            wifi_on: true,
            wwan_hw: "missing".to_string(),
            wwan_on: false,
            airplane: false,
            devices: vec![
                device("wlo1", DeviceKind::Wifi, "Home", "connected"),
                device("enp42s0", DeviceKind::Ethernet, "Office", "connected"),
                device("enp2s0f0u1", DeviceKind::Ethernet, "Spare", "disconnected"),
                device("wg0", DeviceKind::WireGuard, "Work", "connected"),
            ],
            vpns: vec![Vpn {
                name: "Work".to_string(),
                uuid: String::new(),
                kind: VpnKind::WireGuard,
                active: true,
                activating: false,
                device: "wg0".to_string(),
                autoconnect: false,
            }],
        }
    }

    #[test]
    fn finds_the_device_a_connection_runs_on() {
        let ov = overview();
        let name = |owner: &Owner| active_device(&ov, owner).map(|device| device.name.clone());
        assert_eq!(
            name(&Owner::Wifi {
                ssid: "Home".into()
            })
            .as_deref(),
            Some("wlo1")
        );
        assert_eq!(
            name(&Owner::Wired {
                device: "enp42s0".into(),
                profile: "Office".into()
            })
            .as_deref(),
            Some("enp42s0")
        );
        assert_eq!(
            name(&Owner::Vpn {
                name: "Work".into()
            })
            .as_deref(),
            Some("wg0")
        );
    }

    #[test]
    fn a_connection_that_is_not_running_has_no_device() {
        let ov = overview();
        assert!(
            active_device(
                &ov,
                &Owner::Wifi {
                    ssid: "Cafe".into()
                }
            )
            .is_none()
        );
        assert!(
            active_device(
                &ov,
                &Owner::Wired {
                    device: "enp2s0f0u1".into(),
                    profile: "Spare".into()
                }
            )
            .is_none()
        );
    }
}
