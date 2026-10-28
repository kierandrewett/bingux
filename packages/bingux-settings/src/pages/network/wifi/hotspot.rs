//! The Wi-Fi hotspot screen: a name, a password and a band, then Turn on;
//! while it runs, its state, who is connected, a QR code to join it, and
//! Turn off. The form starts from the saved hotspot profile, so the password
//! a person chose last time is the one shown.

use std::time::Duration;

use freya::prelude::*;

use super::nm::{self, Saved};
use super::share;
use super::{Data, Radio, data, radio_of, use_polling, use_ui};
use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::pages::network::model::{self, QUICK, nmcli};
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{
    Group, PageHead, Status, action_button, action_row, action_strip, choice_row, form_field,
    page_column, row, status, strip_action, use_overlays, use_page_busy, use_window,
};
use bingux_ui::controls::form::validate;

const BAND_OPTIONS: [(&str, &str); 3] = [("auto", "Automatic"), ("bg", "2.4 GHz"), ("a", "5 GHz")];
const CLIENTS_EVERY: Duration = Duration::from_secs(5);
/// Letters and digits without the pairs that read alike (0 O, 1 l I).
const PASSWORD_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
const PASSWORD_LENGTH: usize = 12;

/// What the form starts with.
#[derive(Clone, PartialEq)]
struct Defaults {
    name: String,
    password: String,
    /// `auto`, `bg` or `a`.
    band: String,
}

/// The name for a new hotspot: the computer's own.
fn hostname() -> String {
    crate::system::read_to_string("/etc/hostname")
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Hotspot".to_string())
}

/// A fresh password from the system's random source: letters and digits that
/// do not read alike.
fn generate_password() -> String {
    use std::io::Read;
    let limit = 256 - 256 % PASSWORD_ALPHABET.len();
    let mut password = String::new();
    let Ok(mut random) = std::fs::File::open("/dev/urandom") else {
        return password;
    };
    let mut byte = [0u8; 1];
    while password.len() < PASSWORD_LENGTH {
        if random.read_exact(&mut byte).is_err() {
            return String::new();
        }
        // Throw away the bytes that would make some letters likelier.
        if usize::from(byte[0]) < limit {
            password
                .push(PASSWORD_ALPHABET[usize::from(byte[0]) % PASSWORD_ALPHABET.len()] as char);
        }
    }
    password
}

fn band_key(band: &str) -> String {
    match band {
        "bg" | "a" => band.to_string(),
        _ => "auto".to_string(),
    }
}

/// The saved hotspot profile's settings, or a new hotspot's.
fn read_defaults(saved: Option<Saved>) -> Defaults {
    let Some(saved) = saved else {
        return Defaults {
            name: hostname(),
            password: generate_password(),
            band: "auto".into(),
        };
    };
    // A profile that holds no password (a failed read) gets a fresh one, so
    // Turn on is never stuck on an empty field.
    let password = nm::saved_secret(&saved.uuid, &saved.key_mgmt)
        .ok()
        .flatten()
        .unwrap_or_else(generate_password);
    Defaults {
        name: saved.ssid,
        password,
        band: band_key(&saved.band),
    }
}

/// The hotspot profile the form belongs to: the running one, else the one
/// called `Hotspot`, else any saved access point profile.
fn profile_of(model: &nm::Model) -> Option<&Saved> {
    model.hotspot().or_else(|| {
        let mut aps = model.saved.iter().filter(|saved| saved.ap);
        aps.clone()
            .find(|saved| saved.name == "Hotspot")
            .or_else(|| aps.next())
    })
}

/// Turn the hotspot on with these settings. A saved hotspot profile is
/// rewritten and brought up, so there is one profile and no copies pile up;
/// without one, `nmcli` makes it.
fn start(
    device: &str,
    profile: Option<&str>,
    name: &str,
    password: &str,
    band: &str,
) -> Result<(), CommandError> {
    if nm::faked() {
        return Ok(());
    }
    let band = if band == "auto" { "" } else { band };
    match profile {
        Some(uuid) => {
            nmcli(
                &[
                    "connection",
                    "modify",
                    "uuid",
                    uuid,
                    "802-11-wireless.ssid",
                    name,
                    "802-11-wireless-security.psk",
                    password,
                    "802-11-wireless.band",
                    band,
                ],
                QUICK,
            )?;
            let mut args = vec!["connection", "up", "uuid", uuid];
            if !device.is_empty() {
                args.extend(["ifname", device]);
            }
            nmcli(&args, model::CONNECT).map(|_| ())
        }
        None => {
            let mut args = vec!["device", "wifi", "hotspot"];
            if !device.is_empty() {
                args.extend(["ifname", device]);
            }
            args.extend(["ssid", name, "password", password]);
            if !band.is_empty() {
                args.extend(["band", band]);
            }
            nmcli(&args, model::CONNECT).map(|_| ())
        }
    }
}

/// How many devices are connected, from the neighbour table of the hotspot's
/// interface: an entry that answers (not `FAILED` or `INCOMPLETE`) is a device
/// that has talked to this computer lately.
pub fn parse_clients(output: &str) -> usize {
    output
        .lines()
        .filter(|line| line.contains("lladdr"))
        .filter(|line| {
            matches!(
                line.split_whitespace().last(),
                Some("REACHABLE" | "STALE" | "DELAY" | "PROBE" | "PERMANENT")
            )
        })
        .count()
}

/// Devices on the hotspot, `None` when they cannot be told (no interface).
fn read_clients(device: &str) -> Option<usize> {
    if device.is_empty() {
        return None;
    }
    if nm::faked() {
        return Some(2);
    }
    system::run("ip", &["neigh", "show", "dev", device], QUICK)
        .ok()
        .map(|output| parse_clients(&output))
}

/// The hotspot screen.
#[derive(PartialEq)]
pub struct HotspotPage;

impl Component for HotspotPage {
    fn render(&self) -> impl IntoElement {
        let ui = use_ui();
        // A scan on the hotspot's radio would drop its clients: read only.
        use_polling(ui, false);
        use_page_busy(ui.busy.read().is_some());
        let window = use_window();

        let mut name = use_state(String::new);
        let mut password = use_state(String::new);
        let mut band = use_state(|| "auto".to_string());
        let defaults = use_state(|| None::<Defaults>);

        let Data {
            ov,
            model,
            loading,
            error,
        } = data(ui);
        let radio = radio_of(&ov);
        let device = ov
            .wifi_device()
            .map(|device| device.name.clone())
            .unwrap_or_default();
        let running = model.hotspot().cloned();
        let profile = profile_of(&model).cloned();

        // Start the form from the saved profile once the lists are read, and
        // again if the profile changes (a first turn-on creates one). The
        // effect reads its inputs from its dependencies: a closure kept from
        // an earlier render would still hold the earlier profile.
        use_side_effect_with_deps(
            &(loading, profile.clone()),
            move |(loading, profile): &(bool, Option<Saved>)| {
                if !*loading {
                    let seed = profile.clone();
                    background(defaults, move || read_defaults(seed));
                }
            },
        );
        let ready = defaults.read().clone();
        use_side_effect_with_deps(&ready, move |ready| {
            if let Some(ready) = ready {
                name.set(ready.name.clone());
                password.set(ready.password.clone());
                band.set(ready.band.clone());
            }
        });

        if let Some(error) = error {
            return page_column().child(PageHead::new()).child(
                status(Status::Error(error.0)).action("Try again", move |_| ui.reload_all()),
            );
        }

        let head = PageHead::new();
        let mut blocks: Vec<Element> = Vec::new();

        if let Some(ap) = running {
            // Running: what it is, who is on it, how to join, Turn off.
            let share_target = share::Target {
                ssid: ap.ssid.clone(),
                uuid: Some(ap.uuid.clone()),
                key_mgmt: ap.key_mgmt.clone(),
            };
            let stop_uuid = ap.uuid.clone();
            let stop_name = ap.ssid.clone();
            blocks.push(Element::from(RunningGroup {
                ap,
                device: device.clone(),
            }));
            blocks.push(Element::from(action_strip(vec![
                strip_action(Icon::SendTo, "Share", move |_| {
                    share::open(window, share_target.clone())
                }),
                strip_action(Icon::NetworkWirelessDisabled, "Turn off", move |_| {
                    let uuid = stop_uuid.clone();
                    ui.act(
                        &format!("Turning off {stop_name}."),
                        move || nm::disconnect(&uuid),
                        || {},
                    );
                }),
            ])));
        } else {
            match radio {
                Radio::Missing => blocks.push(Element::from(Group::new(None::<String>).row(
                    Element::from(status(Status::Empty(
                        "This computer has no Wi-Fi hardware.".into(),
                    ))),
                ))),
                Radio::Blocked => blocks.push(Element::from(Group::new(None::<String>).row(
                    Element::from(status(Status::Info("Wi-Fi hardware is disabled.".into()))),
                ))),
                Radio::Airplane => blocks.push(Element::from(Group::new(None::<String>).row(
                    Element::from(status(Status::Info(
                        "Airplane mode is on, so the radio is off. Turn it off on the Network page to share a connection."
                            .into(),
                    ))),
                ))),
                Radio::Off => blocks.push(Element::from(
                    Group::new(None::<String>).row(Element::from(
                        status(Status::Info(
                            "Wi-Fi is off.".into(),
                        ))
                        .action("Turn on Wi-Fi", move |_| {
                            model::toggle_link(ui.ctx, "wifi".to_string(), true, || {
                                model::set_wifi_radio(true)
                            })
                        }),
                    )),
                )),
                Radio::On => {}
            }
            if radio != Radio::Missing {
                let can_start = radio == Radio::On;
                let valid = validate::ssid(name.read().trim()).is_ok()
                    && validate::wpa_passphrase(&password.read()).is_ok();
                let options: Vec<(String, String)> = BAND_OPTIONS
                    .iter()
                    .map(|(key, label)| (key.to_string(), label.to_string()))
                    .collect();
                let on_start = {
                    let device = device.clone();
                    let uuid = profile.as_ref().map(|ap| ap.uuid.clone());
                    move || {
                        let ssid = name.read().trim().to_string();
                        let secret = password.read().clone();
                        if validate::ssid(&ssid).is_err()
                            || validate::wpa_passphrase(&secret).is_err()
                        {
                            return;
                        }
                        let band = band.read().clone();
                        let device = device.clone();
                        let uuid = uuid.clone();
                        ui.act(
                            "Turning on the Wi-Fi hotspot.",
                            move || start(&device, uuid.as_deref(), &ssid, &secret, &band),
                            || {},
                        );
                    }
                };
                let press = on_start.clone();
                blocks.push(Element::from(
                    Group::new(None::<String>)
                        .row(
                            form_field("Network name", name)
                                .validate(validate::ssid)
                                .on_submit(move |_: ()| on_start()),
                        )
                        .row(
                            form_field("Password", password)
                                .password()
                                .hint("At least 8 characters.")
                                .validate(validate::wpa_passphrase),
                        )
                        .row(choice_row(
                            "Band",
                            options,
                            &band.read(),
                            move |key: String| band.set(key),
                        )),
                ));
                let note = match joined_name(&model) {
                    Some(current) if can_start => {
                        format!("Turning it on disconnects this computer from {current}.")
                    }
                    _ => "Devices that join share this computer's connection.".to_string(),
                };
                blocks.push(action_row(
                    Some(&note),
                    action_button("Turn on", move |_| press())
                        .suggested()
                        .enabled(can_start && valid),
                ));
            }
        }
        veiled(page_column().child(head).children(blocks), loading)
    }
}

/// The hotspot that runs: its name, password, band and how many devices are
/// on it. It reads the password and counts the devices itself, so both start
/// when the hotspot does and stop when it ends.
#[derive(PartialEq)]
struct RunningGroup {
    ap: Saved,
    device: String,
}

impl Component for RunningGroup {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let secret = use_state(|| None::<Result<Option<String>, CommandError>>);
        let shown = use_state(|| false);
        let clients = use_state(|| None::<usize>);
        let ap = self.ap.clone();
        let device = self.device.clone();
        use_hook(move || {
            let (uuid, key_mgmt) = (ap.uuid.clone(), ap.key_mgmt.clone());
            background(secret, move || nm::saved_secret(&uuid, &key_mgmt));
            let focus = system::window_focus();
            spawn(async move {
                loop {
                    if *focus.peek() {
                        let device = device.clone();
                        background_then(
                            move || read_clients(&device),
                            move |count| {
                                let mut clients = clients;
                                clients.set_if_modified(count);
                            },
                        );
                    }
                    timer(CLIENTS_EVERY).await;
                }
            });
        });
        let password_row = match &*secret.read() {
            Some(Ok(Some(text))) => share::secret_row("Password", text, shown, overlays),
            Some(Ok(None)) => row("Password").value("None saved"),
            Some(Err(error)) => row("Password").value(error.0.clone()),
            None => row("Password").value("—"),
        };
        let band = match band_key(&self.ap.band).as_str() {
            "bg" => "2.4 GHz",
            "a" => "5 GHz",
            _ => "Automatic",
        };
        let people = match *clients.read() {
            Some(0) => "No devices".to_string(),
            Some(1) => "1 device".to_string(),
            Some(count) => format!("{count} devices"),
            None => "—".to_string(),
        };
        Group::titled("Hotspot is on")
            .row(row("Network name").value(self.ap.ssid.clone()))
            .row(password_row)
            .row(row("Band").value(band))
            .row(row("Connected devices").value(people))
    }
}

/// The network this computer is on now, if any: a hotspot takes the radio.
fn joined_name(model: &nm::Model) -> Option<String> {
    model.current().and_then(|(network, profile)| {
        network
            .map(|network| network.ssid.clone())
            .or_else(|| profile.map(|profile| profile.ssid.clone()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clients_are_the_neighbours_that_answer() {
        let output = "\
10.42.0.12 lladdr aa:bb:cc:dd:ee:01 REACHABLE
10.42.0.13 lladdr aa:bb:cc:dd:ee:02 STALE
10.42.0.14  FAILED
10.42.0.15 lladdr aa:bb:cc:dd:ee:03 DELAY
fe80::1 lladdr aa:bb:cc:dd:ee:04 router REACHABLE
10.42.0.16  INCOMPLETE
";
        assert_eq!(parse_clients(output), 4);
        assert_eq!(parse_clients(""), 0);
    }

    #[test]
    fn a_generated_password_is_valid_for_wpa() {
        let password = generate_password();
        assert_eq!(password.len(), PASSWORD_LENGTH);
        assert_eq!(validate::wpa_passphrase(&password), Ok(()));
        assert!(
            password
                .bytes()
                .all(|byte| PASSWORD_ALPHABET.contains(&byte))
        );
    }

    #[test]
    fn bands_map_to_the_form_keys() {
        assert_eq!(band_key("bg"), "bg");
        assert_eq!(band_key("a"), "a");
        assert_eq!(band_key(""), "auto");
    }
}
