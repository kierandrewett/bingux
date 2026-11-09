//! The form dialogs for joining a network: the password of a network in
//! range, the sign-in of an 802.1X network, and a hidden network by name.
//! A dialog stays open while the connection is made, shows a failure inside
//! itself (a wrong password is not a toast), and closes on success.

use freya::prelude::*;

use super::nm::{self, HiddenSecurity, Security};
use crate::system::{CommandError, background_then};
use crate::ui::{Group, Overlays, Status, choice_row, form_field, status, use_overlays};
use bingux_ui::controls::form::validate;
use bingux_ui::shell::{form_dialog, open_form};
use bingux_ui::style::*;

/// Where a join stands, shown under the fields.
#[derive(Clone, PartialEq)]
enum Phase {
    Idle,
    Connecting,
    Failed(String),
}

impl Phase {
    fn busy(&self) -> bool {
        *self == Self::Connecting
    }
}

/// The line under the fields for a join in progress or failed.
fn phase_line(phase: &Phase, ssid: &str) -> Option<Element> {
    match phase {
        Phase::Idle => None,
        Phase::Connecting => Some(Element::from(status(Status::Loading(format!(
            "Connecting to {ssid}…"
        ))))),
        Phase::Failed(text) => Some(Element::from(status(Status::Error(text.clone())))),
    }
}

/// A join ended: close on success, show the reason otherwise.
fn finish(mut phase: State<Phase>, overlays: Overlays, result: Result<(), CommandError>) {
    match result {
        Ok(()) => {
            overlays.dismiss_dialog();
            super::reload_all();
        }
        Err(error) => phase.set(Phase::Failed(nm::join_error(&error))),
    }
}

/// Run `work` off the UI thread and report to the popup.
fn run(
    mut phase: State<Phase>,
    overlays: Overlays,
    work: impl FnOnce() -> Result<(), CommandError> + Send + 'static,
) {
    if phase.read().busy() {
        return;
    }
    phase.set(Phase::Connecting);
    background_then(work, move |result| finish(phase, overlays, result));
}

/// A WEP key: 5 or 13 characters, or 10 or 26 hexadecimal digits.
fn wep_key(text: &str) -> Result<(), String> {
    let hex = text.bytes().all(|byte| byte.is_ascii_hexdigit());
    match text.len() {
        5 | 13 => Ok(()),
        10 | 26 if hex => Ok(()),
        _ => Err("Use 5 or 13 characters, or 10 or 26 hexadecimal digits.".into()),
    }
}

/// A WPA3 password. Any length is allowed by the standard, so only an empty
/// one is refused here; NetworkManager reports the rest.
fn sae_password(text: &str) -> Result<(), String> {
    if text.is_empty() {
        Err("Enter the password.".into())
    } else {
        Ok(())
    }
}

fn password_check(security: Security) -> fn(&str) -> Result<(), String> {
    match security {
        Security::Wep => wep_key,
        Security::Wpa3 => sae_password,
        _ => validate::wpa_passphrase,
    }
}

/// The form dialog that asks for the password of a network in range.
pub fn open_password(overlays: Overlays, ssid: String, security: Security) {
    open_form(
        overlays,
        ssid.clone(),
        PasswordBody { ssid, security },
        None,
    );
}

#[derive(PartialEq)]
struct PasswordBody {
    ssid: String,
    security: Security,
}

impl Component for PasswordBody {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let password = use_state(String::new);
        let phase = use_state(|| Phase::Idle);
        let check = password_check(self.security);
        let problem = check(&password.read()).err();
        let busy = phase.read().busy();
        let ssid = self.ssid.clone();
        let start = {
            let ssid = ssid.clone();
            move || {
                let secret = password.read().clone();
                if check(&secret).is_ok() {
                    let ssid = ssid.clone();
                    run(phase, overlays, move || {
                        nm::connect_new(&ssid, Some(&secret))
                    });
                }
            }
        };
        let submit = start.clone();
        let field = form_field("Password", password)
            .password()
            .autofocus()
            .validate(check)
            .on_submit(move |_: ()| submit());
        let body = rect()
            .tw(
                &bingux_ui::surface::tokens_now(),
                &[flex_col(), w_full(), gap(S4)],
            )
            .child(Group::new(None::<String>).row(field).footer(format!(
                "{} network. Passwords are case sensitive.",
                self.security.label()
            )))
            .child(bingux_ui::motion::reveal_block_if(
                S6,
                phase_line(&phase.read(), &ssid),
            ));
        form_dialog(self.ssid.clone(), body, "Connect", move |_: ()| start())
            .blocked(if busy { None } else { problem })
            .stays_open()
    }
}

/// The form dialog for an 802.1X network: a user name and password.
pub fn open_enterprise(overlays: Overlays, ssid: String) {
    open_form(overlays, ssid.clone(), EnterpriseBody { ssid }, None);
}

#[derive(PartialEq)]
struct EnterpriseBody {
    ssid: String,
}

impl Component for EnterpriseBody {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let user = use_state(String::new);
        let password = use_state(String::new);
        let phase = use_state(|| Phase::Idle);
        let blocked = if user.read().trim().is_empty() {
            Some("Enter the user name.".to_string())
        } else if password.read().is_empty() {
            Some("Enter the password.".to_string())
        } else {
            None
        };
        let ssid = self.ssid.clone();
        let start = {
            let ssid = ssid.clone();
            move || {
                let name = user.read().trim().to_string();
                let secret = password.read().clone();
                if !name.is_empty() && !secret.is_empty() {
                    let ssid = ssid.clone();
                    run(phase, overlays, move || {
                        nm::connect_enterprise(&ssid, &name, &secret)
                    });
                }
            }
        };
        let submit = start.clone();
        let body = rect()
            .tw(&bingux_ui::surface::tokens_now(), &[flex_col(), w_full(), gap(S4)])
            .child(
                Group::new(None::<String>)
                    .row(form_field("User name", user).autofocus())
                    .row(
                        form_field("Password", password)
                            .password()
                            .on_submit(move |_: ()| submit()),
                    )
                    .footer(
                        "Signs in with PEAP and MSCHAPv2, and checks the network's certificate \
                         against the system's. Other methods are set in the network's Security settings.",
                    ),
            )
            .child(bingux_ui::motion::reveal_block_if(S6, phase_line(&phase.read(), &ssid)));
        form_dialog(self.ssid.clone(), body, "Connect", move |_: ()| start())
            .blocked(if phase.read().busy() { None } else { blocked })
            .stays_open()
    }
}

/// The form dialog for a network that does not announce its name.
pub fn open_hidden(overlays: Overlays) {
    open_form(overlays, "Hidden network", HiddenBody, None);
}

#[derive(PartialEq)]
struct HiddenBody;

const SECURITY_OPTIONS: [(&str, &str); 3] =
    [("open", "Open"), ("wpa2", "WPA & WPA2"), ("wpa3", "WPA3")];

fn hidden_security(key: &str) -> HiddenSecurity {
    match key {
        "wpa3" => HiddenSecurity::Wpa3,
        "wpa2" => HiddenSecurity::Wpa2,
        _ => HiddenSecurity::Open,
    }
}

impl Component for HiddenBody {
    fn render(&self) -> impl IntoElement {
        let overlays = use_overlays();
        let name = use_state(String::new);
        let password = use_state(String::new);
        let mut security = use_state(|| "wpa2".to_string());
        let phase = use_state(|| Phase::Idle);
        let kind = hidden_security(&security.read());
        let check = password_check(if kind == HiddenSecurity::Wpa3 {
            Security::Wpa3
        } else {
            Security::Wpa2
        });
        let needs_password = kind != HiddenSecurity::Open;
        let blocked = validate::ssid(name.read().trim()).err().or_else(|| {
            if needs_password {
                check(&password.read()).err()
            } else {
                None
            }
        });
        let busy = phase.read().busy();
        let start = move || {
            let ssid = name.read().trim().to_string();
            let kind = hidden_security(&security.read());
            let secret = if kind == HiddenSecurity::Open {
                String::new()
            } else {
                password.read().clone()
            };
            let valid = validate::ssid(&ssid).is_ok()
                && (kind == HiddenSecurity::Open || check(&secret).is_ok());
            if valid {
                run(phase, overlays, move || {
                    nm::connect_hidden(&ssid, kind, &secret)
                });
            }
        };
        let press = start.clone();
        let options = SECURITY_OPTIONS
            .iter()
            .map(|(key, label)| (key.to_string(), label.to_string()))
            .collect();
        let submit = start.clone();
        let mut fields = Group::new(None::<String>)
            .row(
                form_field("Network name", name)
                    .autofocus()
                    .validate(validate::ssid)
                    .on_submit(move |_: ()| start()),
            )
            .row(choice_row(
                "Security",
                options,
                &security.read(),
                move |key: String| security.set(key),
            ));
        if needs_password {
            fields = fields.row(
                form_field("Password", password)
                    .password()
                    .validate(check)
                    .on_submit(move |_: ()| submit()),
            );
        }
        let ssid = name.read().trim().to_string();
        let body = rect()
            .tw(
                &bingux_ui::surface::tokens_now(),
                &[flex_col(), w_full(), gap(S4)],
            )
            .child(fields)
            .child(bingux_ui::motion::reveal_block_if(
                S6,
                phase_line(&phase.read(), &ssid),
            ));
        form_dialog("Hidden network", body, "Connect", move |_: ()| press())
            .blocked(if busy { None } else { blocked })
            .stays_open()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wep_keys() {
        for good in [
            "12345",
            "abcdefghijklm",
            "0123456789",
            "0123456789abcdef01234567ab",
        ] {
            assert_eq!(wep_key(good), Ok(()), "{good}");
        }
        for bad in [
            "",
            "1234",
            "123456",
            "zzzzzzzzzz",
            "0123456789abcdef01234567a",
        ] {
            assert!(wep_key(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn wpa3_takes_any_non_empty_password() {
        assert!(sae_password("").is_err());
        assert!(sae_password("short").is_ok());
    }
}
