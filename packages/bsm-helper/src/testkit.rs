//! Unit-test helpers that drive an authentication child over its socket.

use std::os::fd::RawFd;

use bsm_proto::helper::PamPromptKind;

use crate::ipc::{self, AuthChildEvent, AuthParentCmd, Incoming};
use crate::sys;

pub use crate::testpam::TestPam;

fn next(fd: RawFd) -> Option<AuthChildEvent> {
    loop {
        if !sys::wait_readable(fd, std::time::Duration::from_secs(10)).unwrap() {
            panic!("child sent nothing for 10 s");
        }
        match ipc::recv_msg::<AuthChildEvent>(fd).unwrap() {
            Incoming::Msg(m) => return Some(m),
            Incoming::Eof => return None,
            Incoming::WouldBlock => continue,
            Incoming::Bad => panic!("bad datagram"),
        }
    }
}

/// Answer each hidden or visible prompt from `answers` in order; return every event.
pub fn drive_prompts(fd: RawFd, answers: &[&str]) -> Vec<AuthChildEvent> {
    let mut answers = answers.iter();
    let mut events = Vec::new();
    while let Some(ev) = next(fd) {
        let done = matches!(ev, AuthChildEvent::Done { .. });
        if let AuthChildEvent::Prompt { kind: PamPromptKind::EchoOff | PamPromptKind::EchoOn, .. } = &ev {
            let a = answers.next().expect("more prompts than answers");
            ipc::send_msg(fd, &AuthParentCmd::Response { text: zeroize::Zeroizing::new((*a).to_owned()) }).unwrap();
        }
        events.push(ev);
        if done {
            break;
        }
    }
    events
}

pub fn abort_at_first_secret_prompt(fd: RawFd) -> Vec<AuthChildEvent> {
    let mut events = Vec::new();
    while let Some(ev) = next(fd) {
        let done = matches!(ev, AuthChildEvent::Done { .. });
        let secret = matches!(ev, AuthChildEvent::Prompt { kind: PamPromptKind::EchoOff, .. });
        events.push(ev);
        if secret {
            ipc::send_msg(fd, &AuthParentCmd::Abort).unwrap();
        }
        if done {
            break;
        }
    }
    events
}

/// Read events without ever answering.
pub fn read_until_done(fd: RawFd) -> Vec<AuthChildEvent> {
    let mut events = Vec::new();
    while let Some(ev) = next(fd) {
        let done = matches!(ev, AuthChildEvent::Done { .. });
        events.push(ev);
        if done {
            break;
        }
    }
    events
}
