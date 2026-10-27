//! Test-only PAM service directory, shared by unit tests (`#[cfg(test)]`) and
//! the integration tests (`#[path]` include). Never part of the shipped binary.
//!
//! The stacks drive the real libpam through `pam_start_confdir`: `pam_echo`
//! produces info text, `pam_exec` with `expose_authtok` makes libpam call the
//! conversation for a hidden prompt and runs a script that checks the answer.

#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const PASSWORD: &str = "correct horse";

pub struct TestPam {
    pub dir: tempfile::TempDir,
    pub user: String,
    pub uid: u32,
}

fn write(path: &Path, text: &str, mode: u32) {
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

impl TestPam {
    pub fn new() -> TestPam {
        let uid = unsafe { libc::getuid() };
        let user = bsm_helper::sys::getpwuid(uid).unwrap().expect("test user has a passwd entry").name;
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(&d.join("banner"), "welcome banner\n", 0o644);
        write(
            &d.join("check.sh"),
            &format!("#!/bin/sh\n[ \"$PAM_TYPE\" = auth ] || exit 0\nIFS= read -r -d '' pw\n[ \"$pw\" = slow-wrong ] && sleep 2\n[ \"$pw\" = \"{PASSWORD}\" ]\n"),
            0o755,
        );
        // The session hook records what a session module sees. pam_exec runs it
        // with the PAM environment, so this proves what pam_putenv delivered.
        write(
            &d.join("session-hook.sh"),
            &format!(
                "#!/bin/sh\necho \"$PAM_TYPE user=$PAM_USER type=$XDG_SESSION_TYPE class=$XDG_SESSION_CLASS \
                 desktop=$XDG_SESSION_DESKTOP seat=${{XDG_SEAT-unset}} vtnr=${{XDG_VTNR-unset}} \
                 session=$BSM_SESSION_ID gen=$BSM_GENERATION\" >> {}/session.log\n",
                d.display()
            ),
            0o755,
        );
        let auth = format!(
            "auth optional pam_echo.so file={d}/banner\nauth requisite pam_exec.so expose_authtok quiet {d}/check.sh\nauth required pam_permit.so\n\
             account required pam_permit.so\n",
            d = d.display()
        );
        write(&d.join("bsm-test-password"), &auth, 0o644);
        write(
            &d.join("bsm-test-login"),
            &format!(
                "{auth}session required pam_permit.so\nsession optional pam_exec.so quiet {}/session-hook.sh\n",
                d.display()
            ),
            0o644,
        );
        write(&d.join("bsm-test-deny"), "auth required pam_deny.so\naccount required pam_permit.so\n", 0o644);
        TestPam { dir, user, uid }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn session_log(&self) -> PathBuf {
        self.dir.path().join("session.log")
    }

    pub fn session_log_lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.session_log()).unwrap_or_default().lines().map(str::to_owned).collect()
    }
}
