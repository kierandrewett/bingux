//! Account discovery against a fixture root laid out like `/`.

use std::fs;
use std::path::{Path, PathBuf};

use bsm_appearance::accounts::{AccountPolicy, Exclusion, evaluate, list_accounts};

const PASSWD: &str = "\
root:x:0:0:root:/root:/bin/bash
daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin
alex:x:1000:1000:Alex Doe,Room 1,,:/home/alex:/bin/bash
bob:x:1001:1001:Bob:/home/bob:/usr/sbin/nologin
carol:x:1002:1002:Carol:/home/carol:/bin/zsh
dave:x:1003:1003:Dave:/home/dave:/bin/bash
erin:x:1004:1004:Erin:/home/erin:/bin/bash
frank:x:1005:1005:Frank:/home/frank:/bin/fish
gina:x:1006:1006::/home/gina:/bin/bash
hank:x:1007:1007:Hank:/home/hank:/bin/false
nobody:x:65534:65534:Kernel Overflow User:/:/usr/sbin/nologin
big:x:70000:70000:Too High:/home/big:/bin/bash
svc:x:999:999:Service:/var/lib/svc:/bin/bash
zed:x:1008:1008:Zed:/home/zed:/bin/bash
dup:x:1000:1000:Duplicate Uid:/home/dup:/bin/bash
bad/name:x:1009:1009:Evil:/home/evil:/bin/bash
";

const SHADOW: &str = "\
alex:$6$hash:19000:0:99999:7:::
erin:$6$hash:19000:0:99999:7::100:
frank:!:19000:0:99999:7:::
gina:*:19000:0:99999:7:::
zed:$6$hash:19000:0:99999:7::999999:
";

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let fixture = Self { root };
        fixture.write("etc/passwd", PASSWD);
        fixture.write("etc/shadow", SHADOW);
        fixture.write("etc/login.defs", "# comment\nUID_MIN 1000\nUID_MAX 60000\nSYS_UID_MAX 999\n");
        fixture.write("etc/bsm/hidden-users", "# hidden from the greeter\ncarol   # a trailing comment\nuid:1007\n\n");
        fixture.write("var/lib/AccountsService/users/dave", "[User]\nSystemAccount=true\nIcon=/home/dave/.face\n");
        fixture.write("var/lib/AccountsService/users/alex", "[User]\nIcon=/var/lib/AccountsService/icons/alex\nSystemAccount=false\n");
        fixture.write("var/lib/AccountsService/icons/gina", "icon bytes");
        fixture
    }

    fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.root.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }

    fn policy(&self) -> AccountPolicy {
        let mut policy = AccountPolicy::under(self.root.path());
        policy.today_days = Some(19_500);
        policy
    }
}

fn names(policy: &AccountPolicy) -> Vec<String> {
    list_accounts(policy).unwrap().into_iter().map(|account| account.user_name).collect()
}

#[test]
fn only_real_login_accounts_are_listed() {
    let fixture = Fixture::new();
    // root (reserved), daemon (low uid, nologin), bob (nologin), carol (hidden by name),
    // dave (AccountsService system account), erin (expired), hank (/bin/false, also hidden by uid),
    // nobody, big (above UID_MAX), svc (below UID_MIN), dup (second entry of uid 1000),
    // bad/name (cannot be a path).
    assert_eq!(names(&fixture.policy()), ["alex", "frank", "gina", "zed"]);
}

#[test]
fn every_exclusion_has_its_reason() {
    let fixture = Fixture::new();
    let evaluation = evaluate(&fixture.policy()).unwrap();
    let reason = |name: &str| {
        evaluation.candidates.iter().find(|candidate| candidate.account.user_name == name).unwrap_or_else(|| panic!("{name}")).exclusion
    };
    assert_eq!(reason("root"), Some(Exclusion::Reserved));
    assert_eq!(reason("nobody"), Some(Exclusion::Reserved));
    assert_eq!(reason("daemon"), Some(Exclusion::SystemUid));
    assert_eq!(reason("svc"), Some(Exclusion::SystemUid));
    assert_eq!(reason("big"), Some(Exclusion::SystemUid));
    assert_eq!(reason("bob"), Some(Exclusion::NoLoginShell));
    assert_eq!(reason("hank"), Some(Exclusion::NoLoginShell));
    assert_eq!(reason("dave"), Some(Exclusion::SystemAccount));
    assert_eq!(reason("carol"), Some(Exclusion::Hidden));
    assert_eq!(reason("erin"), Some(Exclusion::Disabled));
    assert_eq!(reason("bad/name"), Some(Exclusion::InvalidName));
    assert_eq!(reason("alex"), None);
    assert_eq!(evaluation.accounts_service_error, None);
    // A repeated uid is listed once, the first entry wins.
    assert_eq!(evaluation.candidates.iter().filter(|candidate| candidate.account.uid == 1000).count(), 1);
}

#[test]
fn hidden_by_uid_and_by_extra_policy() {
    let fixture = Fixture::new();
    fixture.write("etc/bsm/hidden-users", "uid:1005\n");
    let mut policy = fixture.policy();
    policy.extra_hidden = vec!["zed".into()];
    assert_eq!(names(&policy), ["alex", "carol", "gina"]);
}

#[test]
fn fields_come_from_the_right_places() {
    let fixture = Fixture::new();
    let accounts = list_accounts(&fixture.policy()).unwrap();
    let alex = accounts.iter().find(|account| account.user_name == "alex").unwrap();
    assert_eq!((alex.uid, alex.gid), (1000, 1000));
    assert_eq!(alex.display_name, "Alex Doe");
    assert_eq!(alex.home, Path::new("/home/alex"));
    assert_eq!(alex.shell, "/bin/bash");
    assert!(!alex.locked);
    // The icon named by AccountsService's state file.
    assert_eq!(alex.icon_file.as_deref(), Some(Path::new("/var/lib/AccountsService/icons/alex")));
    let gina = accounts.iter().find(|account| account.user_name == "gina").unwrap();
    assert_eq!(gina.display_name, "gina", "an empty GECOS falls back to the login name");
    // The default icon is the file in the icons directory, only when it exists.
    assert_eq!(gina.icon_file.as_deref(), Some(fixture.root.path().join("var/lib/AccountsService/icons/gina").as_path()));
    let frank = accounts.iter().find(|account| account.user_name == "frank").unwrap();
    assert!(frank.locked && frank.icon_file.is_none());
    assert!(gina.locked, "a '*' password field is locked too");
}

#[test]
fn sorted_by_display_name() {
    let fixture = Fixture::new();
    let display: Vec<String> = list_accounts(&fixture.policy()).unwrap().into_iter().map(|account| account.display_name).collect();
    assert_eq!(display, ["Alex Doe", "Frank", "gina", "Zed"]);
}

#[test]
fn uid_range_follows_login_defs() {
    let fixture = Fixture::new();
    fixture.write("etc/login.defs", "UID_MIN 1004\nUID_MAX 1006\n");
    assert_eq!(names(&fixture.policy()), ["frank", "gina"], "erin is expired, alex..dave are below UID_MIN");
    // No login.defs at all: the documented defaults 1000..=60000 apply.
    fs::remove_file(fixture.root.path().join("etc/login.defs")).unwrap();
    assert_eq!(names(&fixture.policy()), ["alex", "frank", "gina", "zed"]);
}

#[test]
fn missing_shadow_and_hidden_files_are_not_errors() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.path().join("etc/shadow")).unwrap();
    fs::remove_file(fixture.root.path().join("etc/bsm/hidden-users")).unwrap();
    let accounts = list_accounts(&fixture.policy()).unwrap();
    let names: Vec<&str> = accounts.iter().map(|account| account.user_name.as_str()).collect();
    assert_eq!(names, ["alex", "carol", "erin", "frank", "gina", "zed"], "no expiry known, nobody hidden");
    assert!(accounts.iter().all(|account| !account.locked));
}

#[test]
fn a_missing_passwd_file_is_an_error() {
    let root = tempfile::tempdir().unwrap();
    assert!(list_accounts(&AccountPolicy::under(root.path())).is_err());
}

#[test]
fn a_symlinked_account_icon_is_still_only_a_candidate() {
    // The account list does not open icons; the publisher does, and refuses symlinks. Here the
    // fixture only checks that a symlink in the icons directory is not offered as a default.
    let fixture = Fixture::new();
    let icons = fixture.root.path().join("var/lib/AccountsService/icons");
    std::os::unix::fs::symlink("/etc/passwd", icons.join("zed")).unwrap();
    let accounts = list_accounts(&fixture.policy()).unwrap();
    assert!(accounts.iter().find(|account| account.user_name == "zed").unwrap().icon_file.is_none());
}

#[test]
fn the_real_system_lists_the_current_user_and_never_root() {
    // Environment dependent by nature: whatever NSS (and AccountsService, if running) says.
    let policy = AccountPolicy::system();
    let evaluation = evaluate(&policy).unwrap();
    let accounts = evaluation.eligible();
    eprintln!("AccountsService: {:?}; eligible: {:?}", evaluation.accounts_service_error, accounts.iter().map(|a| (a.uid, a.user_name.as_str(), a.source)).collect::<Vec<_>>());
    assert!(accounts.iter().all(|account| account.uid != 0 && account.uid != 65534));
    assert!(accounts.iter().all(|account| !account.user_name.is_empty() && !account.display_name.is_empty()));
    // SAFETY: getuid cannot fail.
    let uid = unsafe { libc::getuid() };
    let (low, high) = bsm_appearance::accounts::login_uid_range(Path::new("/etc/login.defs"));
    if (low..=high).contains(&uid) {
        assert!(accounts.iter().any(|account| account.uid == uid), "the current user is a login account: {accounts:?}");
    }
}
