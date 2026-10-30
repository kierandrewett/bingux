//! The shipped PAM files and why they use `substack` (review F8).

use std::path::PathBuf;

use bsm_helper::pam::{Conversation, Interrupt, Pam};
use bsm_proto::helper::PamPromptKind;

struct Silent;
impl Conversation for Silent {
    fn converse(&mut self, _: PamPromptKind, _: &str) -> Result<Option<String>, Interrupt> {
        Ok(None)
    }
}

fn shipped(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packaging/bsm/pam").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn shipped_stacks_pull_in_system_auth_with_substack_for_auth_account_and_session() {
    for (file, phases) in [("bsm-login", &["auth", "account", "session"][..]), ("bsm-unlock", &["auth", "account"][..])] {
        let text = shipped(file);
        for phase in phases {
            let line = text
                .lines()
                .find(|l| !l.trim_start().starts_with('#') && l.split_whitespace().collect::<Vec<_>>() == [phase, "substack", "system-auth"]
                    || (!l.trim_start().starts_with('#') && l.split_whitespace().take(3).collect::<Vec<_>>() == [phase, "include", "system-auth"]));
            match line {
                Some(l) => assert!(l.contains("substack"), "{file}: {phase} must use substack, found `{l}`"),
                None => panic!("{file}: no {phase} line for system-auth"),
            }
        }
    }
}

fn run_auth(dir: &std::path::Path, service: &str) -> bool {
    let mut p = Pam::start(service, "nobody", Some(dir), Box::new(Silent)).unwrap();
    p.authenticate().is_ok()
}

#[test]
fn a_sufficient_module_in_system_auth_does_not_skip_the_lines_after_a_substack() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let marker = |name: &str| d.join(name);
    let touch = |name: &str| format!("auth required pam_exec.so quiet /bin/touch {}\n", marker(name).display());
    // system-auth in the Fedora/Arch style: sufficient success, then a deny.
    std::fs::write(d.join("sys-auth"), "auth sufficient pam_permit.so\nauth required pam_deny.so\n").unwrap();
    std::fs::write(d.join("svc-sub"), format!("auth substack sys-auth\n{}", touch("sub-ran"))).unwrap();
    std::fs::write(d.join("svc-inc"), format!("auth include sys-auth\n{}", touch("inc-ran"))).unwrap();
    assert!(run_auth(d, "svc-sub"));
    assert!(marker("sub-ran").exists(), "substack: the keyring-style line after it must run");
    assert!(run_auth(d, "svc-inc"));
    assert!(!marker("inc-ran").exists(), "include: the line after a sufficient success is skipped (the bug F8 fixes)");
}
