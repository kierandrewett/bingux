//! Which accounts the greeter offers.
//!
//! The list comes from the system password database through NSS (`getpwent`, so LDAP,
//! SSSD and systemd-userdbd accounts count when the system enumerates them), enriched by
//! AccountsService over the system bus (real name, icon, locked, "system account", local),
//! with `/var/lib/AccountsService/users/<name>` as the fallback when the bus has nothing.
//! Accounts AccountsService knows but NSS did not enumerate (a cached domain user) are
//! looked up by uid and added.
//!
//! An account is excluded when it is:
//!
//! * reserved: uid 0, `nobody` (65534), or the "no uid" values;
//! * a system account: a local account outside `UID_MIN..=UID_MAX` of `/etc/login.defs`
//!   (1000..=60000 when the file does not say). A non-local account (a domain user) is
//!   only required to be at or above `UID_MIN`, because directories hand out uids in
//!   their own, much higher, ranges;
//! * unable to log in: the shell is `nologin`, `false`, `sync`, `halt` or `shutdown`;
//! * hidden: AccountsService says `SystemAccount`, or the name or `uid:N` is in
//!   `/etc/bsm/hidden-users`;
//! * disabled: `/etc/shadow` (when readable) gives an expiry date that has passed.
//!
//! A locked password does not exclude an account (fingerprint or smart card logins
//! still work); it is reported in [`Account::locked`] for the caller's policy.

use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use zbus::blocking::Connection;
use zbus::proxy;
use zbus::zvariant::OwnedObjectPath;

use crate::publish::ACCOUNTS_SERVICE_ICONS;

// Logic reused, with attribution, from `packages/bingux-session/src/accounts.rs`:
// the `UID_MIN`/`UID_MAX` parsing, the nologin shell list and the GECOS display name rule.
const DEFAULT_UID_MIN: u32 = 1000;
const DEFAULT_UID_MAX: u32 = 60000;
const NOBODY: u32 = 65534;
/// Longest display name published to the greeter, in characters.
pub const MAX_DISPLAY_NAME_CHARS: usize = 128;
const MAX_NAME_BYTES: usize = 256;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountSource {
    /// Enriched by AccountsService over D-Bus.
    AccountsService,
    /// Password database only (plus AccountsService's files on disk, if any).
    PasswdOnly,
}

/// One account the greeter may show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub uid: u32,
    pub gid: u32,
    pub user_name: String,
    pub display_name: String,
    /// AccountsService `IconFile`, unvalidated. The publisher only opens it below trusted roots.
    pub icon_file: Option<PathBuf>,
    pub home: PathBuf,
    pub shell: String,
    /// The password is locked. Unknown (false) when `/etc/shadow` is unreadable.
    pub locked: bool,
    pub local: bool,
    pub source: AccountSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    Reserved,
    SystemUid,
    NoLoginShell,
    SystemAccount,
    Hidden,
    Disabled,
    InvalidName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub account: Account,
    pub exclusion: Option<Exclusion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswdSource {
    /// `getpwent` and `getpwuid_r`: whatever NSS provides.
    Nss,
    /// A passwd(5) file, for tests and containers.
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct AccountPolicy {
    pub passwd: PasswdSource,
    /// The local passwd file, to tell local accounts from directory ones in NSS mode.
    pub local_passwd: PathBuf,
    pub shadow: PathBuf,
    pub login_defs: PathBuf,
    pub hidden_users: PathBuf,
    /// Names to hide in addition to the file, for configuration that lives elsewhere.
    pub extra_hidden: Vec<String>,
    pub accounts_service_users: PathBuf,
    pub accounts_service_icons: PathBuf,
    /// Ask AccountsService on the system bus. Off for fixtures.
    pub use_accounts_service: bool,
    /// Days since the epoch used for shadow expiry; `None` is today.
    pub today_days: Option<i64>,
}

impl AccountPolicy {
    /// The real system.
    pub fn system() -> Self {
        Self {
            passwd: PasswdSource::Nss,
            local_passwd: "/etc/passwd".into(),
            shadow: "/etc/shadow".into(),
            login_defs: "/etc/login.defs".into(),
            hidden_users: "/etc/bsm/hidden-users".into(),
            extra_hidden: Vec::new(),
            accounts_service_users: "/var/lib/AccountsService/users".into(),
            accounts_service_icons: ACCOUNTS_SERVICE_ICONS.into(),
            use_accounts_service: true,
            today_days: None,
        }
    }

    /// A fixture tree laid out like `/` under `root`, with the bus switched off.
    pub fn under(root: &Path) -> Self {
        Self {
            passwd: PasswdSource::File(root.join("etc/passwd")),
            local_passwd: root.join("etc/passwd"),
            shadow: root.join("etc/shadow"),
            login_defs: root.join("etc/login.defs"),
            hidden_users: root.join("etc/bsm/hidden-users"),
            extra_hidden: Vec::new(),
            accounts_service_users: root.join("var/lib/AccountsService/users"),
            accounts_service_icons: root.join("var/lib/AccountsService/icons"),
            use_accounts_service: false,
            today_days: None,
        }
    }
}

#[derive(Debug)]
pub enum AccountsError {
    /// The configured passwd file could not be read.
    Passwd(std::io::Error),
}

impl fmt::Display for AccountsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passwd(error) => write!(f, "cannot read the passwd file: {error}"),
        }
    }
}

impl std::error::Error for AccountsError {}

/// One passwd(5) entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswdEntry {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub gecos: String,
    pub home: String,
    pub shell: String,
}

/// What AccountsService says about one account, from the bus or from its state files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountsServiceUser {
    pub uid: u32,
    pub user_name: String,
    pub real_name: String,
    pub icon_file: String,
    pub locked: Option<bool>,
    pub system_account: bool,
    pub local: Option<bool>,
}

/// Account listing with the reason for each exclusion and what AccountsService did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub candidates: Vec<Candidate>,
    /// `None` when AccountsService answered; otherwise why it did not (fixture mode: `Ok`-less).
    pub accounts_service_error: Option<String>,
}

impl Evaluation {
    pub fn eligible(&self) -> Vec<Account> {
        let mut accounts: Vec<Account> =
            self.candidates.iter().filter(|candidate| candidate.exclusion.is_none()).map(|candidate| candidate.account.clone()).collect();
        accounts.sort_by_key(|account| (account.display_name.to_lowercase(), account.uid));
        accounts
    }
}

/// The accounts the greeter offers, sorted by display name.
pub fn list_accounts(policy: &AccountPolicy) -> Result<Vec<Account>, AccountsError> {
    Ok(evaluate(policy)?.eligible())
}

/// Every account the database knows, each with its exclusion reason, if any.
pub fn evaluate(policy: &AccountPolicy) -> Result<Evaluation, AccountsError> {
    let entries = match &policy.passwd {
        PasswdSource::Nss => nss_entries(),
        PasswdSource::File(path) => parse_passwd(&read_limited(path).map_err(AccountsError::Passwd)?),
    };
    let (bus_users, accounts_service_error) = if policy.use_accounts_service {
        match query_accounts_service() {
            Ok(users) => (users, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        }
    } else {
        (Vec::new(), None)
    };
    let bus_available = policy.use_accounts_service && accounts_service_error.is_none();
    let mut entries = entries;
    if bus_available {
        // Cached directory users that NSS did not enumerate.
        let known: HashSet<u32> = entries.iter().map(|entry| entry.uid).collect();
        for user in bus_users.iter().filter(|user| !known.contains(&user.uid)) {
            let entry = match &policy.passwd {
                PasswdSource::Nss => nss_lookup_uid(user.uid),
                PasswdSource::File(_) => None,
            };
            entries.extend(entry);
        }
    }
    let local_names = local_names(policy);
    let shadow = read_shadow(policy);
    let hidden = read_hidden(policy);
    let (uid_min, uid_max) = login_uid_range(&policy.login_defs);
    let today = policy.today_days.unwrap_or_else(today_days);
    let bus_by_uid: HashMap<u32, &AccountsServiceUser> = bus_users.iter().map(|user| (user.uid, user)).collect();

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for entry in entries {
        if !seen.insert(entry.uid) {
            continue; // the first entry of a uid wins, like getpwuid
        }
        let service = bus_by_uid.get(&entry.uid).map(|user| (*user).clone()).or_else(|| read_accounts_service_file(policy, &entry.name));
        let from_bus = bus_by_uid.contains_key(&entry.uid);
        let local = service
            .as_ref()
            .and_then(|user| user.local)
            .unwrap_or_else(|| matches!(policy.passwd, PasswdSource::File(_)) || local_names.contains(&entry.name));
        let shadow_entry = shadow.get(&entry.name);
        let icon_file = service
            .as_ref()
            .map(|user| user.icon_file.clone())
            .filter(|icon| !icon.is_empty())
            .map(PathBuf::from)
            .or_else(|| default_icon(policy, &entry.name));
        let real_name = service.as_ref().map(|user| user.real_name.as_str()).filter(|name| !name.is_empty());
        let account = Account {
            uid: entry.uid,
            gid: entry.gid,
            display_name: display_name(real_name.unwrap_or(&entry.gecos), &entry.name),
            icon_file,
            home: PathBuf::from(&entry.home),
            shell: entry.shell.clone(),
            locked: service.as_ref().and_then(|user| user.locked).or(shadow_entry.map(|shadow| shadow.locked)).unwrap_or(false),
            local,
            source: if from_bus { AccountSource::AccountsService } else { AccountSource::PasswdOnly },
            user_name: entry.name.clone(),
        };
        let exclusion = if !valid_user_name(&entry.name) {
            Some(Exclusion::InvalidName)
        } else if is_reserved(entry.uid) {
            Some(Exclusion::Reserved)
        } else if entry.uid < uid_min || (local && entry.uid > uid_max) {
            Some(Exclusion::SystemUid)
        } else if no_login_shell(&entry.shell) {
            Some(Exclusion::NoLoginShell)
        } else if service.as_ref().is_some_and(|user| user.system_account) {
            Some(Exclusion::SystemAccount)
        } else if hidden.contains(&entry.name) || hidden.contains(&format!("uid:{}", entry.uid)) || policy.extra_hidden.contains(&entry.name) {
            Some(Exclusion::Hidden)
        } else if shadow_entry.and_then(|shadow| shadow.expire_days).is_some_and(|expire| today >= expire) {
            Some(Exclusion::Disabled)
        } else {
            None
        };
        candidates.push(Candidate { account, exclusion });
    }
    Ok(Evaluation { candidates, accounts_service_error })
}

fn is_reserved(uid: u32) -> bool {
    uid == 0 || uid == NOBODY || uid == u32::MAX || uid == u32::MAX - 1
}

/// `nologin`, `false` and the other shells that exist to refuse a login.
pub fn no_login_shell(shell: &str) -> bool {
    let name = shell.rsplit('/').next().unwrap_or(shell);
    matches!(name, "nologin" | "false" | "sync" | "halt" | "shutdown")
}

/// A login name the greeter can show and a path can contain: no control or whitespace
/// characters, no `/`, `:` or NUL, not empty, not too long, not starting with `-` or `.`.
pub fn valid_user_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && !name.starts_with(['-', '.'])
        && !name.chars().any(|c| c.is_control() || c.is_whitespace() || matches!(c, '/' | ':' | '\0'))
}

/// The first GECOS field, or the login name; control characters removed, length capped.
pub fn display_name(gecos: &str, name: &str) -> String {
    let full = gecos.split(',').next().unwrap_or("");
    let cleaned: String = full.chars().filter(|c| !c.is_control()).take(MAX_DISPLAY_NAME_CHARS).collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() { name.to_string() } else { cleaned.to_string() }
}

/// `UID_MIN` and `UID_MAX` from login.defs (from `bingux-session`).
pub fn login_uid_range(login_defs: &Path) -> (u32, u32) {
    let mut range = (DEFAULT_UID_MIN, DEFAULT_UID_MAX);
    let Ok(text) = read_limited(login_defs) else { return range };
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match (parts.next(), parts.next().and_then(|value| value.parse().ok()), parts.next()) {
            (Some("UID_MIN"), Some(value), None) => range.0 = value,
            (Some("UID_MAX"), Some(value), None) => range.1 = value,
            _ => {}
        }
    }
    range
}

fn read_limited(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut text = String::new();
    fs::File::open(path)?.take(MAX_FILE_BYTES).read_to_string(&mut text)?;
    Ok(text)
}

/// passwd(5) text to entries. Malformed lines are skipped.
pub fn parse_passwd(text: &str) -> Vec<PasswdEntry> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 7 {
                return None;
            }
            Some(PasswdEntry {
                name: fields[0].to_string(),
                uid: fields[2].parse().ok()?,
                gid: fields[3].parse().ok()?,
                gecos: fields[4].to_string(),
                home: fields[5].to_string(),
                shell: fields[6].to_string(),
            })
        })
        .collect()
}

fn local_names(policy: &AccountPolicy) -> HashSet<String> {
    read_limited(&policy.local_passwd).map(|text| parse_passwd(&text).into_iter().map(|entry| entry.name).collect()).unwrap_or_default()
}

#[derive(Debug, Clone, Copy)]
struct ShadowEntry {
    locked: bool,
    expire_days: Option<i64>,
}

fn read_shadow(policy: &AccountPolicy) -> HashMap<String, ShadowEntry> {
    let Ok(text) = read_limited(&policy.shadow) else { return HashMap::new() };
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 8 {
                return None;
            }
            let hash = fields[1];
            let locked = hash.starts_with('!') || hash.starts_with('*');
            let expire_days = fields[7].parse::<i64>().ok().filter(|days| *days >= 0);
            Some((fields[0].to_string(), ShadowEntry { locked, expire_days }))
        })
        .collect()
}

fn today_days() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| (elapsed.as_secs() / 86_400) as i64).unwrap_or(0)
}

/// `/etc/bsm/hidden-users`: one account name or `uid:N` per line, `#` starts a comment.
fn read_hidden(policy: &AccountPolicy) -> HashSet<String> {
    let Ok(text) = read_limited(&policy.hidden_users) else { return HashSet::new() };
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// AccountsService's own state file, `<dir>/<name>`, section `[User]`.
fn read_accounts_service_file(policy: &AccountPolicy, name: &str) -> Option<AccountsServiceUser> {
    if !valid_user_name(name) {
        return None;
    }
    let text = read_limited(&policy.accounts_service_users.join(name)).ok()?;
    let mut user = AccountsServiceUser { user_name: name.to_string(), ..AccountsServiceUser::default() };
    let mut in_user = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_user = line == "[User]";
        } else if in_user && let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "SystemAccount" => user.system_account = value.trim() == "true",
                "Icon" => user.icon_file = value.trim().to_string(),
                _ => {}
            }
        }
    }
    Some(user)
}

/// `<icons>/<name>`, AccountsService's copy of the avatar, when it is a plain file.
fn default_icon(policy: &AccountPolicy, name: &str) -> Option<PathBuf> {
    if !valid_user_name(name) {
        return None;
    }
    let path = policy.accounts_service_icons.join(name);
    fs::symlink_metadata(&path).ok().filter(|metadata| metadata.is_file()).map(|_| path)
}

static PWENT_LOCK: Mutex<()> = Mutex::new(());

fn entry_from_passwd(entry: &libc::passwd) -> PasswdEntry {
    let text = |pointer: *const libc::c_char| {
        if pointer.is_null() {
            String::new()
        } else {
            // SAFETY: libc returns NUL terminated strings that stay valid until the next call.
            unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
        }
    };
    PasswdEntry {
        name: text(entry.pw_name),
        uid: entry.pw_uid,
        gid: entry.pw_gid,
        gecos: text(entry.pw_gecos),
        home: text(entry.pw_dir),
        shell: text(entry.pw_shell),
    }
}

/// Every entry NSS enumerates. `getpwent` keeps global state, so callers are serialised.
fn nss_entries() -> Vec<PasswdEntry> {
    let _guard = PWENT_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut entries = Vec::new();
    // SAFETY: the enumeration functions are serialised by PWENT_LOCK and every returned
    // pointer is copied before the next call.
    unsafe {
        libc::setpwent();
        loop {
            let entry = libc::getpwent();
            if entry.is_null() {
                break;
            }
            entries.push(entry_from_passwd(&*entry));
        }
        libc::endpwent();
    }
    entries
}

fn nss_lookup_uid(uid: u32) -> Option<PasswdEntry> {
    let mut buffer = vec![0u8; 16 * 1024];
    // SAFETY: passwd is plain old data that getpwuid_r fills.
    let mut storage: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: all pointers are valid for the sizes given; the result points into `buffer`.
    let status = unsafe { libc::getpwuid_r(uid, &mut storage, buffer.as_mut_ptr().cast(), buffer.len(), &mut result) };
    (status == 0 && !result.is_null()).then(|| entry_from_passwd(&storage))
}

#[proxy(
    interface = "org.freedesktop.Accounts",
    default_service = "org.freedesktop.Accounts",
    default_path = "/org/freedesktop/Accounts"
)]
trait Accounts {
    fn list_cached_users(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(interface = "org.freedesktop.Accounts.User", default_service = "org.freedesktop.Accounts")]
trait AccountsUser {
    #[zbus(property)]
    fn uid(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn user_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn real_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon_file(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn locked(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn system_account(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn local_account(&self) -> zbus::Result<bool>;
}

/// Asks AccountsService for its cached users on the system bus.
pub fn query_accounts_service() -> zbus::Result<Vec<AccountsServiceUser>> {
    let connection = Connection::system()?;
    let accounts = AccountsProxyBlocking::new(&connection)?;
    let mut users = Vec::new();
    for path in accounts.list_cached_users()? {
        let user = AccountsUserProxyBlocking::builder(&connection)
            .path(path)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()?;
        let Ok(uid) = user.uid().map(u32::try_from) else { continue };
        let Ok(uid) = uid else { continue };
        users.push(AccountsServiceUser {
            uid,
            user_name: user.user_name().unwrap_or_default(),
            real_name: user.real_name().unwrap_or_default(),
            icon_file: user.icon_file().unwrap_or_default(),
            locked: user.locked().ok(),
            // A user whose flag cannot be read is treated as a system account: fail closed.
            system_account: user.system_account().unwrap_or(true),
            local: user.local_account().ok(),
        });
    }
    Ok(users)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_rules() {
        assert_eq!(display_name("Alex Doe,Room 1,,", "alex"), "Alex Doe");
        assert_eq!(display_name(",,,", "alex"), "alex");
        assert_eq!(display_name("  ", "alex"), "alex");
        assert_eq!(display_name("Evil\u{1b}[31m Name\n", "alex"), "Evil[31m Name");
        assert_eq!(display_name(&"x".repeat(1000), "alex").chars().count(), MAX_DISPLAY_NAME_CHARS);
    }

    #[test]
    fn user_names_that_cannot_be_trusted_in_a_path() {
        for bad in ["", "a/b", "..", ".hidden", "-rf", "a b", "a:b", "a\nb", &"x".repeat(300)] {
            assert!(!valid_user_name(bad), "{bad:?}");
        }
        for good in ["alex", "alex.doe", "alex_doe-1", "DOMAIN\\alex", "alex@example.com", "_svc"] {
            assert!(valid_user_name(good), "{good:?}");
        }
    }

    #[test]
    fn nologin_shells() {
        for shell in ["/sbin/nologin", "/usr/sbin/nologin", "/bin/false", "/usr/bin/false", "/bin/sync", "/usr/bin/nologin"] {
            assert!(no_login_shell(shell), "{shell}");
        }
        for shell in ["/bin/bash", "/usr/bin/fish", "/bin/zsh", ""] {
            assert!(!no_login_shell(shell), "{shell}");
        }
    }

    #[test]
    fn passwd_parsing_skips_malformed_lines() {
        let entries = parse_passwd("root:x:0:0:root:/root:/bin/bash\nbroken line\n# comment\nalex:x:1000:1000:Alex:/home/alex:/bin/bash\nbad:x:notanumber:1:a:/b:/c\n");
        assert_eq!(entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(), ["root", "alex"]);
    }
}
