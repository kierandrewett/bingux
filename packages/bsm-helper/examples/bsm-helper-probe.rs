//! Scripted client for exercising a real `bsm-helper` from shell scripts
//! (`tests/bsm-helper-root.sh`). Run it as the account the helper accepts
//! (`runuser -u bsm -- bsm-helper-probe ...`). Every event is printed as one
//! JSON line on stdout; the exit status is 0 only when the scenario reached
//! its expected end.
//!
//! ```text
//! bsm-helper-probe [--socket PATH] ping
//! bsm-helper-probe [--socket PATH] login USER UID GEN [login|unlock]   password from $BSM_PROBE_PASSWORD
//! bsm-helper-probe [--socket PATH] open  USER UID SESSION GEN SOCKET   login + OpenSession
//! bsm-helper-probe [--socket PATH] close SESSION GEN [GRACE_MS]
//! bsm-helper-probe [--socket PATH] lock  SESSION GEN true|false
//! bsm-helper-probe [--socket PATH] greeter GEN WAYLAND CONTROL         spawn, wait for $BSM_PROBE_HOLD seconds, kill
//! bsm-helper-probe [--socket PATH] wait-exit SESSION                    until SessionExited
//! ```

use std::os::fd::AsRawFd;
use std::process::ExitCode;
use std::time::Duration;

use bsm_helper::sys::{self, Recv};
use bsm_proto::helper::{HelperEvent, HelperRequest, MAX_DATAGRAM, PamPromptKind};
use bsm_proto::paths;
use bsm_proto::state::AuthIntent;

struct Client {
    fd: std::os::fd::OwnedFd,
}

impl Client {
    fn send(&self, req: &HelperRequest) -> Result<(), String> {
        let bytes = serde_json::to_vec(req).map_err(|e| e.to_string())?;
        sys::send(self.fd.as_raw_fd(), &bytes).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn recv(&self, timeout: Duration) -> Result<HelperEvent, String> {
        if !sys::wait_readable(self.fd.as_raw_fd(), timeout).map_err(|e| e.to_string())? {
            return Err("timed out waiting for the helper".into());
        }
        let mut buf = vec![0u8; MAX_DATAGRAM + 1];
        match sys::recv(self.fd.as_raw_fd(), &mut buf).map_err(|e| e.to_string())? {
            Recv::Data(n) => {
                let ev: HelperEvent = serde_json::from_slice(&buf[..n]).map_err(|e| e.to_string())?;
                println!("{}", serde_json::to_string(&ev).unwrap());
                Ok(ev)
            }
            Recv::Eof => Err("the helper closed the connection".into()),
            other => Err(format!("unexpected receive result {other:?}")),
        }
    }

    fn until(&self, timeout: Duration, mut pred: impl FnMut(&HelperEvent) -> bool) -> Result<HelperEvent, String> {
        loop {
            let ev = self.recv(timeout)?;
            if pred(&ev) {
                return Ok(ev);
            }
        }
    }

    fn login(&self, user: &str, uid: u32, generation: u64, intent: AuthIntent) -> Result<u64, String> {
        let password = std::env::var("BSM_PROBE_PASSWORD").map_err(|_| "BSM_PROBE_PASSWORD is not set".to_owned())?;
        let service = match intent {
            AuthIntent::Login => "bsm-login",
            _ => "bsm-unlock",
        };
        self.send(&HelperRequest::AuthStart {
            req_id: 1,
            user: user.to_owned(),
            uid,
            intent,
            generation,
            service: service.to_owned(),
        })?;
        loop {
            match self.recv(Duration::from_secs(60))? {
                HelperEvent::AuthPrompt { kind: PamPromptKind::EchoOff | PamPromptKind::EchoOn, .. } => {
                    self.send(&HelperRequest::AuthResponse { req_id: 1, response: password.clone() })?;
                }
                HelperEvent::AuthDone { ok: true, ticket: Some(t), .. } => return Ok(t),
                HelperEvent::AuthDone { message, .. } => return Err(format!("authentication failed: {message}")),
                _ => {}
            }
        }
    }
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut socket = paths::HELPER_SOCKET.to_owned();
    if args.first().map(String::as_str) == Some("--socket") {
        if args.len() < 2 {
            return Err("--socket needs a path".into());
        }
        socket = args[1].clone();
        args.drain(..2);
    }
    let fd = sys::connect_seqpacket(std::path::Path::new(&socket)).map_err(|e| format!("connect {socket}: {e}"))?;
    let c = Client { fd };
    let a = |i: usize| args.get(i).cloned().ok_or_else(|| "missing argument".to_owned());
    let num = |i: usize| -> Result<u64, String> { a(i)?.parse().map_err(|_| format!("argument {i} must be a number")) };
    match a(0)?.as_str() {
        "ping" => {
            c.send(&HelperRequest::Ping { nonce: 1 })?;
            c.until(Duration::from_secs(10), |e| matches!(e, HelperEvent::Pong { .. }))?;
        }
        "login" => {
            let intent = match args.get(4).map(String::as_str) {
                Some("unlock") => AuthIntent::Unlock,
                _ => AuthIntent::Login,
            };
            c.login(&a(1)?, num(2)? as u32, num(3)?, intent)?;
        }
        "open" => {
            let (user, uid, session, generation, socket_path) = (a(1)?, num(2)? as u32, a(3)?, num(4)?, a(5)?);
            let ticket = c.login(&user, uid, generation, AuthIntent::Login)?;
            c.send(&HelperRequest::OpenSession {
                req_id: 2,
                ticket,
                session,
                generation,
                socket_path,
                output_hints: vec![(1280, 800)],
            })?;
            match c.until(Duration::from_secs(120), |e| {
                matches!(e, HelperEvent::SessionOpened { .. } | HelperEvent::SessionFailed { .. })
            })? {
                HelperEvent::SessionOpened { .. } => {}
                HelperEvent::SessionFailed { message, .. } => return Err(format!("session failed: {message}")),
                _ => unreachable!(),
            }
        }
        "close" => {
            let grace = args.get(3).and_then(|g| g.parse().ok()).unwrap_or(2000);
            c.send(&HelperRequest::CloseSession { session: a(1)?, generation: num(2)?, grace_ms: grace })?;
            c.until(Duration::from_secs(60), |e| matches!(e, HelperEvent::SessionExited { .. }))?;
        }
        "wait-exit" => {
            let session = a(1)?;
            c.until(Duration::from_secs(120), |e| matches!(e, HelperEvent::SessionExited { session: s, .. } if *s == session))?;
        }
        "lock" => {
            let locked = a(3)? == "true";
            c.send(&HelperRequest::SetLockedHint { req_id: 1, session: a(1)?, generation: num(2)?, locked })?;
            match c.until(Duration::from_secs(30), |e| matches!(e, HelperEvent::LockedHintResult { .. }))? {
                HelperEvent::LockedHintResult { ok: true, .. } => {}
                HelperEvent::LockedHintResult { message, .. } => return Err(format!("SetLockedHint failed: {message}")),
                _ => unreachable!(),
            }
        }
        "greeter" => {
            let generation = num(1)?;
            c.send(&HelperRequest::SpawnGreeter { req_id: 1, generation, wayland_socket: a(2)?, control_socket: a(3)? })?;
            c.until(Duration::from_secs(30), |e| matches!(e, HelperEvent::GreeterSpawned { .. }))?;
            let hold: u64 = std::env::var("BSM_PROBE_HOLD").ok().and_then(|h| h.parse().ok()).unwrap_or(5);
            std::thread::sleep(Duration::from_secs(hold));
            c.send(&HelperRequest::KillGreeter { generation })?;
            c.until(Duration::from_secs(30), |e| matches!(e, HelperEvent::GreeterExited { .. }))?;
        }
        other => return Err(format!("unknown command {other:?}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bsm-helper-probe: {e}");
            ExitCode::FAILURE
        }
    }
}
