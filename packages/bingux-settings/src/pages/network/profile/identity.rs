//! The Identity page: the profile's name, the hardware address it presents, the MTU, and for each
//! kind its own hardware privacy setting (random Wi-Fi addresses, wake on LAN).
//!
//! The name, the cloned address and the MTU depend on each other's fields, so they wait in a draft
//! for Apply. The Wi-Fi randomisation and wake on LAN dropdowns stand alone and apply at once.

use freya::prelude::*;

use bingux_ui::controls::form::validate;
use bingux_ui::controls::{form_field, pending_bar};

use super::super::{Owner, Section};
use super::advanced::options_with;
use super::draft::{Draft, Values, use_draft};
use super::store;
use super::{Page, options};
use crate::pages::connection_editor::{
    CLONED_MAC_CHOICES, Changes, Choices, Kind, MAC_RANDOMIZATION_CHOICES, ProfileData,
    WAKE_ON_LAN_CHOICES, cloned_mac_caption, connection_name, diff, join_cloned_mac,
    mac_randomization_caption, mtu, split_cloned_mac, wake_on_lan_caption,
};
use crate::routes::Route;
use crate::ui::{Group, choice_row, page_column, row};

/// How the MTU is chosen: left to the network, or set in bytes.
const MTU_CHOICES: Choices = &[("auto", "Automatic"), ("custom", "Custom")];

/// What the form holds, as typed.
#[derive(Clone, Debug, PartialEq)]
struct Form {
    name: String,
    mac_mode: String,
    mac_address: String,
    mtu_mode: String,
    mtu_value: String,
}

impl Form {
    /// The values a profile has now.
    fn from_saved(saved: &ProfileData) -> Self {
        let setting = saved.device_setting();
        let (mac_mode, mac_address) =
            split_cloned_mac(saved.get(&format!("{setting}.cloned-mac-address")));
        let mtu_value = stored_mtu(saved);
        Self {
            name: saved.get("connection.id").to_string(),
            mac_mode,
            mac_address,
            mtu_mode: if mtu_value.is_empty() {
                "auto"
            } else {
                "custom"
            }
            .to_string(),
            mtu_value,
        }
    }

    fn from_draft(draft: &Draft) -> Self {
        Self {
            name: draft.get("name"),
            mac_mode: draft.get("mac_mode"),
            mac_address: draft.get("mac_address"),
            mtu_mode: draft.get("mtu_mode"),
            mtu_value: draft.get("mtu_value"),
        }
    }

    fn values(self) -> Values {
        Values {
            texts: vec![
                ("name", self.name),
                ("mac_mode", self.mac_mode),
                ("mac_address", self.mac_address),
                ("mtu_mode", self.mtu_mode),
                ("mtu_value", self.mtu_value),
            ],
            lists: Vec::new(),
        }
    }

    /// Whether Apply may write the form. A Wi-Fi name is not editable, so it is not checked.
    fn valid(&self, name_editable: bool) -> bool {
        (!name_editable || connection_name(&self.name).is_ok())
            && (self.mac_mode != "address" || validate::mac(&self.mac_address).is_ok())
            && (self.mtu_mode != "custom" || mtu(&self.mtu_value).is_ok())
    }
}

fn read(saved: &ProfileData) -> Values {
    Form::from_saved(saved).values()
}

/// The MTU in bytes as the profile stores it, empty when NetworkManager chooses it. nmcli prints
/// `auto` for 0.
fn stored_mtu(saved: &ProfileData) -> String {
    let raw = saved.get(&format!("{}.mtu", saved.device_setting()));
    if matches!(raw, "" | "auto" | "0") {
        String::new()
    } else {
        raw.to_string()
    }
}

/// The values to write: only what differs from the profile. The name changes only for a profile
/// whose name the person may edit.
fn changes(form: &Form, saved: &ProfileData, name_editable: bool) -> Changes {
    let setting = saved.device_setting();
    let mut out = Changes::default();
    if name_editable {
        diff(
            &mut out,
            "connection.id",
            saved.get("connection.id"),
            form.name.trim(),
        );
    }
    let mac_key = format!("{setting}.cloned-mac-address");
    diff(
        &mut out,
        &mac_key,
        saved.get(&mac_key),
        &join_cloned_mac(&form.mac_mode, &form.mac_address),
    );
    let (old, new) = (
        stored_mtu(saved),
        if form.mtu_mode == "custom" {
            form.mtu_value.trim()
        } else {
            ""
        },
    );
    if old != new {
        out.set(
            &format!("{setting}.mtu"),
            if new.is_empty() { "auto" } else { new },
        );
    }
    out
}

/// The new name when `changes` renames the profile.
fn renamed_to(changes: &Changes) -> Option<String> {
    changes
        .set
        .iter()
        .find(|(key, _)| key == "connection.id")
        .map(|(_, name)| name.clone())
}

/// The stored wake on LAN value as the dropdown names it: an empty value is the same as `none`.
fn wake_on_lan_of(saved: &ProfileData) -> &str {
    match saved.get("802-3-ethernet.wake-on-lan") {
        "" => "none",
        other => other,
    }
}

/// The Identity page of a Wi-Fi or wired connection.
#[derive(PartialEq)]
pub struct IdentityBody {
    pub page: Page,
}

impl Component for IdentityBody {
    fn render(&self) -> impl IntoElement {
        let page = self.page.clone();
        let draft = use_draft(&page.saved, read);
        let busy = use_state(|| false);

        let kind = page.kind();
        if kind == Kind::Vpn {
            return page_column().child(crate::ui::status(crate::ui::Status::Empty(
                "A VPN has no hardware address or MTU of its own.".into(),
            )));
        }
        // A Wi-Fi profile is found again by its name, so the name stays what the network is called.
        let name_editable = kind == Kind::Wired;

        let form = Form::from_draft(&draft);
        let dirty = draft.dirty(&page.saved);
        let ok = form.valid(name_editable);

        let apply = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| {
                let form = Form::from_draft(&draft);
                let changes = changes(&form, &page.saved, name_editable);
                if changes.is_empty() {
                    return;
                }
                let renamed = renamed_to(&changes);
                let after = page.saved.with(&changes);
                let (open, draft) = (page.clone(), draft.clone());
                store::apply_batch(
                    page.ctx,
                    &page.owner,
                    page.state,
                    busy,
                    changes,
                    move || {
                        draft.reset(&after);
                        open_renamed(&open, renamed);
                    },
                );
            }
        };
        let revert = {
            let (page, draft) = (page.clone(), draft.clone());
            move |_: ()| draft.reset(&page.saved)
        };

        let mut identity = Group::titled("Identity");
        if name_editable {
            identity =
                identity.row(form_field("Name", draft.text("name")).validate(connection_name));
        } else {
            identity = identity.row(row("Name").value(page.saved.get("connection.id")));
            let bssid = page.saved.get("802-11-wireless.bssid");
            if !bssid.is_empty() {
                identity = identity.row(
                    row("BSSID")
                        .subtitle("This profile only connects to this access point.")
                        .mono_value(bssid),
                );
            }
        }

        let mac_row = {
            let (draft, saved) = (draft.clone(), page.saved.clone());
            choice_row(
                "Cloned MAC address",
                options(CLONED_MAC_CHOICES),
                &form.mac_mode,
                move |value: String| {
                    // An address typed for a mode that no longer uses it must not keep the draft dirty.
                    if value != "address" {
                        draft.set("mac_address", Form::from_saved(&saved).mac_address);
                    }
                    draft.set("mac_mode", value);
                },
            )
            .subtitle(cloned_mac_caption(&form.mac_mode))
        };
        let mtu_row = {
            let (draft, saved) = (draft.clone(), page.saved.clone());
            choice_row(
                "MTU",
                options(MTU_CHOICES),
                &form.mtu_mode,
                move |value: String| {
                    if value != "custom" {
                        draft.set("mtu_value", Form::from_saved(&saved).mtu_value);
                    }
                    draft.set("mtu_mode", value);
                },
            )
            .subtitle("The largest packet size in bytes. Leave it automatic unless a network needs another.")
        };

        let mut hardware = Group::titled("Hardware").row(mac_row);
        if form.mac_mode == "address" {
            hardware = hardware.row(
                form_field("MAC address", draft.text("mac_address"))
                    .placeholder("00:1A:2B:3C:4D:5E")
                    .monospace()
                    .validate(validate::mac),
            );
        }
        hardware = hardware.row(mtu_row);
        if form.mtu_mode == "custom" {
            hardware = hardware.row(
                form_field("MTU size", draft.text("mtu_value"))
                    .placeholder("1500")
                    .suffix("bytes")
                    .validate(mtu),
            );
        }
        hardware = hardware.row(match kind {
            Kind::Wifi => {
                let selected = match page.saved.get("802-11-wireless.mac-address-randomization") {
                    "" => "default",
                    other => other,
                };
                let page = page.clone();
                choice_row(
                    "Random MAC address",
                    options(MAC_RANDOMIZATION_CHOICES),
                    selected,
                    move |value: String| {
                        page.apply_now(Changes::one(
                            "802-11-wireless.mac-address-randomization",
                            value,
                        ))
                    },
                )
                .subtitle(mac_randomization_caption(selected))
            }
            _ => {
                let selected = wake_on_lan_of(&page.saved).to_string();
                let page = page.clone();
                choice_row(
                    "Wake on LAN",
                    options_with(WAKE_ON_LAN_CHOICES, &selected),
                    &selected,
                    move |value: String| {
                        page.apply_now(Changes::one("802-3-ethernet.wake-on-lan", value))
                    },
                )
                .subtitle(wake_on_lan_caption(&selected))
            }
        });

        page_column()
            .maybe_child(pending_bar(dirty, ok && !*busy.read(), apply, revert))
            .child(identity)
            .child(hardware)
    }
}

/// After a wired profile is renamed its address has changed, unless the page is the device's own:
/// open the page again under the new name.
fn open_renamed(page: &Page, renamed: Option<String>) {
    let (Some(profile), Owner::Wired { device, in_use: false, .. }) = (renamed, &page.owner)
    else {
        return;
    };
    page.ctx.navigator.push(Route::Section {
        owner: Owner::Wired {
            device: device.clone(),
            profile,
            in_use: false,
        },
        section: Section::Identity,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wired(extra: &str) -> ProfileData {
        ProfileData::parse(&format!(
            "connection.type:802-3-ethernet\nconnection.id:Office\n802-3-ethernet.cloned-mac-address:\n802-3-ethernet.mtu:auto\n802-3-ethernet.wake-on-lan:default\n{extra}"
        ))
    }

    fn wifi(extra: &str) -> ProfileData {
        ProfileData::parse(&format!(
            "connection.type:802-11-wireless\nconnection.id:Cafe\n802-11-wireless.cloned-mac-address:\n802-11-wireless.mtu:auto\n{extra}"
        ))
    }

    #[test]
    fn a_default_profile_reads_as_automatic() {
        let form = Form::from_saved(&wired(""));
        assert_eq!(form.name, "Office");
        assert_eq!(
            (form.mac_mode.as_str(), form.mac_address.as_str()),
            ("", "")
        );
        assert_eq!(
            (form.mtu_mode.as_str(), form.mtu_value.as_str()),
            ("auto", "")
        );
    }

    #[test]
    fn a_literal_address_and_a_set_mtu_read_back() {
        let saved =
            wired("802-3-ethernet.cloned-mac-address:02:11:22:33:44:55\n802-3-ethernet.mtu:1400\n");
        let form = Form::from_saved(&saved);
        assert_eq!(form.mac_mode, "address");
        assert_eq!(form.mac_address, "02:11:22:33:44:55");
        assert_eq!(
            (form.mtu_mode.as_str(), form.mtu_value.as_str()),
            ("custom", "1400")
        );
    }

    #[test]
    fn nothing_changed_writes_nothing() {
        let saved = wired("");
        assert!(changes(&Form::from_saved(&saved), &saved, true).is_empty());
    }

    #[test]
    fn rename_writes_the_trimmed_name_and_is_found_again() {
        let saved = wired("");
        let mut form = Form::from_saved(&saved);
        form.name = "  Desk  ".into();
        let changes = changes(&form, &saved, true);
        assert_eq!(
            changes.commands("Office"),
            vec![vec![
                "connection",
                "modify",
                "Office",
                "connection.id",
                "Desk"
            ]]
        );
        assert_eq!(renamed_to(&changes).as_deref(), Some("Desk"));
    }

    #[test]
    fn a_wifi_name_is_never_written() {
        let saved = wifi("");
        let mut form = Form::from_saved(&saved);
        form.name = "Other".into();
        assert!(changes(&form, &saved, false).is_empty());
    }

    #[test]
    fn cloned_mac_modes_and_a_literal_address() {
        let saved = wired("");
        for mode in ["preserve", "permanent", "random", "stable"] {
            let mut form = Form::from_saved(&saved);
            form.mac_mode = mode.into();
            form.mac_address = "ignored".into();
            assert_eq!(
                changes(&form, &saved, true).set,
                vec![(
                    "802-3-ethernet.cloned-mac-address".to_string(),
                    mode.to_string()
                )]
            );
        }
        let mut form = Form::from_saved(&saved);
        form.mac_mode = "address".into();
        form.mac_address = " 02:11:22:33:44:55 ".into();
        assert_eq!(
            changes(&form, &saved, true).set,
            vec![(
                "802-3-ethernet.cloned-mac-address".to_string(),
                "02:11:22:33:44:55".to_string()
            )]
        );
        // Back to the default clears the stored mode.
        let literal = wired("802-3-ethernet.cloned-mac-address:02:11:22:33:44:55\n");
        let mut form = Form::from_saved(&literal);
        form.mac_mode = String::new();
        assert_eq!(
            changes(&form, &literal, true).set,
            vec![(
                "802-3-ethernet.cloned-mac-address".to_string(),
                String::new()
            )]
        );
    }

    #[test]
    fn mtu_custom_and_automatic() {
        let saved = wired("");
        let mut form = Form::from_saved(&saved);
        form.mtu_mode = "custom".into();
        form.mtu_value = " 1400 ".into();
        assert_eq!(
            changes(&form, &saved, true).set,
            vec![("802-3-ethernet.mtu".to_string(), "1400".to_string())]
        );
        let custom = wired("802-3-ethernet.mtu:1400\n");
        let mut form = Form::from_saved(&custom);
        form.mtu_mode = "auto".into();
        assert_eq!(
            changes(&form, &custom, true).set,
            vec![("802-3-ethernet.mtu".to_string(), "auto".to_string())]
        );
        // A Wi-Fi profile keeps its MTU in its own setting.
        let wifi_saved = wifi("");
        let mut form = Form::from_saved(&wifi_saved);
        form.mtu_mode = "custom".into();
        form.mtu_value = "1300".into();
        assert_eq!(
            changes(&form, &wifi_saved, false).set,
            vec![("802-11-wireless.mtu".to_string(), "1300".to_string())]
        );
    }

    #[test]
    fn validity_follows_the_visible_fields() {
        let saved = wired("");
        let mut form = Form::from_saved(&saved);
        assert!(form.valid(true));
        form.name = " ".into();
        assert!(!form.valid(true));
        assert!(form.valid(false));
        form.name = "Office".into();
        form.mac_mode = "address".into();
        assert!(!form.valid(true), "an address is required");
        form.mac_address = "02:11:22:33:44:55".into();
        assert!(form.valid(true));
        form.mtu_mode = "custom".into();
        assert!(!form.valid(true), "a size is required");
        form.mtu_value = "100".into();
        assert!(!form.valid(true), "below 576");
        form.mtu_value = "1500".into();
        assert!(form.valid(true));
        // A hidden field does not block.
        form.mac_mode = "random".into();
        form.mac_address = "nonsense".into();
        assert!(form.valid(true));
    }

    #[test]
    fn empty_wake_on_lan_reads_as_off() {
        assert_eq!(wake_on_lan_of(&wired("")), "default");
        assert_eq!(
            wake_on_lan_of(&wired("802-3-ethernet.wake-on-lan:\n")),
            "none"
        );
        assert_eq!(
            Changes::one("802-3-ethernet.wake-on-lan", "none").commands("Office"),
            vec![vec![
                "connection",
                "modify",
                "Office",
                "802-3-ethernet.wake-on-lan",
                "none"
            ]]
        );
    }
}
