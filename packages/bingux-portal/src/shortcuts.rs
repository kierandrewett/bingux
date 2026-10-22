//! The `org.gnoblin.Shell.GlobalShortcutsProvider` owner.
//!
//! The Gnoblin portal backend calls this provider to approve and configure
//! global shortcuts. The provider asks the shell for the approval UI through
//! `binguxctl ipc shortcuts`, keeps the approved grants in the user's config,
//! and tells the backend when saved bindings change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::json;
use zbus::fdo;
use zbus::message::Header;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, interface};

use crate::binguxctl::{Ctl, parse_request_reply};
use crate::identity;
use crate::screenshot::{MAX_CONSECUTIVE_FAILURES, POLL_INTERVAL};
use crate::store;

/// The bus name and object path that the Gnoblin portal backend calls.
pub const PROVIDER_NAME: &str = "org.gnoblin.Shell.GlobalShortcutsProvider";
pub const PROVIDER_PATH: &str = "/org/gnoblin/Shell/GlobalShortcutsProvider";
/// The Gnoblin portal backend. It is the only caller allowed to use the provider.
pub const BACKEND_NAME: &str = "org.freedesktop.impl.portal.desktop.gnoblin";
/// The backend object and interface that apply saved binding changes.
const REBIND_PATH: &str = "/org/gnoblin/Shell/GlobalShortcuts";
const REBIND_INTERFACE: &str = "org.gnoblin.Shell.GlobalShortcutsRebind";
const REBIND_METHOD: &str = "RebindShortcuts";
/// The shell IPC target and methods that present the approval UI.
const SHELL_TARGET: &str = "shortcuts";
const REQUEST_METHOD: &str = "requestShortcuts";
const RESULT_METHOD: &str = "requestResult";
const CANCEL_METHOD: &str = "cancelRequest";
const GRANTS_FILE: &str = "global-shortcuts.toml";
const MAX_SHORTCUTS: usize = 64;
const MAX_TEXT: usize = 256;

/// The `a(sa{sv})` list that the portal interfaces use.
pub type ShortcutList = Vec<(String, HashMap<String, OwnedValue>)>;

/// One accelerator the user has allowed for one application's shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub app_id: String,
    pub id: String,
    #[serde(default)]
    pub description: String,
    pub accelerator: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct GrantFile {
    #[serde(default)]
    grant: Vec<Grant>,
}

/// A shortcut that an application asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requested {
    pub id: String,
    pub description: String,
    /// The application's suggested accelerator. Empty when it gave none.
    pub trigger: String,
}

/// An accelerator that the shell reports as approved.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Approval {
    pub id: String,
    pub accelerator: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ShortcutState {
    Pending,
    Approved,
    Denied,
    Cancelled,
    Unknown,
}

#[derive(Deserialize)]
struct ShortcutReply {
    state: ShortcutState,
    #[serde(default)]
    approved: Vec<Approval>,
}

/// What the approval UI decided.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Approved(Vec<Approval>),
    Declined,
}

/// The default grants file: `$XDG_CONFIG_HOME/bingux/global-shortcuts.toml`.
pub fn grants_path() -> PathBuf {
    store::config_file(GRANTS_FILE)
}

fn check_text(text: &str, what: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT {
        return Err(format!("{what} is longer than {MAX_TEXT} bytes"));
    }
    if text.chars().any(char::is_control) {
        return Err(format!("{what} contains a control character"));
    }
    Ok(())
}

fn text_option(options: &HashMap<String, OwnedValue>, key: &str) -> Result<String, String> {
    match options.get(key) {
        None => Ok(String::new()),
        Some(value) => {
            let text = String::try_from(&**value).map_err(|_| format!("{key} must be a string"))?;
            check_text(&text, key)?;
            Ok(text)
        }
    }
}

/// Validates the shortcut list that an application sent.
pub fn parse_requested(shortcuts: &ShortcutList) -> Result<Vec<Requested>, String> {
    if shortcuts.len() > MAX_SHORTCUTS {
        return Err(format!("at most {MAX_SHORTCUTS} shortcuts are allowed"));
    }
    let mut requested: Vec<Requested> = Vec::with_capacity(shortcuts.len());
    for (id, options) in shortcuts {
        if id.is_empty() {
            return Err("shortcut IDs must not be empty".to_owned());
        }
        check_text(id, "shortcut ID")?;
        if requested.iter().any(|entry| entry.id == *id) {
            return Err(format!("shortcut ID {id} is listed twice"));
        }
        requested.push(Requested {
            id: id.clone(),
            description: text_option(options, "description")?,
            trigger: text_option(options, "preferred_trigger")?,
        });
    }
    Ok(requested)
}

/// The grants an approval would cover: one per requested shortcut with a trigger.
pub fn offered_grants(app_id: &str, requested: &[Requested]) -> Vec<Grant> {
    requested
        .iter()
        .filter(|entry| !entry.trigger.is_empty())
        .map(|entry| Grant {
            app_id: app_id.to_owned(),
            id: entry.id.clone(),
            description: entry.description.clone(),
            accelerator: entry.trigger.clone(),
        })
        .collect()
}

fn granted(grants: &[Grant], offer: &Grant) -> bool {
    grants.iter().any(|grant| {
        grant.app_id == offer.app_id
            && grant.id == offer.id
            && grant.accelerator == offer.accelerator
    })
}

/// Whether any offered accelerator still needs the user's approval.
pub fn needs_approval(offered: &[Grant], grants: &[Grant]) -> bool {
    offered.iter().any(|offer| !granted(grants, offer))
}

/// The offered grants that the user approved, in offer order.
pub fn approved_grants(offered: &[Grant], approved: &[Approval]) -> Vec<Grant> {
    offered
        .iter()
        .filter(|offer| {
            approved
                .iter()
                .any(|a| a.id == offer.id && a.accelerator == offer.accelerator)
        })
        .cloned()
        .collect()
}

/// The grants for `app_id`.
pub fn app_grants(grants: &[Grant], app_id: &str) -> Vec<Grant> {
    grants
        .iter()
        .filter(|grant| grant.app_id == app_id)
        .cloned()
        .collect()
}

/// Replaces the grants for `app_id` whose IDs are in `scope` with `approved`.
pub fn replace_grants(
    grants: &mut Vec<Grant>,
    app_id: &str,
    scope: &[String],
    approved: Vec<Grant>,
) {
    grants.retain(|grant| !(grant.app_id == app_id && scope.contains(&grant.id)));
    grants.extend(approved);
}

/// Builds the portal results for `entries` (id, description), in order.
///
/// Each result lists only the granted accelerators. Entries with no grant get
/// an empty `shortcuts` list, which the portal treats as not registered.
pub fn results_for(
    grants: &[Grant],
    app_id: &str,
    entries: &[(String, String)],
) -> fdo::Result<ShortcutList> {
    let mut results = Vec::with_capacity(entries.len());
    for (id, description) in entries {
        let mut options = HashMap::new();
        let mine: Vec<&Grant> = grants
            .iter()
            .filter(|grant| grant.app_id == app_id && grant.id == *id)
            .collect();
        let label = if description.is_empty() {
            mine.first()
                .map(|grant| grant.description.clone())
                .unwrap_or_default()
        } else {
            description.clone()
        };
        if !label.is_empty() {
            options.insert("description".to_owned(), owned(Value::from(label))?);
        }
        let accelerators: Vec<String> =
            mine.iter().map(|grant| grant.accelerator.clone()).collect();
        options.insert("shortcuts".to_owned(), owned(Value::from(accelerators))?);
        results.push((id.clone(), options));
    }
    Ok(results)
}

fn owned(value: Value<'_>) -> fdo::Result<OwnedValue> {
    OwnedValue::try_from(value).map_err(|error| fdo::Error::Failed(error.to_string()))
}

/// Parses the shell's `requestResult` reply.
pub fn parse_decision(text: &str) -> Result<Option<Decision>, String> {
    let reply: ShortcutReply =
        serde_json::from_str(text.trim()).map_err(|e| format!("bad shortcut reply: {e}"))?;
    Ok(match reply.state {
        ShortcutState::Pending => None,
        ShortcutState::Approved => Some(Decision::Approved(reply.approved)),
        ShortcutState::Denied | ShortcutState::Cancelled | ShortcutState::Unknown => {
            Some(Decision::Declined)
        }
    })
}

/// Reads the grants file. A missing file has no grants. An unreadable file is
/// an error, so that a later save cannot overwrite the user's grants.
pub fn load_grants(path: &Path) -> Result<Vec<Grant>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str::<GrantFile>(&text)
            .map(|file| file.grant)
            .map_err(|e| format!("cannot read {}: {e}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

/// Writes the grants file atomically and privately.
pub fn save_grants(path: &Path, grants: &[Grant]) -> Result<(), String> {
    let text = toml::to_string(&GrantFile {
        grant: grants.to_vec(),
    })
    .map_err(|e| format!("cannot encode grants: {e}"))?;
    store::write_private(path, &text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn invalid(message: String) -> fdo::Error {
    fdo::Error::InvalidArgs(message)
}

/// Clears the busy flag when the approval request ends.
struct Busy<'a>(&'a AtomicBool);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The `org.gnoblin.Shell.GlobalShortcutsProvider` interface.
pub struct Provider {
    ctl: Ctl,
    grants_path: PathBuf,
    busy: AtomicBool,
    sequence: AtomicU64,
}

impl Provider {
    pub fn new(ctl: Ctl, grants_path: PathBuf) -> Self {
        Provider {
            ctl,
            grants_path,
            busy: AtomicBool::new(false),
            sequence: AtomicU64::new(0),
        }
    }

    fn next_id(&self) -> String {
        let n = self.sequence.fetch_add(1, Ordering::SeqCst);
        format!("gs-{}-{n}", std::process::id())
    }

    fn acquire(&self) -> fdo::Result<Busy<'_>> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(fdo::Error::AccessDenied(
                "another shortcut approval is already open".to_owned(),
            ));
        }
        Ok(Busy(&self.busy))
    }

    fn load(&self) -> fdo::Result<Vec<Grant>> {
        load_grants(&self.grants_path).map_err(fdo::Error::Failed)
    }

    fn save(&self, grants: &[Grant]) -> fdo::Result<()> {
        save_grants(&self.grants_path, grants).map_err(fdo::Error::Failed)
    }

    /// Opens the approval UI with `payload` and waits for the user.
    ///
    /// The request is cancelled and treated as declined when the backend
    /// `sender` stops owning its bus name while the user is deciding.
    async fn ask(
        &self,
        connection: &Connection,
        sender: &str,
        mode: &str,
        payload: serde_json::Value,
    ) -> fdo::Result<Decision> {
        let _busy = self.acquire()?;
        let id = self.next_id();
        let request = json!({ "mode": mode, "request": payload }).to_string();
        let reply = self
            .ctl
            .ipc(SHELL_TARGET, REQUEST_METHOD, &[&id, &request])
            .await
            .map_err(fdo::Error::Failed)?;
        parse_request_reply(&reply).map_err(fdo::Error::Failed)?;

        let mut failures = 0;
        loop {
            async_io::Timer::after(POLL_INTERVAL).await;
            if !identity::owner_is(connection, sender, BACKEND_NAME).await {
                let _ = self.ctl.ipc(SHELL_TARGET, CANCEL_METHOD, &[&id]).await;
                return Ok(Decision::Declined);
            }
            match self
                .ctl
                .ipc(SHELL_TARGET, RESULT_METHOD, &[&id])
                .await
                .and_then(|text| parse_decision(&text))
            {
                Ok(Some(decision)) => return Ok(decision),
                Ok(None) => failures = 0,
                Err(message) => {
                    failures += 1;
                    if failures >= MAX_CONSECUTIVE_FAILURES {
                        let _ = self.ctl.ipc(SHELL_TARGET, CANCEL_METHOD, &[&id]).await;
                        return Err(fdo::Error::Failed(message));
                    }
                }
            }
        }
    }
}

/// Returns the sender's unique name when it owns the Gnoblin backend name.
async fn authorize(connection: &Connection, header: &Header<'_>) -> fdo::Result<String> {
    let Some(sender) = header.sender().map(|name| name.to_string()) else {
        return Err(fdo::Error::AccessDenied(
            "the call has no sender".to_owned(),
        ));
    };
    if identity::owner_is(connection, &sender, BACKEND_NAME).await {
        Ok(sender)
    } else {
        Err(fdo::Error::AccessDenied(
            "only the Gnoblin portal backend may use the shortcut provider".to_owned(),
        ))
    }
}

/// Tells the Gnoblin backend about changed saved bindings.
async fn rebind(connection: &Connection, app_id: &str, results: &ShortcutList) -> zbus::Result<()> {
    connection
        .call_method(
            Some(BACKEND_NAME),
            REBIND_PATH,
            Some(REBIND_INTERFACE),
            REBIND_METHOD,
            &(app_id, results),
        )
        .await?;
    Ok(())
}

fn offered_entries(offered: &[Grant]) -> serde_json::Value {
    offered
        .iter()
        .map(|grant| {
            json!({
                "id": grant.id,
                "description": grant.description,
                "accelerator": grant.accelerator,
            })
        })
        .collect()
}

#[interface(name = "org.gnoblin.Shell.GlobalShortcutsProvider")]
impl Provider {
    async fn bind_shortcuts(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        app_id: &str,
        _parent_window: &str,
        shortcuts: ShortcutList,
    ) -> fdo::Result<ShortcutList> {
        let sender = authorize(connection, &header).await?;
        check_text(app_id, "app_id").map_err(invalid)?;
        let requested = parse_requested(&shortcuts).map_err(invalid)?;
        let offered = offered_grants(app_id, &requested);
        let mut grants = self.load()?;

        if needs_approval(&offered, &grants) {
            let payload = json!({ "app_id": app_id, "shortcuts": offered_entries(&offered) });
            match self.ask(connection, &sender, "bind", payload).await? {
                Decision::Declined => {
                    return Err(fdo::Error::AccessDenied(
                        "the user did not approve the shortcuts".to_owned(),
                    ));
                }
                Decision::Approved(approved) => {
                    let kept = approved_grants(&offered, &approved);
                    if kept.is_empty() {
                        return Err(fdo::Error::AccessDenied(
                            "the user did not approve the shortcuts".to_owned(),
                        ));
                    }
                    let scope: Vec<String> = requested.iter().map(|r| r.id.clone()).collect();
                    replace_grants(&mut grants, app_id, &scope, kept);
                    self.save(&grants)?;
                }
            }
        }

        let entries: Vec<(String, String)> = requested
            .into_iter()
            .map(|r| (r.id, r.description))
            .collect();
        results_for(&grants, app_id, &entries)
    }

    async fn configure_shortcuts(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        app_id: &str,
        _parent_window: &str,
    ) -> fdo::Result<ShortcutList> {
        let sender = authorize(connection, &header).await?;
        check_text(app_id, "app_id").map_err(invalid)?;
        let mut grants = self.load()?;
        let offered = app_grants(&grants, app_id);
        if offered.is_empty() {
            return Ok(Vec::new());
        }

        let payload = json!({ "app_id": app_id, "shortcuts": offered_entries(&offered) });
        if let Decision::Approved(approved) =
            self.ask(connection, &sender, "configure", payload).await?
        {
            let kept = approved_grants(&offered, &approved);
            if kept != offered {
                let scope: Vec<String> = offered.iter().map(|g| g.id.clone()).collect();
                replace_grants(&mut grants, app_id, &scope, kept);
                self.save(&grants)?;
                let entries: Vec<(String, String)> = offered
                    .iter()
                    .map(|g| (g.id.clone(), g.description.clone()))
                    .collect();
                let results = results_for(&grants, app_id, &entries)?;
                // The grants are saved. A failed rebind only delays the change until
                // the backend next binds, so it is logged, not returned as an error.
                if let Err(error) = rebind(connection, app_id, &results).await {
                    eprintln!("bingux-global-shortcuts: cannot apply saved bindings: {error}");
                }
            }
        }

        let entries: Vec<(String, String)> = app_grants(&grants, app_id)
            .into_iter()
            .map(|g| (g.id, g.description))
            .collect();
        results_for(&grants, app_id, &entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(app: &str, id: &str, accelerator: &str) -> Grant {
        Grant {
            app_id: app.to_owned(),
            id: id.to_owned(),
            description: format!("{id} label"),
            accelerator: accelerator.to_owned(),
        }
    }

    fn requested(id: &str, trigger: &str) -> Requested {
        Requested {
            id: id.to_owned(),
            description: format!("{id} label"),
            trigger: trigger.to_owned(),
        }
    }

    fn entry(id: &str, trigger: Option<&str>) -> (String, HashMap<String, OwnedValue>) {
        let mut options = HashMap::new();
        options.insert(
            "description".to_owned(),
            owned(Value::from(format!("{id} label"))).unwrap(),
        );
        if let Some(trigger) = trigger {
            options.insert(
                "preferred_trigger".to_owned(),
                owned(Value::from(trigger.to_owned())).unwrap(),
            );
        }
        (id.to_owned(), options)
    }

    #[test]
    fn requested_shortcuts_parse_and_reject_bad_input() {
        let parsed =
            parse_requested(&vec![entry("open", Some("<Super>o")), entry("noop", None)]).unwrap();
        assert_eq!(parsed[0], requested("open", "<Super>o"));
        assert_eq!(parsed[1], requested("noop", ""));
        assert!(parse_requested(&vec![entry("", None)]).is_err());
        assert!(parse_requested(&vec![entry("a", None), entry("a", None)]).is_err());
        assert!(parse_requested(&vec![entry("a\n", None)]).is_err());
        let long = "x".repeat(MAX_TEXT + 1);
        assert!(parse_requested(&vec![entry(&long, None)]).is_err());
        let many: ShortcutList = (0..=MAX_SHORTCUTS)
            .map(|n| entry(&format!("s{n}"), None))
            .collect();
        assert!(parse_requested(&many).is_err());
    }

    #[test]
    fn approval_is_needed_only_for_ungranted_triggers() {
        let offered = offered_grants(
            "app",
            &[requested("open", "<Super>o"), requested("noop", "")],
        );
        assert_eq!(offered, vec![grant("app", "open", "<Super>o")]);
        assert!(needs_approval(&offered, &[]));
        assert!(!needs_approval(
            &offered,
            &[grant("app", "open", "<Super>o")]
        ));
        assert!(needs_approval(&offered, &[grant("app", "open", "<Alt>o")]));
        assert!(!needs_approval(&[], &[]));
    }

    #[test]
    fn only_offered_accelerators_are_approved() {
        let offered = vec![
            grant("app", "open", "<Super>o"),
            grant("app", "run", "<Super>r"),
        ];
        let approved = vec![
            Approval {
                id: "open".into(),
                accelerator: "<Super>o".into(),
            },
            Approval {
                id: "run".into(),
                accelerator: "<Ctrl>x".into(),
            },
            Approval {
                id: "other".into(),
                accelerator: "<Super>o".into(),
            },
        ];
        assert_eq!(
            approved_grants(&offered, &approved),
            vec![grant("app", "open", "<Super>o")]
        );
    }

    #[test]
    fn replace_touches_only_the_scope_of_one_app() {
        let mut grants = vec![
            grant("app", "open", "<Super>o"),
            grant("app", "keep", "<Super>k"),
            grant("other", "open", "<Super>o"),
        ];
        replace_grants(
            &mut grants,
            "app",
            &["open".to_owned()],
            vec![grant("app", "open", "<Alt>o")],
        );
        assert_eq!(
            grants,
            vec![
                grant("app", "keep", "<Super>k"),
                grant("other", "open", "<Super>o"),
                grant("app", "open", "<Alt>o"),
            ]
        );
    }

    #[test]
    fn results_list_only_granted_accelerators() {
        let grants = vec![
            grant("app", "open", "<Super>o"),
            grant("app", "open", "<Alt>o"),
        ];
        let results = results_for(
            &grants,
            "app",
            &[
                ("open".to_owned(), String::new()),
                ("idle".to_owned(), String::new()),
            ],
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        let accelerators = |index: usize| -> Vec<String> {
            let value: &Value = &results[index].1["shortcuts"];
            Vec::<String>::try_from(value.clone()).unwrap()
        };
        assert_eq!(
            accelerators(0),
            vec!["<Super>o".to_owned(), "<Alt>o".to_owned()]
        );
        assert!(accelerators(1).is_empty());
        let description: &Value = &results[0].1["description"];
        assert_eq!(String::try_from(description).unwrap(), "open label");
    }

    #[test]
    fn decisions_parse_every_state() {
        assert_eq!(parse_decision(r#"{"id":"a","state":"pending"}"#), Ok(None));
        assert_eq!(
            parse_decision(
                r#"{"id":"a","state":"approved","approved":[{"id":"open","accelerator":"<Super>o"}]}"#
            ),
            Ok(Some(Decision::Approved(vec![Approval {
                id: "open".into(),
                accelerator: "<Super>o".into(),
            }])))
        );
        for state in ["denied", "cancelled", "unknown"] {
            let json = format!(r#"{{"id":"a","state":"{state}"}}"#);
            assert_eq!(parse_decision(&json), Ok(Some(Decision::Declined)));
        }
        assert!(parse_decision(r#"{"id":"a","state":"bogus"}"#).is_err());
        assert!(parse_decision("not json").is_err());
    }

    #[test]
    fn grants_round_trip_and_unreadable_files_are_errors() {
        let dir = std::env::temp_dir().join(format!("bingux-gs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(GRANTS_FILE);
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_grants(&path), Ok(Vec::new()));
        let grants = vec![grant("app", "open", "<Super>o")];
        save_grants(&path, &grants).unwrap();
        assert_eq!(load_grants(&path), Ok(grants));
        std::fs::write(&path, "not = [valid").unwrap();
        assert!(load_grants(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
