//! The authentication child: one forked process per conversation.
//!
//! The long-lived helper never calls into PAM for authentication. A child
//! runs `pam_authenticate` and `pam_acct_mgmt` for exactly one conversation,
//! relays every prompt to the helper and exits. All PAM state, including the
//! typed secret inside libpam, dies with the child.

use std::os::fd::RawFd;
use std::path::PathBuf;
use std::time::Duration;

use bsm_proto::error::ErrorCode;
use bsm_proto::helper::PamPromptKind;

use crate::ipc::{self, AuthChildEvent, AuthParentCmd, Incoming};
use crate::pam::{Conversation, Interrupt, InterruptSlot, Pam};
use crate::sys;

pub struct AuthParams {
    pub service: String,
    pub user: String,
    pub confdir: Option<PathBuf>,
    /// How long the user may take to answer one prompt.
    pub prompt_timeout: Duration,
}

/// Conversation that relays prompts over the child's socket to the helper.
struct Relay {
    sock: RawFd,
    timeout: Duration,
    slot: InterruptSlot,
}

impl Relay {
    fn interrupt(&self, why: Interrupt) -> Interrupt {
        self.slot.set(Some(why));
        why
    }
}

impl Conversation for Relay {
    fn converse(&mut self, kind: PamPromptKind, text: &str) -> Result<Option<String>, Interrupt> {
        let ev = AuthChildEvent::Prompt { kind, text: text.to_owned() };
        match ipc::send_msg(self.sock, &ev) {
            Ok(true) => {}
            _ => return Err(self.interrupt(Interrupt::Cancelled)),
        }
        if matches!(kind, PamPromptKind::Info | PamPromptKind::Error) {
            return Ok(None);
        }
        loop {
            match sys::wait_readable(self.sock, self.timeout) {
                Ok(true) => {}
                Ok(false) => return Err(self.interrupt(Interrupt::TimedOut)),
                Err(_) => return Err(self.interrupt(Interrupt::Cancelled)),
            }
            match ipc::recv_msg::<AuthParentCmd>(self.sock) {
                Ok(Incoming::Msg(cmd)) => match &cmd {
                    AuthParentCmd::Response { text } => return Ok(Some(text.clone())),
                    AuthParentCmd::Abort => return Err(self.interrupt(Interrupt::Cancelled)),
                },
                Ok(Incoming::WouldBlock) => continue,
                // Parent gone, or garbage on a private socket: stop.
                _ => return Err(self.interrupt(Interrupt::Cancelled)),
            }
        }
    }
}

/// Outcome sent as the last message.
pub fn run_conversation(sock: RawFd, params: &AuthParams) -> AuthChildEvent {
    let slot: InterruptSlot = Default::default();
    let relay = Relay { sock, timeout: params.prompt_timeout, slot: slot.clone() };
    let mut pam = match Pam::start(&params.service, &params.user, params.confdir.as_deref(), Box::new(relay)) {
        Ok(p) => p,
        Err(e) => {
            return AuthChildEvent::Done { ok: false, code: Some(ErrorCode::PamFailure), message: e.message };
        }
    };
    let result = pam.authenticate().and_then(|()| pam.acct_mgmt());
    match result {
        Ok(()) => AuthChildEvent::Done { ok: true, code: None, message: String::new() },
        Err(e) => {
            let (code, message) = match slot.get() {
                Some(Interrupt::Cancelled) => (ErrorCode::Cancelled, "authentication cancelled".to_owned()),
                Some(Interrupt::TimedOut) => (ErrorCode::Timeout, "authentication timed out".to_owned()),
                None => (e.error_code(), e.message.clone()),
            };
            AuthChildEvent::Done { ok: false, code: Some(code), message }
        }
    }
}

/// Entry point of the forked child. Returns the process exit status.
pub fn child_main(sock: RawFd, params: &AuthParams) -> i32 {
    let done = run_conversation(sock, params);
    let ok = matches!(done, AuthChildEvent::Done { ok: true, .. });
    let _ = ipc::send_msg(sock, &done);
    if ok { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{TestPam, drive_prompts};
    use std::os::fd::AsRawFd;

    fn params(pam: &TestPam, service: &str) -> AuthParams {
        AuthParams {
            service: service.to_owned(),
            user: pam.user.clone(),
            confdir: Some(pam.dir.path().to_path_buf()),
            prompt_timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn correct_password_succeeds_after_info_and_prompt() {
        let pam = TestPam::new();
        let (parent, child) = sys::seqpacket_pair().unwrap();
        let p = params(&pam, "bsm-test-password");
        let t = std::thread::spawn(move || child_main(child.as_raw_fd(), &p));
        let events = drive_prompts(parent.as_raw_fd(), &["correct horse"]);
        assert_eq!(t.join().unwrap(), 0);
        assert!(events.iter().any(|e| matches!(e, AuthChildEvent::Prompt { kind: PamPromptKind::Info, text } if text.contains("welcome banner"))));
        assert!(events.iter().any(|e| matches!(e, AuthChildEvent::Prompt { kind: PamPromptKind::EchoOff, .. })));
        assert!(matches!(events.last(), Some(AuthChildEvent::Done { ok: true, code: None, .. })));
    }

    #[test]
    fn wrong_password_fails_with_pam_failure_and_never_echoes_it() {
        let pam = TestPam::new();
        let (parent, child) = sys::seqpacket_pair().unwrap();
        let p = params(&pam, "bsm-test-password");
        let t = std::thread::spawn(move || child_main(child.as_raw_fd(), &p));
        let events = drive_prompts(parent.as_raw_fd(), &["wrong-guess-123"]);
        assert_eq!(t.join().unwrap(), 1);
        let Some(AuthChildEvent::Done { ok: false, code: Some(ErrorCode::PamFailure), message }) = events.last() else {
            panic!("unexpected outcome: {events:?}");
        };
        assert!(!message.contains("wrong-guess-123"));
        assert!(!format!("{events:?}").contains("wrong-guess-123"));
    }

    #[test]
    fn abort_during_prompt_reports_cancelled() {
        let pam = TestPam::new();
        let (parent, child) = sys::seqpacket_pair().unwrap();
        let p = params(&pam, "bsm-test-password");
        let t = std::thread::spawn(move || child_main(child.as_raw_fd(), &p));
        // Wait for the password prompt, then abort instead of answering.
        let events = crate::testkit::abort_at_first_secret_prompt(parent.as_raw_fd());
        t.join().unwrap();
        assert!(matches!(events.last(), Some(AuthChildEvent::Done { ok: false, code: Some(ErrorCode::Cancelled), .. })));
    }

    #[test]
    fn unanswered_prompt_times_out() {
        let pam = TestPam::new();
        let (parent, child) = sys::seqpacket_pair().unwrap();
        let mut p = params(&pam, "bsm-test-password");
        p.prompt_timeout = Duration::from_millis(200);
        let t = std::thread::spawn(move || child_main(child.as_raw_fd(), &p));
        let events = crate::testkit::read_until_done(parent.as_raw_fd());
        t.join().unwrap();
        assert!(matches!(events.last(), Some(AuthChildEvent::Done { ok: false, code: Some(ErrorCode::Timeout), .. })));
    }

    #[test]
    fn parent_vanishing_ends_the_conversation() {
        let pam = TestPam::new();
        let (parent, child) = sys::seqpacket_pair().unwrap();
        let p = params(&pam, "bsm-test-password");
        let t = std::thread::spawn(move || child_main(child.as_raw_fd(), &p));
        std::thread::sleep(Duration::from_millis(100));
        drop(parent);
        assert_eq!(t.join().unwrap(), 1);
    }

    #[test]
    fn deny_stack_fails_and_unknown_service_is_not_a_success() {
        let pam = TestPam::new();
        let (parent, child) = sys::seqpacket_pair().unwrap();
        let p = params(&pam, "bsm-test-deny");
        let t = std::thread::spawn(move || child_main(child.as_raw_fd(), &p));
        let events = crate::testkit::read_until_done(parent.as_raw_fd());
        assert_eq!(t.join().unwrap(), 1);
        assert!(matches!(events.last(), Some(AuthChildEvent::Done { ok: false, .. })));
        // A service name that does not exist in the directory must fail
        // closed, never fall back to a permissive stack.
        let (parent, child) = sys::seqpacket_pair().unwrap();
        let p = params(&pam, "bsm-test-missing");
        let t = std::thread::spawn(move || child_main(child.as_raw_fd(), &p));
        let events = crate::testkit::read_until_done(parent.as_raw_fd());
        assert_eq!(t.join().unwrap(), 1);
        assert!(matches!(events.last(), Some(AuthChildEvent::Done { ok: false, .. })));
    }

    #[test]
    fn pam_conv_error_codes_are_exported() {
        assert_eq!(crate::pam::PAM_CONV_ERR, 19);
    }
}
