//! Sharing a network: a QR code another phone's camera joins from, the name,
//! and the password to show or copy. The password is read from
//! NetworkManager's saved secret for the profile; a network with none says so.

use freya::prelude::*;

use super::nm;
use crate::system::{CommandError, background};
use crate::ui::{
    Group, PopupAction, PopupActions, Status, Window, action_button, caption, popup, popup_column,
    qr_code, row, status, use_overlays, use_popup_actions, use_window,
};
use bingux_ui::layout::hstack;
use bingux_ui::style::*;
use bingux_ui::window::{PopupFlags, WindowLike, WindowParent};

/// What the popup shares.
#[derive(Clone, PartialEq)]
pub struct Target {
    pub ssid: String,
    /// The saved profile that holds the password; `None` for a network that
    /// is open and not saved.
    pub uuid: Option<String>,
    /// `802-11-wireless-security.key-mgmt` of that profile, empty when open.
    pub key_mgmt: String,
}

/// How a phone should treat the network, as the QR format names it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Open,
    Wep,
    Wpa,
}

impl Kind {
    /// `None` for a profile that cannot be shared as one password: 802.1X
    /// signs in with an account, not a code.
    pub fn of(key_mgmt: &str) -> Option<Self> {
        match key_mgmt {
            "" => Some(Self::Open),
            "none" => Some(Self::Wep),
            "wpa-psk" | "sae" | "wpa-psk-sha256" | "ft-psk" | "ft-sae" => Some(Self::Wpa),
            _ => None,
        }
    }
}

/// A field value of the QR text. The characters `\ ; , : "` are escaped with
/// a backslash, and a value made only of hexadecimal digits is quoted so a
/// reader does not take it for raw bytes.
fn field(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    for c in value.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

/// The text a phone's camera reads to join: `WIFI:T:WPA;S:name;P:password;;`.
/// An open network is `T:nopass` and carries no password.
pub fn payload(ssid: &str, kind: Kind, password: Option<&str>) -> String {
    let ssid = field(ssid);
    match (kind, password) {
        (Kind::Open, _) | (_, None) => format!("WIFI:T:nopass;S:{ssid};;"),
        (Kind::Wep, Some(password)) => format!("WIFI:T:WEP;S:{ssid};P:{};;", field(password)),
        (Kind::Wpa, Some(password)) => format!("WIFI:T:WPA;S:{ssid};P:{};;", field(password)),
    }
}

/// Open the share popup.
pub fn open(window: Window, target: Target) {
    window.open(
        popup("Share network", ShareBody { target })
            .size(420.0, 500.0)
            .flags(PopupFlags::GRABBER | PopupFlags::OK),
    );
}

/// A row for a secret: dots or the text under the title, with Show/Hide and
/// Copy at the right. The text is the subtitle so a long one wraps instead of
/// pushing the title out of a narrow row.
pub fn secret_row(
    title: &str,
    secret: &str,
    shown: State<bool>,
    overlays: crate::ui::Overlays,
) -> crate::ui::Row {
    let mut shown = shown;
    let visible = *shown.read();
    let text = if visible {
        secret.to_string()
    } else {
        "•".repeat(secret.chars().count().min(12))
    };
    let copy = secret.to_string();
    row(title).subtitle(text).trailing(
        hstack(S2)
            .child(
                action_button(if visible { "Hide" } else { "Show" }, move |_| {
                    shown.set(!visible)
                })
                .compact()
                .flat(),
            )
            .child(
                action_button("Copy", move |_| {
                    match freya::clipboard::Clipboard::set(copy.clone()) {
                        Ok(()) => overlays.toast("Copied."),
                        Err(_) => overlays.toast("The password could not be copied."),
                    }
                })
                .compact()
                .flat(),
            ),
    )
}

/// The password, or why there is none.
type Secret = Result<Option<String>, CommandError>;

#[derive(PartialEq)]
struct ShareBody {
    target: Target,
}

impl Component for ShareBody {
    fn render(&self) -> impl IntoElement {
        let window = use_window();
        let overlays = use_overlays();
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Done".into(),
                enabled: true,
                on_press: EventHandler::new(move |_: ()| window.close()),
            }),
        });
        let secret = use_state(|| None::<Secret>);
        let shown = use_state(|| false);
        let target = self.target.clone();
        use_hook({
            let target = target.clone();
            move || match target.uuid.clone() {
                Some(uuid) => {
                    let key_mgmt = target.key_mgmt.clone();
                    background(secret, move || nm::saved_secret(&uuid, &key_mgmt));
                }
                None => {
                    let mut secret = secret;
                    secret.set(Some(Ok(None)));
                }
            }
        });

        let Some(kind) = Kind::of(&target.key_mgmt) else {
            return popup_column().child(status(Status::Info(
                "This network signs in with an account, so it cannot be shared as a code.".into(),
            )));
        };
        let loaded = secret.read().clone();
        let password = match &loaded {
            Some(Ok(Some(password))) => Some(password.clone()),
            _ => None,
        };
        let mut body = popup_column();
        match &loaded {
            None => {
                body = body.child(status(Status::Loading(
                    "Reading the saved password…".into(),
                )));
            }
            Some(Err(error)) => {
                body = body.child(status(Status::Error(error.0.clone())));
            }
            Some(Ok(None)) if kind != Kind::Open => {
                body = body.child(status(Status::Info(
                    "NetworkManager has no saved password for this network, so there is nothing to share."
                        .into(),
                )));
            }
            Some(Ok(_)) => {
                let text = payload(&target.ssid, kind, password.as_deref());
                body = body
                    .child(
                        rect().tw_layout(&[w_full(), items(Align::Center)]).child(
                            qr_code(text)
                                .size(220.0)
                                .label(format!("QR code to join {}", target.ssid)),
                        ),
                    )
                    .child(caption(
                        "Point a phone's camera at the code to join this network.",
                    ));
            }
        }

        let name_row = row("Network name").value(target.ssid.clone());
        let password_row = match (&loaded, kind) {
            (_, Kind::Open) => row("Password").value("None, open network"),
            (Some(Ok(Some(password))), _) => secret_row("Password", password, shown, overlays),
            _ => row("Password").value("—"),
        };
        body.child(Group::new(None::<String>).row(name_row).row(password_row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_network() {
        assert_eq!(
            payload("Home", Kind::Wpa, Some("correct horse")),
            "WIFI:T:WPA;S:Home;P:correct horse;;"
        );
    }

    #[test]
    fn special_characters_are_escaped() {
        assert_eq!(
            payload("Cafe: \"Un;it,y\"\\x", Kind::Wpa, Some("p:a\\ss;w,o\"rd1")),
            "WIFI:T:WPA;S:Cafe\\: \\\"Un\\;it\\,y\\\"\\\\x;P:p\\:a\\\\ss\\;w\\,o\\\"rd1;;"
        );
    }

    #[test]
    fn hexadecimal_looking_values_are_quoted() {
        assert_eq!(
            payload("1234", Kind::Wpa, Some("deadbeef")),
            "WIFI:T:WPA;S:\"1234\";P:\"deadbeef\";;"
        );
        // Not all hexadecimal: left alone.
        assert_eq!(
            payload("Home", Kind::Wpa, Some("12345678g")),
            "WIFI:T:WPA;S:Home;P:12345678g;;"
        );
    }

    #[test]
    fn open_and_wep_networks() {
        assert_eq!(
            payload("Cafe Guest", Kind::Open, None),
            "WIFI:T:nopass;S:Cafe Guest;;"
        );
        // An open network never carries a password, even if one is given.
        assert_eq!(
            payload("Cafe Guest", Kind::Open, Some("x")),
            "WIFI:T:nopass;S:Cafe Guest;;"
        );
        assert_eq!(
            payload("Old", Kind::Wep, Some("qwert")),
            "WIFI:T:WEP;S:Old;P:qwert;;"
        );
    }

    #[test]
    fn kinds_follow_the_key_management() {
        assert_eq!(Kind::of(""), Some(Kind::Open));
        assert_eq!(Kind::of("none"), Some(Kind::Wep));
        assert_eq!(Kind::of("wpa-psk"), Some(Kind::Wpa));
        assert_eq!(Kind::of("sae"), Some(Kind::Wpa));
        assert_eq!(Kind::of("wpa-eap"), None);
    }
}
