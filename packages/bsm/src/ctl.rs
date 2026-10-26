//! `bsmctl`: the command line client of `org.bingux.Session1`.
//!
//! Read-only commands work for any local user. The actions (`lock`, `logout`,
//! `switch-user`, `suspend`, `reboot`, `poweroff`) ask bsm, which decides by who the
//! caller is (its session found from its cgroup, or root); nothing here can override an
//! authentication, unlock a session or show another user's desktop, because the service has
//! no such method. `select-user` only changes whose picture the greeter shows, and only root
//! may call it.
//!
//! With `--json` every command prints JSON and errors are one line
//! `{"error":"<code>","message":"..."}` with exit status 1. A request prints one line
//! `{"operation_id":N}` and returns at once; `--wait` blocks until `OperationFinished` and
//! prints `{"operation_id":N,"success":bool,"reason":..}`, exit 0 only on success.
//! 64 bit generations are printed as decimal strings.

use std::io::Write;
use std::time::Duration;

use bsm_manager::core::{DbusSignal, OperationInfo, SessionInfo, StatusInfo, UserInfo};
use bsm_manager::runtime::client::{Client, ClientError, monitor_event};
use bsm_manager::runtime::dbus::BusKind;
use bsm_proto::error::ErrorCode;
use serde_json::{Value, json};

pub const HELP: &str = "\
bsmctl - talk to bsm, the Bingux Session Manager

USAGE:
    bsmctl [--json] [--bus system|session|ADDRESS] <COMMAND>

READ ONLY:
    status                 seat state, selected and last used user, foreground session
    sessions               hosted sessions and their state
    users                  eligible accounts and whether they are signed in
    outputs                the outputs bsm drives
    transitions            the operation in flight and recent ones
    monitor                stream signals, one line each (use with --json)

ACTIONS (the request returns at once; --wait blocks until it finished):
    lock [SESSION]         protect the session, show the lock screen
    logout [SESSION]       fade out and close the session
    switch-user            lock and show the account picker
    suspend | reboot | poweroff
    cancel OPERATION       cancel an operation that has not passed its point of no return
    select-user UID        (root) show another account on the greeter; reveals nothing

OPTIONS:
    --json                 machine readable output
    --wait                 wait for the operation to finish (actions)
    --timeout SECONDS      bound for --wait (default 60)
    --bus ...              system (default), session or a D-Bus address
    --version, --help
";

#[derive(Debug, PartialEq)]
pub enum Command {
    Status,
    Sessions,
    Users,
    Outputs,
    Transitions,
    Monitor,
    Lock(Option<String>),
    Logout(Option<String>),
    SwitchUser,
    Suspend,
    Reboot,
    PowerOff,
    Cancel(u64),
    SelectUser(u32),
}

#[derive(Debug, PartialEq)]
pub struct Args {
    pub json: bool,
    pub wait: bool,
    pub timeout: Duration,
    pub bus: String,
    pub command: Command,
}

#[derive(Debug)]
pub enum Parsed {
    Run(Args),
    Help,
    Version,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Parsed, String> {
    let mut json = false;
    let mut wait = false;
    let mut timeout = Duration::from_secs(60);
    let mut bus = "system".to_owned();
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--version" | "-V" => return Ok(Parsed::Version),
            "--json" => json = true,
            "--wait" => wait = true,
            "--timeout" => {
                let v = it.next().ok_or("--timeout needs seconds")?;
                timeout = Duration::from_secs(v.parse().map_err(|_| format!("bad timeout {v:?}"))?);
            }
            "--bus" => bus = it.next().ok_or("--bus needs a value")?,
            s if s.starts_with("--") => return Err(format!("unknown option {s:?}; try --help")),
            _ => rest.push(a),
        }
    }
    let mut r = rest.into_iter();
    let name = r.next().ok_or("a command is required; try --help")?;
    let one = |r: &mut std::vec::IntoIter<String>| r.next();
    let command = match name.as_str() {
        "status" => Command::Status,
        "sessions" => Command::Sessions,
        "users" => Command::Users,
        "outputs" => Command::Outputs,
        "transitions" => Command::Transitions,
        "monitor" => Command::Monitor,
        "lock" => Command::Lock(one(&mut r)),
        "logout" => Command::Logout(one(&mut r)),
        "switch-user" => Command::SwitchUser,
        "suspend" => Command::Suspend,
        "reboot" => Command::Reboot,
        "poweroff" | "power-off" => Command::PowerOff,
        "cancel" => Command::Cancel(one(&mut r).ok_or("cancel needs an operation id")?.parse().map_err(|_| "bad operation id")?),
        "select-user" => Command::SelectUser(one(&mut r).ok_or("select-user needs a uid")?.parse().map_err(|_| "bad uid")?),
        other => return Err(format!("unknown command {other:?}; try --help")),
    };
    if r.next().is_some() {
        return Err("too many arguments".into());
    }
    Ok(Parsed::Run(Args { json, wait, timeout, bus, command }))
}

fn name_of<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

pub fn user_json(u: &UserInfo) -> Value {
    json!({
        "uid": u.uid, "user_name": u.user_name, "display_name": u.display_name,
        "availability": name_of(&u.availability), "session": u.session,
        "avatar_rev": u.avatar_rev, "wallpaper_rev": u.wallpaper_rev,
    })
}

pub fn session_json(s: &SessionInfo) -> Value {
    json!({
        "id": s.id, "generation": s.generation.to_string(), "uid": s.uid,
        "state": name_of(&s.state), "availability": name_of(&s.availability),
        "foreground": s.foreground, "locked": s.locked, "progress": s.progress,
    })
}

pub fn operation_json(o: &OperationInfo) -> Value {
    json!({
        "operation_id": o.id, "kind": name_of(&o.kind), "phase": name_of(&o.phase),
        "state": name_of(&o.state), "reason": o.reason.map(|r| r.as_str()), "session": o.session,
    })
}

pub fn status_json(s: &StatusInfo) -> Value {
    json!({
        "seat": name_of(&s.seat), "selected_uid": s.selected_uid, "last_used_uid": s.last_used_uid,
        "foreground": s.foreground, "revision": s.revision, "greeter_connected": s.greeter_connected,
        "helper_connected": s.helper_connected, "outputs": s.outputs,
        "operation": s.operation.as_ref().map(operation_json),
    })
}

/// One line per signal for `monitor`: the `MonitorEvent` where one exists, otherwise an
/// object with the same `event` tag convention.
pub fn signal_json(s: &DbusSignal) -> Value {
    if let Some(e) = monitor_event(s) {
        return name_of(&e);
    }
    match s {
        DbusSignal::SelectedUserChanged { revision, uid } => json!({"event":"selected_user_changed","revision":revision,"uid":uid}),
        DbusSignal::AppearanceChanged { uid, revision } => json!({"event":"appearance_changed","uid":uid,"revision":revision}),
        DbusSignal::SessionReadinessChanged { session, generation, ready } => {
            json!({"event":"session_readiness_changed","session":session,"generation":generation.to_string(),"ready":ready})
        }
        other => json!({"event":"unknown","debug":format!("{other:?}")}),
    }
}

fn human_table(rows: &[Vec<String>]) -> String {
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols).map(|c| rows.iter().map(|r| r.get(c).map_or(0, |s| s.len())).max().unwrap_or(0)).collect();
    let mut out = String::new();
    for r in rows {
        let line: Vec<String> = r.iter().enumerate().map(|(i, c)| format!("{:<w$}", c, w = widths[i])).collect();
        out.push_str(line.join("  ").trim_end());
        out.push('\n');
    }
    out
}

fn print_value(v: &Value, human: String, json: bool) {
    if json {
        println!("{v}");
    } else {
        print!("{human}");
    }
    let _ = std::io::stdout().flush();
}

fn fail(json: bool, e: &ClientError) -> i32 {
    let code = e.code.map_or("internal", |c| c.as_str());
    if json {
        println!("{}", json!({"error": code, "message": e.message}));
    } else {
        eprintln!("bsmctl: {} ({code})", e.message);
    }
    1
}

fn bus_kind(s: &str) -> BusKind {
    match s {
        "system" => BusKind::System,
        "session" => BusKind::Session,
        a => BusKind::Address(a.to_owned()),
    }
}

/// Run the command; returns the exit status.
pub fn run(a: Args) -> i32 {
    let client = match Client::connect(bus_kind(&a.bus)) {
        Ok(c) => c,
        Err(e) => return fail(a.json, &e),
    };
    match go(&client, &a) {
        Ok(code) => code,
        Err(e) => fail(a.json, &e),
    }
}

fn go(c: &Client, a: &Args) -> Result<i32, ClientError> {
    let j = a.json;
    match &a.command {
        Command::Status => {
            let s = c.status()?;
            let human = format!(
                "seat:        {:?}\nselected:    {}\nlast used:   {}\nforeground:  {}\ngreeter:     {}\nhelper:      {}\noutputs:     {}\noperation:   {}\n",
                s.seat,
                s.selected_uid.map_or("-".into(), |u| u.to_string()),
                s.last_used_uid.map_or("-".into(), |u| u.to_string()),
                s.foreground.clone().unwrap_or_else(|| "-".into()),
                if s.greeter_connected { "connected" } else { "not connected" },
                if s.helper_connected { "connected" } else { "not connected" },
                s.outputs,
                s.operation.as_ref().map_or("-".into(), |o| format!("{} {:?} {:?}", o.id, o.kind, o.phase)),
            );
            print_value(&status_json(&s), human, j);
        }
        Command::Sessions => {
            let v = c.sessions()?;
            let mut rows = vec![vec!["SESSION".into(), "UID".into(), "STATE".into(), "FOREGROUND".into(), "LOCKED".into(), "PROGRESS".into()]];
            rows.extend(v.iter().map(|s| {
                vec![s.id.clone(), s.uid.to_string(), format!("{:?}", s.state), s.foreground.to_string(), s.locked.to_string(), format!("{}%", s.progress)]
            }));
            print_value(&Value::Array(v.iter().map(session_json).collect()), human_table(&rows), j);
        }
        Command::Users => {
            let v = c.users()?;
            let mut rows = vec![vec!["UID".into(), "NAME".into(), "DISPLAY".into(), "STATE".into(), "SESSION".into()]];
            rows.extend(v.iter().map(|u| {
                vec![u.uid.to_string(), u.user_name.clone(), u.display_name.clone(), format!("{:?}", u.availability), u.session.clone().unwrap_or_else(|| "-".into())]
            }));
            print_value(&Value::Array(v.iter().map(user_json).collect()), human_table(&rows), j);
        }
        Command::Outputs => {
            let v = c.outputs()?;
            let mut rows = vec![vec!["NAME".into(), "POSITION".into(), "SIZE".into(), "SCALE".into(), "REFRESH".into()]];
            rows.extend(v.iter().map(|o| {
                vec![
                    o.name.clone(),
                    format!("{},{}", o.x, o.y),
                    format!("{}x{}", o.width, o.height),
                    format!("{:.2}", o.scale_120 as f64 / 120.0),
                    format!("{:.1} Hz", o.refresh_mhz as f64 / 1000.0),
                ]
            }));
            print_value(&json!(v.iter().map(|o| name_of(o)).collect::<Vec<_>>()), human_table(&rows), j);
        }
        Command::Transitions => {
            let v = c.transitions()?;
            let mut rows = vec![vec!["OPERATION".into(), "KIND".into(), "PHASE".into(), "STATE".into(), "REASON".into(), "SESSION".into()]];
            rows.extend(v.iter().map(|o| {
                vec![
                    o.id.to_string(),
                    format!("{:?}", o.kind),
                    format!("{:?}", o.phase),
                    format!("{:?}", o.state),
                    o.reason.map_or("-".into(), |r| r.as_str().into()),
                    o.session.clone().unwrap_or_else(|| "-".into()),
                ]
            }));
            print_value(&Value::Array(v.iter().map(operation_json).collect()), human_table(&rows), j);
        }
        Command::Monitor => {
            let stream = c.signals()?;
            let stdout = std::io::stdout();
            for sig in stream {
                let line = if j { signal_json(&sig).to_string() } else { format!("{sig:?}") };
                let mut out = stdout.lock();
                if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
                    return Ok(0);
                }
            }
        }
        Command::SelectUser(uid) => {
            c.select_greeter_user(*uid)?;
            print_value(&json!({"ok": true}), "ok\n".into(), j);
        }
        Command::Cancel(op) => {
            c.cancel_operation(*op)?;
            print_value(&json!({"ok": true}), "ok\n".into(), j);
        }
        action => return do_action(c, a, action),
    }
    Ok(0)
}

fn do_action(c: &Client, a: &Args, action: &Command) -> Result<i32, ClientError> {
    let request = |c: &Client| match action {
        Command::Lock(s) => c.request_lock(s.as_deref()),
        Command::Logout(s) => c.request_logout(s.as_deref()),
        Command::SwitchUser => c.request_switch_user(),
        Command::Suspend => c.request_suspend(),
        Command::Reboot => c.request_reboot(),
        Command::PowerOff => c.request_power_off(),
        _ => Err(ClientError { code: Some(ErrorCode::Internal), message: "not an action".into() }),
    };
    if a.wait {
        let f = c.request_and_wait(a.timeout, request)?;
        let v = json!({"operation_id": f.operation_id, "success": f.success, "reason": f.reason.map(|r| r.as_str())});
        print_value(&v, format!("operation {} {}\n", f.operation_id, if f.success { "completed" } else { "failed" }), a.json);
        return Ok(if f.success { 0 } else { 1 });
    }
    let op = request(c)?;
    print_value(&json!({"operation_id": op}), format!("operation {op}\n"), a.json);
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Parsed, String> {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn commands_and_flags_parse() {
        let Parsed::Run(a) = p(&["--json", "lock"]).unwrap() else { panic!() };
        assert_eq!(a.command, Command::Lock(None));
        assert!(a.json && !a.wait);
        let Parsed::Run(a) = p(&["logout", "s0000000a", "--wait", "--timeout", "5"]).unwrap() else { panic!() };
        assert_eq!(a.command, Command::Logout(Some("s0000000a".into())));
        assert!(a.wait);
        assert_eq!(a.timeout, Duration::from_secs(5));
        for (word, cmd) in [("poweroff", Command::PowerOff), ("power-off", Command::PowerOff), ("reboot", Command::Reboot), ("suspend", Command::Suspend)] {
            let Parsed::Run(a) = p(&[word]).unwrap() else { panic!() };
            assert_eq!(a.command, cmd);
        }
    }

    #[test]
    fn mistakes_are_sentences() {
        assert!(p(&[]).unwrap_err().contains("command is required"));
        assert!(p(&["unlock"]).unwrap_err().contains("unknown command"));
        assert!(p(&["status", "extra"]).unwrap_err().contains("too many"));
        assert!(p(&["select-user", "x"]).is_err());
        assert!(p(&["--nope", "status"]).is_err());
    }

    #[test]
    fn there_is_no_command_that_unlocks_or_reveals() {
        for word in ["unlock", "login", "reveal", "switch-to", "activate", "foreground"] {
            assert!(p(&[word]).is_err(), "{word} must not exist");
        }
    }

    #[test]
    fn generations_are_strings_in_json() {
        let s = SessionInfo {
            id: "s1".into(),
            generation: u64::MAX,
            uid: 1,
            state: bsm_proto::state::SessionState::Foreground,
            availability: bsm_proto::state::SessionAvailability::RunningForeground,
            foreground: true,
            locked: false,
            progress: 100,
        };
        let v = session_json(&s);
        assert_eq!(v["generation"], "18446744073709551615");
        assert_eq!(v["state"], "foreground");
    }
}
