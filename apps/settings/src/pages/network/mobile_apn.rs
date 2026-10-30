//! Access point names: the `gsm` profiles of NetworkManager, their list and
//! their editor.
//!
//! One profile is one access point name (APN). The one the modem uses is the
//! one that is active, or else the one that connects by itself. Everything is
//! `nmcli`; nothing is kept anywhere else.

use freya::prelude::*;

use crate::icons::Icon;
use crate::pages::loading::veiled;
use crate::pages::network::model::{CONNECT, NetCtx, QUICK, nmcli, split_terse, use_net_ctx};
use crate::routes;
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{
    CANCEL_ID, Group, PageHead, ResponseKind, Status, action_row, choice_row, dialog, form_field, icon_button,
    page_column, pending_bar, primary_button, radio_row, radio_rows, row, status, switch_row,
    use_page_busy,
};

/// The properties one profile is read with.
const FIELDS: &str = "connection.id,connection.uuid,connection.autoconnect,gsm.apn,gsm.username,\
gsm.home-only,gsm.network-id,gsm.auto-config,ppp.refuse-pap,ppp.refuse-chap,ppp.refuse-mschap,\
ppp.refuse-mschapv2,ppp.refuse-eap,ipv4.method,ipv6.method,GENERAL.STATE";

// ---------------------------------------------------------------------------
// Authentication and IP type
// ---------------------------------------------------------------------------

/// Which PPP authentication methods the modem may use, as NetworkManager
/// stores them: `ppp.refuse-*` for PAP, CHAP, MSCHAP, MSCHAPv2 and EAP.
/// `true` means allowed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Auth(pub [bool; 5]);

impl Auth {
    pub const AUTOMATIC: Self = Self([true; 5]);

    pub fn key(self) -> &'static str {
        match self.0 {
            [true, true, true, true, true] => "auto",
            [false, false, false, false, false] => "none",
            [true, false, false, false, false] => "pap",
            [false, true, false, false, false] => "chap",
            [true, true, false, false, false] => "pap-chap",
            [false, false, true, false, false] => "mschap",
            [false, false, false, true, false] => "mschapv2",
            [false, false, false, false, true] => "eap",
            _ => "custom",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(Self(match key {
            "auto" => [true; 5],
            "none" => [false; 5],
            "pap" => [true, false, false, false, false],
            "chap" => [false, true, false, false, false],
            "pap-chap" => [true, true, false, false, false],
            "mschap" => [false, false, true, false, false],
            "mschapv2" => [false, false, false, true, false],
            "eap" => [false, false, false, false, true],
            _ => return None,
        }))
    }

    fn label(key: &str) -> &'static str {
        match key {
            "auto" => "Automatic",
            "none" => "None",
            "pap" => "PAP",
            "chap" => "CHAP",
            "pap-chap" => "PAP or CHAP",
            "mschap" => "MSCHAP",
            "mschapv2" => "MSCHAPv2",
            "eap" => "EAP",
            _ => "Custom",
        }
    }

    /// The Authentication list; a combination set elsewhere is listed too, so
    /// it shows as selected.
    fn options(current: Self) -> Vec<(String, String)> {
        let mut keys = vec![
            "auto", "none", "pap", "chap", "pap-chap", "mschap", "mschapv2", "eap",
        ];
        if current.key() == "custom" {
            keys.push("custom");
        }
        keys.into_iter()
            .map(|key| (key.to_string(), Self::label(key).to_string()))
            .collect()
    }
}

/// Which IP families the data session asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IpType {
    V4,
    V6,
    V4V6,
}

impl IpType {
    fn key(self) -> &'static str {
        match self {
            Self::V4 => "ipv4",
            Self::V6 => "ipv6",
            Self::V4V6 => "ipv4v6",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "ipv6" => Self::V6,
            "ipv4v6" => Self::V4V6,
            _ => Self::V4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::V4 => "IPv4",
            Self::V6 => "IPv6",
            Self::V4V6 => "IPv4 and IPv6",
        }
    }

    /// The `ipv4.method` and `ipv6.method` values of this type: a family that
    /// is not asked for is switched off.
    fn methods(self) -> (&'static str, &'static str) {
        match self {
            Self::V4 => ("auto", "disabled"),
            Self::V6 => ("disabled", "auto"),
            Self::V4V6 => ("auto", "auto"),
        }
    }

    /// The type of a profile with these methods. A profile with both off is
    /// treated as IPv4, the one every network has.
    fn from_methods(ipv4: &str, ipv6: &str) -> Self {
        let on = |method: &str| !matches!(method, "disabled" | "ignore");
        match (on(ipv4), on(ipv6)) {
            (false, true) => Self::V6,
            (true, true) => Self::V4V6,
            _ => Self::V4,
        }
    }
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

/// One mobile broadband profile.
#[derive(Clone, PartialEq, Debug)]
pub struct Profile {
    pub name: String,
    pub uuid: String,
    pub apn: String,
    pub user: String,
    /// Read only for the editor; the list never asks for secrets.
    pub password: String,
    /// NetworkManager fills APN, user name and password from its provider
    /// database.
    pub auto_config: bool,
    pub autoconnect: bool,
    pub active: bool,
    pub home_only: bool,
    /// The carrier this profile is pinned to (MCC and MNC), or empty.
    pub network_id: String,
    pub auth: Auth,
    pub ip: IpType,
}

impl Profile {
    pub fn blank() -> Self {
        Self {
            name: String::new(),
            uuid: String::new(),
            apn: String::new(),
            user: String::new(),
            password: String::new(),
            auto_config: false,
            autoconnect: false,
            active: false,
            home_only: false,
            network_id: String::new(),
            auth: Auth::AUTOMATIC,
            ip: IpType::V4V6,
        }
    }

    /// What the list shows under the name.
    pub fn summary(&self) -> String {
        if self.auto_config && self.apn.is_empty() {
            "Detected from the carrier".to_string()
        } else if self.apn.is_empty() {
            "No access point name".to_string()
        } else {
            self.apn.clone()
        }
    }
}

/// Which profile the modem uses: the one that is active, else the one that
/// connects by itself.
pub fn selected(profiles: &[Profile]) -> Option<&Profile> {
    profiles
        .iter()
        .find(|profile| profile.active)
        .or_else(|| profiles.iter().find(|profile| profile.autoconnect))
}

/// A profile from `nmcli -t -f <FIELDS> connection show`: one `key:value` line
/// per property. Values are not escaped in this output.
pub fn parse_profile(output: &str) -> Profile {
    let mut profile = Profile::blank();
    let mut methods = (String::new(), String::new());
    let mut refused = [false; 5];
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim_end();
        let yes = value == "yes";
        match key {
            "connection.id" => profile.name = value.to_string(),
            "connection.uuid" => profile.uuid = value.to_string(),
            "connection.autoconnect" => profile.autoconnect = yes,
            "gsm.apn" => profile.apn = value.to_string(),
            "gsm.username" => profile.user = value.to_string(),
            "gsm.password" => profile.password = value.to_string(),
            "gsm.home-only" => profile.home_only = yes,
            "gsm.network-id" => profile.network_id = value.to_string(),
            "gsm.auto-config" => profile.auto_config = yes,
            "ppp.refuse-pap" => refused[0] = yes,
            "ppp.refuse-chap" => refused[1] = yes,
            "ppp.refuse-mschap" => refused[2] = yes,
            "ppp.refuse-mschapv2" => refused[3] = yes,
            "ppp.refuse-eap" => refused[4] = yes,
            "ipv4.method" => methods.0 = value.to_string(),
            "ipv6.method" => methods.1 = value.to_string(),
            "GENERAL.STATE" => profile.active = value.starts_with("activated"),
            _ => {}
        }
    }
    profile.auth = Auth(refused.map(|refuse| !refuse));
    profile.ip = IpType::from_methods(&methods.0, &methods.1);
    profile
}

/// The names and ids of the `gsm` profiles in
/// `nmcli -t -f NAME,UUID,TYPE connection show` output.
pub fn parse_gsm_list(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let fields = split_terse(line);
            (fields.get(2).map(String::as_str) == Some("gsm"))
                .then(|| (fields[0].clone(), fields[1].clone()))
        })
        .collect()
}

fn show(uuid: &str, secrets: bool) -> Result<Profile, CommandError> {
    let fields = if secrets {
        format!("{FIELDS},gsm.password")
    } else {
        FIELDS.to_string()
    };
    let mut args = vec!["-t"];
    if secrets {
        args.push("-s");
    }
    args.extend(["-f", fields.as_str(), "connection", "show", "uuid", uuid]);
    nmcli(&args, QUICK).map(|output| parse_profile(&output))
}

/// Every `gsm` profile, without passwords.
pub fn load_profiles() -> Result<Vec<Profile>, CommandError> {
    #[cfg(feature = "snapshot")]
    if super::modem::fake::scenario().is_some() {
        return Ok(fixtures::profiles());
    }
    let list = nmcli(&["-t", "-f", "NAME,UUID,TYPE", "connection", "show"], QUICK)?;
    parse_gsm_list(&list)
        .iter()
        .map(|(_, uuid)| show(uuid, false))
        .collect()
}

/// The profile named `name` with its password, and the names every profile
/// has, for the editor.
#[derive(Clone)]
struct Editing {
    profile: Option<Profile>,
    names: Vec<String>,
}

fn load_editing(name: Option<String>) -> Result<Editing, CommandError> {
    #[cfg(feature = "snapshot")]
    if super::modem::fake::scenario().is_some() {
        let profiles = fixtures::profiles();
        return Ok(Editing {
            profile: name.and_then(|name| {
                profiles
                    .iter()
                    .find(|profile| profile.name == name)
                    .cloned()
            }),
            names: fixtures::profiles()
                .into_iter()
                .map(|profile| profile.name)
                .collect(),
        });
    }
    let list = nmcli(&["-t", "-f", "NAME,UUID,TYPE", "connection", "show"], QUICK)?;
    let names = list
        .lines()
        .filter_map(|line| split_terse(line).into_iter().next())
        .collect();
    let profile = match name {
        None => None,
        Some(name) => {
            let (_, uuid) = parse_gsm_list(&list)
                .into_iter()
                .find(|(profile, _)| *profile == name)
                .ok_or_else(|| CommandError(format!("\"{name}\" no longer exists.")))?;
            Some(show(&uuid, true)?)
        }
    };
    Ok(Editing { profile, names })
}

// ---------------------------------------------------------------------------
// Changes
// ---------------------------------------------------------------------------

fn run(args: &[String], timeout: std::time::Duration) -> Result<(), CommandError> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    nmcli(&args, timeout).map(|_| ())
}

fn flag(on: bool) -> String {
    if on { "yes" } else { "no" }.to_string()
}

/// What the editor holds.
#[derive(Clone, PartialEq, Debug)]
pub struct Draft {
    pub name: String,
    pub apn: String,
    pub user: String,
    pub password: String,
    pub auto_config: bool,
    pub auth: Auth,
    pub ip: IpType,
}

impl Draft {
    fn of(profile: &Profile) -> Self {
        Self {
            name: profile.name.clone(),
            apn: profile.apn.clone(),
            user: profile.user.clone(),
            password: profile.password.clone(),
            auto_config: profile.auto_config,
            auth: profile.auth,
            ip: profile.ip,
        }
    }

    /// The `nmcli` property and value pairs that make a profile of this draft.
    /// A profile that detects its settings sends none of its own: NetworkManager
    /// fills in the APN, user name and password, and it turns detection off for
    /// good once a password is stored, which `nmcli` cannot take back.
    fn properties(&self) -> Vec<String> {
        let (ipv4, ipv6) = self.ip.methods();
        let mut args = Vec::new();
        let mut set = |key: &str, value: String| {
            args.push(key.to_string());
            args.push(value);
        };
        set("gsm.auto-config", flag(self.auto_config));
        if !self.auto_config {
            set("gsm.apn", self.apn.clone());
            set("gsm.username", self.user.clone());
            set("gsm.password", self.password.clone());
            // The password is stored in the profile, not asked for by an agent.
            set("gsm.password-flags", "0".to_string());
        }
        for (key, allowed) in [
            "ppp.refuse-pap",
            "ppp.refuse-chap",
            "ppp.refuse-mschap",
            "ppp.refuse-mschapv2",
            "ppp.refuse-eap",
        ]
        .into_iter()
        .zip(self.auth.0)
        {
            set(key, flag(!allowed));
        }
        set("ipv4.method", ipv4.to_string());
        set("ipv6.method", ipv6.to_string());
        args
    }
}

/// Add a profile. It connects by itself when no other profile does.
pub fn create(draft: &Draft, autoconnect: bool) -> Result<(), CommandError> {
    let mut args: Vec<String> = [
        "connection",
        "add",
        "type",
        "gsm",
        // Any modem: the interface of a modem is its control port, which
        // changes between boots.
        "ifname",
        "*",
        "con-name",
    ]
    .map(String::from)
    .to_vec();
    args.push(draft.name.trim().to_string());
    args.push("connection.autoconnect".into());
    args.push(flag(autoconnect));
    args.extend(draft.properties());
    run(&args, QUICK)
}

/// Write `draft` over `profile`. A profile that is connected reconnects so the
/// modem uses the new settings: a mobile connection has no live reapply.
pub fn modify(profile: &Profile, draft: &Draft) -> Result<(), CommandError> {
    let mut args: Vec<String> = ["connection", "modify", "uuid", profile.uuid.as_str()]
        .map(String::from)
        .to_vec();
    args.push("connection.id".into());
    args.push(draft.name.trim().to_string());
    args.extend(draft.properties());
    run(&args, QUICK)?;
    if profile.active {
        run(
            &["connection", "up", "uuid", profile.uuid.as_str()].map(String::from),
            CONNECT,
        )?;
    }
    Ok(())
}

pub fn delete(profile: &Profile) -> Result<(), CommandError> {
    run(
        &["connection", "delete", "uuid", profile.uuid.as_str()].map(String::from),
        QUICK,
    )
}

/// Make `chosen` the profile the modem uses: it connects by itself and no
/// other does. When `connected`, the modem switches to it now.
pub fn choose(profiles: &[Profile], chosen: &Profile, connected: bool) -> Result<(), CommandError> {
    for profile in profiles {
        let wanted = profile.uuid == chosen.uuid;
        if profile.autoconnect != wanted {
            run(
                &[
                    "connection",
                    "modify",
                    "uuid",
                    profile.uuid.as_str(),
                    "connection.autoconnect",
                    if wanted { "yes" } else { "no" },
                ]
                .map(String::from),
                QUICK,
            )?;
        }
    }
    if connected {
        run(
            &["connection", "up", "uuid", chosen.uuid.as_str()].map(String::from),
            CONNECT,
        )?;
    }
    Ok(())
}

/// Allow or refuse roaming on every profile; the choice is the modem's, not
/// one profile's. The connection that is up reconnects, as NetworkManager
/// reads the setting only when it connects.
pub fn set_roaming(profiles: &[Profile], allowed: bool) -> Result<(), CommandError> {
    for profile in profiles {
        run(
            &[
                "connection",
                "modify",
                "uuid",
                profile.uuid.as_str(),
                "gsm.home-only",
                if allowed { "no" } else { "yes" },
            ]
            .map(String::from),
            QUICK,
        )?;
    }
    match profiles.iter().find(|profile| profile.active) {
        Some(active) => run(
            &["connection", "up", "uuid", active.uuid.as_str()].map(String::from),
            CONNECT,
        ),
        None => Ok(()),
    }
}

/// Pin every profile to the carrier with `code` (MCC and MNC), or to none.
pub fn set_network_id(profiles: &[Profile], code: &str) -> Result<(), CommandError> {
    for profile in profiles {
        run(
            &[
                "connection",
                "modify",
                "uuid",
                profile.uuid.as_str(),
                "gsm.network-id",
                code,
            ]
            .map(String::from),
            QUICK,
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// NetworkManager's rule for an APN: letters, digits, `.`, `_` and `-`, up to
/// 64 characters. An empty APN is allowed; it means no APN.
pub fn apn_error(text: &str) -> Result<(), String> {
    if text.chars().count() > 64 {
        return Err("An access point name has at most 64 characters.".to_string());
    }
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        Ok(())
    } else {
        Err("Use letters, digits, dots, hyphens and underscores only.".to_string())
    }
}

/// A message when `name` cannot be the profile's name.
fn name_error(name: &str, original: &str, names: &[String]) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && name != original && names.iter().any(|other| other == name))
        .then(|| "A connection with this name already exists.".to_string())
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

pub type Profiles = State<Option<Result<Vec<Profile>, CommandError>>>;

/// The profiles, read when a screen that calls this opens and again after every
/// change made here.
pub fn use_profiles() -> Profiles {
    use_hook(|| {
        let state = system::cached::<Result<Vec<Profile>, CommandError>>("mobile-profiles");
        reload(state);
        state
    })
}

pub fn reload(state: Profiles) {
    reload_then(state, || {});
}

/// Read the profiles again, and run `after` once the new list is in place.
pub fn reload_then(state: Profiles, after: impl FnOnce() + 'static) {
    background_then(load_profiles, move |loaded| {
        let mut state = state;
        state.set(Some(loaded));
        after();
    });
}

/// Whether the profiles pin the modem to one carrier (a manual choice).
pub fn pinned(profiles: &[Profile]) -> bool {
    profiles
        .iter()
        .any(|profile| !profile.network_id.is_empty())
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct ApnListPage;

impl Component for ApnListPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let profiles = use_profiles();
        let busy = use_state(|| false);
        use_page_busy(*busy.read());
        let loaded = profiles.read().clone();
        let list = match loaded {
            None => return veiled(page_column().child(PageHead::new()), true),
            Some(Err(error)) => {
                return veiled(
                    page_column().child(PageHead::new()).child(
                        status(Status::Error(error.0))
                            .action("Try again", move |_| reload(profiles)),
                    ),
                    false,
                );
            }
            Some(Ok(list)) => list,
        };
        let connected = ctx
            .overview
            .read()
            .clone()
            .and_then(Result::ok)
            .and_then(|ov| ov.modem_device().map(|device| device.connected()))
            .unwrap_or(false);
        let current = selected(&list).map(|profile| profile.uuid.clone());

        let mut page =
            page_column()
                .child(PageHead::new());
        if list.is_empty() {
            page = page.child(Group::new(None::<String>).row(Element::from(status(
                Status::Empty("No access point names yet.".into()),
            ))));
        } else {
            let rows = list
                .iter()
                .map(|profile| {
                    let chosen = profile.clone();
                    let edit = profile.name.clone();
                    let all = list.clone();
                    radio_row(
                        profile.name.clone(),
                        current.as_deref() == Some(profile.uuid.as_str()),
                        move |_| {
                            use_profile(ctx, profiles, busy, all.clone(), chosen.clone(), connected)
                        },
                    )
                    .subtitle(profile.summary())
                    .trailing(Element::from(
                        icon_button(Icon::PreferencesSystem, move |_| {
                            ctx.router.open(routes::mobile_apn(Some(&edit.clone())))
                        })
                        .tooltip("Edit access point"),
                    ))
                })
                .collect();
            page = page.child(radio_rows("Access point names".to_string(), rows));
        }
        veiled(
            page.child(
                Group::new(None::<String>).row(
                    row("Add access point…")
                        .navigates()
                        .on_press(move |_| ctx.router.open(routes::mobile_apn(None))),
                ),
            ),
            false,
        )
    }
}

/// Switch the modem to `chosen`.
fn use_profile(
    ctx: NetCtx,
    profiles: Profiles,
    busy: State<bool>,
    all: Vec<Profile>,
    chosen: Profile,
    connected: bool,
) {
    let mut busy = busy;
    busy.set(true);
    background_then(
        move || choose(&all, &chosen, connected),
        move |result| {
            let mut busy = busy;
            busy.set(false);
            if let Err(error) = result {
                ctx.overlays.toast(error.0);
            }
            reload(profiles);
            ctx.reload();
        },
    );
}

// ---------------------------------------------------------------------------
// The editor
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct ApnPage {
    pub name: Option<String>,
}

type Loaded = State<Option<Result<Editing, CommandError>>>;

impl Component for ApnPage {
    fn render(&self) -> impl IntoElement {
        let name = self.name.clone();
        let loaded: Loaded = use_state(|| None);
        let load = move || {
            let name = name.clone();
            background(loaded, move || load_editing(name));
        };
        let first = load.clone();
        use_hook(move || first());
        let current = loaded.read().clone();
        // Every branch draws through `veiled`, so the number of hooks stays
        // the same while the profile loads.
        match current {
            None => veiled(page_column().child(PageHead::new()), true),
            Some(Err(error)) => veiled(
                page_column()
                    .child(PageHead::new())
                    .child(status(Status::Error(error.0)).action("Try again", move |_| load())),
                false,
            ),
            Some(Ok(editing)) => veiled(
                page_column().child(ApnForm {
                    initial: editing.profile.clone().unwrap_or_else(Profile::blank),
                    existing: editing.profile.is_some(),
                    names: editing.names,
                    loaded,
                }),
                false,
            ),
        }
    }
}

#[derive(PartialEq)]
struct ApnForm {
    /// The profile when the page opened; also what Revert goes back to.
    initial: Profile,
    existing: bool,
    names: Vec<String>,
    loaded: Loaded,
}

impl PartialEq for Editing {
    fn eq(&self, other: &Self) -> bool {
        self.profile == other.profile && self.names == other.names
    }
}

impl Component for ApnForm {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let profiles = use_profiles();
        let busy = use_state(|| false);
        use_page_busy(*busy.read());
        let seed = Draft::of(&self.initial);
        let name = use_state(|| seed.name.clone());
        let apn = use_state(|| seed.apn.clone());
        let user = use_state(|| seed.user.clone());
        let password = use_state(|| seed.password.clone());
        let auto_config = use_state(|| seed.auto_config);
        let auth = use_state(|| seed.auth.key().to_string());
        let ip = use_state(|| seed.ip.key().to_string());

        let draft = Draft {
            name: name.read().clone(),
            apn: apn.read().clone(),
            user: user.read().clone(),
            password: password.read().clone(),
            auto_config: *auto_config.read(),
            auth: Auth::from_key(&auth.read()).unwrap_or(self.initial.auth),
            ip: IpType::from_key(&ip.read()),
        };
        let name_clash = name_error(&draft.name, &self.initial.name, &self.names);
        let valid = !draft.name.trim().is_empty()
            && name_clash.is_none()
            && (draft.auto_config || apn_error(&draft.apn).is_ok());
        let dirty = draft != seed;
        // Whether a character is allowed never depends on what follows it, so
        // a wrong one is called out as soon as it is typed.
        let apn_message = (draft.apn != seed.apn)
            .then(|| apn_error(&draft.apn).err())
            .flatten();

        let initial = self.initial.clone();
        let existing = self.existing;
        let loaded = self.loaded;
        let apply = {
            let initial = initial.clone();
            let draft = draft.clone();
            let all = profiles;
            move |_| {
                let mut busy = busy;
                busy.set(true);
                let before = initial.clone();
                let refreshed = initial.name.clone();
                let next = draft.clone();
                let others_connect = all
                    .read()
                    .clone()
                    .and_then(Result::ok)
                    .is_some_and(|list| list.iter().any(|profile| profile.autoconnect));
                let renamed = existing && next.name.trim() != before.name;
                background_then(
                    move || {
                        if existing {
                            modify(&before, &next)
                        } else {
                            create(&next, !others_connect)
                        }
                    },
                    move |result| {
                        let mut busy = busy;
                        busy.set(false);
                        match result {
                            Err(error) => ctx.overlays.toast(error.0),
                            Ok(()) => {
                                reload(profiles);
                                ctx.reload();
                                // A new profile, or one that now has another
                                // name (the address of this page is its name),
                                // goes back to the list.
                                if !existing || renamed {
                                    ctx.router.pop();
                                } else {
                                    background(loaded, move || load_editing(Some(refreshed)));
                                }
                            }
                        }
                    },
                );
            }
        };
        let revert = {
            let saved = seed.clone();
            move |_| {
                let (mut name, mut apn, mut user, mut password) = (name, apn, user, password);
                let (mut auto_config, mut auth, mut ip) = (auto_config, auth, ip);
                name.set(saved.name.clone());
                apn.set(saved.apn.clone());
                user.set(saved.user.clone());
                password.set(saved.password.clone());
                auto_config.set(saved.auto_config);
                auth.set(saved.auth.key().to_string());
                ip.set(saved.ip.key().to_string());
            }
        };

        let mut name_field = form_field("Name", name)
            .placeholder("Carrier")
            .error(name_clash);
        if !existing {
            name_field = name_field.autofocus();
        }
        let mut general = Group::new(None::<String>).row(name_field);
        // Detection can be chosen for a new profile, and turned off for one
        // that has it. A profile with its own settings cannot go back: see
        // `Draft::properties`.
        if !existing || self.initial.auto_config {
            general = general.row(
                switch_row(
                    "Detect settings automatically",
                    draft.auto_config,
                    move |on: bool| {
                        let mut auto_config = auto_config;
                        auto_config.set(on);
                    },
                ),
            );
        }

        let mut page = page_column()
            .child(PageHead::new())
            .child(pending_bar(
                existing && dirty,
                valid,
                apply.clone(),
                revert,
            ))
            .child(general);

        if !draft.auto_config {
            page = page.child(
                Group::titled("Access point")
                    .row(
                        form_field("APN", apn)
                            .placeholder("internet")
                            .hint("Access point name. Empty means none.")
                            .error(apn_message),
                    )
                    .row(form_field("User name", user).optional())
                    .row(form_field("Password", password).password().optional())
                    .row(choice_row(
                        "Authentication",
                        Auth::options(draft.auth),
                        draft.auth.key(),
                        move |key: String| {
                            let mut auth = auth;
                            auth.set(key);
                        },
                    )),
            );
        }
        page = page.child(
            Group::new(None::<String>).row(choice_row(
                "IP type",
                [IpType::V4, IpType::V6, IpType::V4V6]
                    .into_iter()
                    .map(|kind| (kind.key().to_string(), kind.label().to_string()))
                    .collect(),
                draft.ip.key(),
                move |key: String| {
                    let mut ip = ip;
                    ip.set(key);
                },
            )),
        );

        if existing {
            let delete = self.initial.clone();
            page = page.child(
                Group::new(None::<String>).row(
                    row("Delete access point")
                        .destructive()
                        .on_press(move |_| confirm_delete(ctx, profiles, delete.clone())),
                ),
            );
        } else {
            page = page.child(Group::new(None::<String>).row(action_row(
                None,
                primary_button("Add access point", apply).enabled(valid && dirty),
            )));
        }
        page
    }
}

fn confirm_delete(ctx: NetCtx, profiles: Profiles, profile: Profile) {
    let title = format!("Delete \"{}\"?", profile.name);
    ctx.overlays.show_dialog(
        dialog(
            title,
            "Mobile data stops working if no other access point is left.",
        )
        .cancel("Cancel")
        .default_action(CANCEL_ID)
        .response("Delete", ResponseKind::Destructive, move |_| {
            let gone = profile.clone();
            background_then(
                move || delete(&gone),
                move |result| match result {
                    Ok(()) => {
                        reload(profiles);
                        ctx.reload();
                        ctx.router.pop();
                    }
                    Err(error) => ctx.overlays.toast(error.0),
                },
            );
        }),
    );
}

// ---------------------------------------------------------------------------
// Snapshot stand-in
// ---------------------------------------------------------------------------

/// Two profiles as `nmcli` prints them, for `BINGUX_SETTINGS_FAKE_MODEM`.
#[cfg(any(test, feature = "snapshot"))]
pub mod fixtures {
    use super::*;

    pub const VODAFONE: &str = "connection.id:Vodafone\n\
connection.uuid:6a1c2f2e-4b0d-4a39-9d6e-0f3c8d1a7b01\n\
connection.autoconnect:yes\n\
gsm.apn:internet\n\
gsm.username:web\n\
gsm.home-only:no\n\
gsm.network-id:\n\
gsm.auto-config:no\n\
ppp.refuse-pap:no\n\
ppp.refuse-chap:no\n\
ppp.refuse-mschap:no\n\
ppp.refuse-mschapv2:no\n\
ppp.refuse-eap:no\n\
ipv4.method:auto\n\
ipv6.method:auto\n\
GENERAL.STATE:activated\n";

    pub const WORK: &str = "connection.id:Work private APN\n\
connection.uuid:0b7e5d3c-91a4-4c52-8f1e-2d6a9c4e8f02\n\
connection.autoconnect:no\n\
gsm.apn:corp.example.net\n\
gsm.username:fieldwork\n\
gsm.home-only:no\n\
gsm.network-id:\n\
gsm.auto-config:no\n\
ppp.refuse-pap:no\n\
ppp.refuse-chap:no\n\
ppp.refuse-mschap:yes\n\
ppp.refuse-mschapv2:yes\n\
ppp.refuse-eap:yes\n\
ipv4.method:auto\n\
ipv6.method:disabled\n\
GENERAL.STATE:\n";

    #[cfg(feature = "snapshot")]
    pub fn profiles() -> Vec<Profile> {
        vec![parse_profile(VODAFONE), parse_profile(WORK)]
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    const LIST: &str = "Vodafone:6a1c2f2e-4b0d-4a39-9d6e-0f3c8d1a7b01:gsm\n\
Work private APN:0b7e5d3c-91a4-4c52-8f1e-2d6a9c4e8f02:gsm\n\
Wired connection 1:53e2ef18-ad77-339e-96ee-e3efc38648f0:802-3-ethernet\n";

    #[test]
    fn profile_fields() {
        let profile = parse_profile(VODAFONE);
        assert_eq!(profile.name, "Vodafone");
        assert_eq!(profile.apn, "internet");
        assert_eq!(profile.user, "web");
        assert!(profile.autoconnect && profile.active && !profile.home_only);
        assert_eq!(profile.auth, Auth::AUTOMATIC);
        assert_eq!(profile.ip, IpType::V4V6);
        assert_eq!(profile.summary(), "internet");
    }

    #[test]
    fn restricted_authentication_and_one_family() {
        let profile = parse_profile(WORK);
        assert_eq!(profile.auth.key(), "pap-chap");
        assert_eq!(profile.ip, IpType::V4);
        assert!(!profile.active && !profile.autoconnect);
    }

    #[test]
    fn passwords_may_hold_colons_and_backslashes() {
        let profile = parse_profile("gsm.password:p:ass\\w\ngsm.username:u\n");
        assert_eq!(profile.password, "p:ass\\w");
    }

    #[test]
    fn only_gsm_profiles_are_listed() {
        let list = parse_gsm_list(LIST);
        assert_eq!(
            list,
            vec![
                (
                    "Vodafone".into(),
                    "6a1c2f2e-4b0d-4a39-9d6e-0f3c8d1a7b01".into()
                ),
                (
                    "Work private APN".into(),
                    "0b7e5d3c-91a4-4c52-8f1e-2d6a9c4e8f02".into()
                ),
            ]
        );
        assert!(parse_gsm_list("").is_empty());
    }

    #[test]
    fn escaped_names_in_the_list() {
        let list = parse_gsm_list("Carrier\\: one:1111:gsm\n");
        assert_eq!(list[0].0, "Carrier: one");
    }

    #[test]
    fn the_selected_profile_is_the_active_one() {
        let mut list = vec![parse_profile(VODAFONE), parse_profile(WORK)];
        assert_eq!(selected(&list).unwrap().name, "Vodafone");
        list[0].active = false;
        list[0].autoconnect = false;
        list[1].autoconnect = true;
        assert_eq!(selected(&list).unwrap().name, "Work private APN");
        list[1].autoconnect = false;
        assert!(selected(&list).is_none());
    }

    #[test]
    fn authentication_keys_round_trip() {
        for key in [
            "auto", "none", "pap", "chap", "pap-chap", "mschap", "mschapv2", "eap",
        ] {
            assert_eq!(Auth::from_key(key).unwrap().key(), key);
        }
        assert_eq!(Auth([true, false, true, false, false]).key(), "custom");
        assert!(Auth::from_key("custom").is_none());
        assert_eq!(Auth::options(Auth::AUTOMATIC).len(), 8);
        assert_eq!(
            Auth::options(Auth([true, false, true, false, false])).len(),
            9
        );
    }

    #[test]
    fn ip_types_map_to_methods() {
        for kind in [IpType::V4, IpType::V6, IpType::V4V6] {
            let (ipv4, ipv6) = kind.methods();
            assert_eq!(IpType::from_methods(ipv4, ipv6), kind);
            assert_eq!(IpType::from_key(kind.key()), kind);
        }
        assert_eq!(IpType::from_methods("auto", "ignore"), IpType::V4);
        assert_eq!(IpType::from_methods("disabled", "disabled"), IpType::V4);
    }

    #[test]
    fn apn_rules_follow_networkmanager() {
        for good in [
            "",
            "internet",
            "web.vodafone.de",
            "good_apn.x-1",
            "UPPER",
            &"x".repeat(64),
        ] {
            assert!(apn_error(good).is_ok(), "{good:?}");
        }
        for bad in ["bad apn", "a:b", "ä", "x/y", &"x".repeat(65)] {
            assert!(apn_error(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn names_must_be_new() {
        let names = vec!["Vodafone".to_string(), "Home".to_string()];
        assert!(name_error("Vodafone", "Vodafone", &names).is_none());
        assert!(name_error("Vodafone", "Other", &names).is_some());
        assert!(name_error(" Home ", "Vodafone", &names).is_some());
        assert!(name_error("Fresh", "Vodafone", &names).is_none());
        assert!(name_error("", "Vodafone", &names).is_none());
    }

    #[test]
    fn draft_becomes_properties() {
        let mut draft = Draft::of(&parse_profile(WORK));
        draft.password = "pw".into();
        let properties = draft.properties();
        let get = |key: &str| {
            let index = properties.iter().position(|item| item == key).unwrap();
            properties[index + 1].clone()
        };
        assert_eq!(get("gsm.apn"), "corp.example.net");
        assert_eq!(get("gsm.password"), "pw");
        assert_eq!(get("gsm.password-flags"), "0");
        assert_eq!(get("ppp.refuse-pap"), "no");
        assert_eq!(get("ppp.refuse-mschap"), "yes");
        assert_eq!(get("ipv4.method"), "auto");
        assert_eq!(get("ipv6.method"), "disabled");
        // A detected profile sends no settings of its own.
        draft.auto_config = true;
        let detected = draft.properties();
        assert!(!detected.iter().any(|item| item.starts_with("gsm.apn")));
        assert!(!detected.iter().any(|item| item.starts_with("gsm.pass")));
        assert_eq!(detected[0], "gsm.auto-config");
        assert_eq!(detected[1], "yes");
    }
}
