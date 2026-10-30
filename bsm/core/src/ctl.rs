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
use bsm_manager::runtime::client::{Client, ClientError, Publish, monitor_event};
use bsm_manager::runtime::dbus::BusKind;
use bsm_proto::control::{Appearance, ClockFormat, ColorScheme};
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
    publish-appearance     publish this account's greeter appearance (see below)

PUBLISH-APPEARANCE (run as the account whose appearance it is; bsm takes the uid from the bus):
    --color-scheme dark|light  --accent #rrggbb  --clock-format 12h|24h  --reduce-motion true|false
                           all four are required
    --show-personal-images true|false   (default true)
    --wallpaper PATH  --avatar PATH     optional images. bsmctl opens them itself with the caller's
                           rights (links are followed; regular file, at most 16 MiB) and hands bsm
                           the open descriptors; bsm never opens a path. Omitted: no image, the
                           published copy is removed (the state published is the state sent).

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
    PublishAppearance(PublishArgs),
}

#[derive(Debug, PartialEq)]
pub struct PublishArgs {
    pub appearance: Appearance,
    pub show_personal_images: bool,
    pub wallpaper: Option<std::path::PathBuf>,
    pub avatar: Option<std::path::PathBuf>,
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
    let mut opt: std::collections::BTreeMap<&'static str, String> = Default::default();
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
            "--wallpaper" | "--avatar" | "--color-scheme" | "--accent" | "--clock-format" | "--reduce-motion"
            | "--show-personal-images" => {
                let key: &'static str = match a.as_str() {
                    "--wallpaper" => "wallpaper",
                    "--avatar" => "avatar",
                    "--color-scheme" => "color-scheme",
                    "--accent" => "accent",
                    "--clock-format" => "clock-format",
                    "--reduce-motion" => "reduce-motion",
                    _ => "show-personal-images",
                };
                let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
                if opt.insert(key, v).is_some() {
                    return Err(format!("{a} given twice"));
                }
            }
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
        "publish-appearance" => Command::PublishAppearance(publish_args(&mut opt)?),
        "select-user" => Command::SelectUser(one(&mut r).ok_or("select-user needs a uid")?.parse().map_err(|_| "bad uid")?),
        other => return Err(format!("unknown command {other:?}; try --help")),
    };
    if r.next().is_some() {
        return Err("too many arguments".into());
    }
    if let Some(k) = opt.keys().next() {
        return Err(format!("--{k} is only for publish-appearance"));
    }
    Ok(Parsed::Run(Args { json, wait, timeout, bus, command }))
}

fn parse_bool(flag: &str, v: &str) -> Result<bool, String> {
    match v {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("{flag} must be true or false, not {v:?}")),
    }
}

fn publish_args(opt: &mut std::collections::BTreeMap<&'static str, String>) -> Result<PublishArgs, String> {
    let mut need = |k: &str| opt.remove(k).ok_or_else(|| format!("publish-appearance needs --{k}"));
    let color_scheme = match need("color-scheme")?.as_str() {
        "dark" => ColorScheme::Dark,
        "light" => ColorScheme::Light,
        v => return Err(format!("--color-scheme must be dark or light, not {v:?}")),
    };
    let accent = need("accent")?;
    if !(accent.len() == 7 && accent.starts_with('#') && accent[1..].bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err(format!("--accent must be #rrggbb, not {accent:?}"));
    }
    let clock_format = match need("clock-format")?.as_str() {
        "12h" => ClockFormat::H12,
        "24h" => ClockFormat::H24,
        v => return Err(format!("--clock-format must be 12h or 24h, not {v:?}")),
    };
    let reduce_motion = parse_bool("--reduce-motion", &need("reduce-motion")?)?;
    let show_personal_images = match opt.remove("show-personal-images") {
        Some(v) => parse_bool("--show-personal-images", &v)?,
        None => true,
    };
    Ok(PublishArgs {
        appearance: Appearance { color_scheme, accent, clock_format, reduce_motion },
        show_personal_images,
        wallpaper: opt.remove("wallpaper").map(Into::into),
        avatar: opt.remove("avatar").map(Into::into),
    })
}

/// Open an image the way the service requires it: read only, close-on-exec, a regular file of
/// at most `MAX_IMAGE_BYTES`. Symlinks are followed: the file is opened with the calling user's
/// own rights (wallpapers are often links), checked with `fstat` on the open descriptor, and bsm
/// only ever receives the descriptor, never a path.
pub fn open_image(path: &std::path::Path) -> Result<std::fs::File, ClientError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let bad = |m: String| ClientError { code: Some(ErrorCode::InvalidArgument), message: m };
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| bad(format!("cannot open {}: {e}", path.display())))?;
    let m = f.metadata().map_err(|e| bad(format!("cannot stat {}: {e}", path.display())))?;
    if !m.is_file() {
        return Err(bad(format!("{} is not a regular file", path.display())));
    }
    if m.size() > bsm_manager::runtime::dbus::MAX_IMAGE_BYTES {
        return Err(bad(format!("{} is larger than 16 MiB", path.display())));
    }
    Ok(f)
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
        Command::PublishAppearance(p) => {
            let wall = p.wallpaper.as_deref().map(open_image).transpose()?;
            let avatar = p.avatar.as_deref().map(open_image).transpose()?;
            use std::os::fd::AsFd;
            c.publish_appearance(&Publish {
                appearance: &p.appearance,
                show_personal_images: p.show_personal_images,
                wallpaper: wall.as_ref().map(|f| f.as_fd()),
                avatar: avatar.as_ref().map(|f| f.as_fd()),
            })?;
            print_value(&json!({"ok": true}), "published\n".into(), j);
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

    fn publish(extra: &[&str]) -> Result<Parsed, String> {
        let mut a = vec!["publish-appearance", "--color-scheme", "dark", "--accent", "#3584e4", "--clock-format", "24h", "--reduce-motion", "false"];
        a.extend_from_slice(extra);
        p(&a)
    }

    #[test]
    fn publish_appearance_parses_and_validates() {
        let Parsed::Run(a) = publish(&["--wallpaper", "/w.png", "--avatar", "/a.png", "--show-personal-images", "false", "--json"]).unwrap() else { panic!() };
        let Command::PublishAppearance(x) = a.command else { panic!() };
        assert_eq!(x.appearance.accent, "#3584e4");
        assert!(!x.show_personal_images && !x.appearance.reduce_motion);
        assert_eq!(x.wallpaper.as_deref(), Some(std::path::Path::new("/w.png")));
        let Parsed::Run(a) = publish(&[]).unwrap() else { panic!() };
        let Command::PublishAppearance(x) = a.command else { panic!() };
        assert!(x.show_personal_images && x.wallpaper.is_none() && x.avatar.is_none());
        assert!(p(&["publish-appearance"]).unwrap_err().contains("needs --"));
        for bad in [["--color-scheme", "blue"], ["--accent", "red"], ["--clock-format", "25h"], ["--reduce-motion", "yes"]] {
            let mut a = vec!["publish-appearance", "--color-scheme", "dark", "--accent", "#112233", "--clock-format", "24h", "--reduce-motion", "true"];
            let i = a.iter().position(|x| *x == bad[0]).unwrap();
            a[i + 1] = bad[1];
            assert!(p(&a).is_err(), "{bad:?}");
        }
        assert!(p(&["status", "--accent", "#000000"]).unwrap_err().contains("only for publish-appearance"));
        assert!(publish(&["--wallpaper", "/a", "--wallpaper", "/b"]).unwrap_err().contains("twice"));
    }

    #[test]
    fn images_are_opened_by_the_caller_with_the_service_rules() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let ok = d.path().join("a.png");
        std::fs::write(&ok, b"png").unwrap();
        assert!(open_image(&ok).is_ok());
        let link = d.path().join("l.png");
        symlink(&ok, &link).unwrap();
        assert!(open_image(&link).is_ok(), "a link to a regular file is the user's own file");
        let dangling = d.path().join("d.png");
        symlink(d.path().join("nowhere"), &dangling).unwrap();
        assert!(open_image(&dangling).is_err());
        assert!(open_image(d.path()).unwrap_err().message.contains("not a regular file"));
        let fifo = d.path().join("f");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(open_image(&fifo).unwrap_err().message.contains("not a regular file"));
        let big = d.path().join("big");
        std::fs::File::create(&big).unwrap().set_len(17 * 1024 * 1024).unwrap();
        assert!(open_image(&big).unwrap_err().message.contains("16 MiB"));
        assert!(open_image(&d.path().join("missing")).is_err());
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
