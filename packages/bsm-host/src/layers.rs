//! Gnoblin's layer-surface list, read from Gnoblin's control socket
//! (`$XDG_RUNTIME_DIR/gnoblin/compositor-v1.sock`, JSON lines).
//!
//! The bridge cross-checks Bingux's own claims about mapped surfaces with this
//! list (see `readiness.rs`). The same connection carries Gnoblin's
//! `gnoblin.session.lock-requested` event, which the bridge forwards to bsm.
//!
//! Records are parsed tolerantly: a layer needs `namespace`, `monitor_id` and
//! `mapped`; a monitor needs an id and connector names (`output_names`).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::msg::Msg;
use crate::readiness::LayerSurface;

const API: (u32, u32) = (1, 81);
const REQUERY: Duration = Duration::from_secs(2);

pub fn socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("GNOBLIN_COMPOSITOR_SOCKET") {
        return PathBuf::from(p);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_default();
    runtime.join("gnoblin/compositor-v1.sock")
}

fn version() -> Value {
    json!({"major": API.0, "minor": API.1})
}

fn walk<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            if let Some(x) = m.get(key) {
                out.push(x);
            }
            for (k, x) in m {
                if k != key {
                    walk(x, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| walk(x, key, out)),
        _ => {}
    }
}

fn id_of(rec: &Value) -> Option<String> {
    let v = rec.get("monitor_id").or_else(|| rec.get("id"))?;
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Monitor records anywhere in `msg` as `id -> connector`.
pub fn parse_monitors(msg: &Value, into: &mut HashMap<String, String>) {
    let mut arrays = Vec::new();
    walk(msg, "monitors", &mut arrays);
    for a in arrays {
        let recs: Vec<&Value> = match a {
            Value::Array(a) => a.iter().collect(),
            Value::Object(m) => m.values().collect(),
            _ => continue,
        };
        for rec in recs {
            let Some(id) = id_of(rec) else { continue };
            let connector = rec
                .get("output_names")
                .and_then(Value::as_array)
                .and_then(|a| a.iter().find_map(Value::as_str))
                .or_else(|| rec.get("connector").and_then(Value::as_str))
                .or_else(|| rec.get("name").and_then(Value::as_str));
            if let Some(c) = connector {
                into.insert(id, c.to_owned());
            }
        }
    }
}

/// A layer record before its monitor id is resolved to a connector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLayer {
    pub namespace: String,
    pub monitor_id: String,
    pub mapped: bool,
}

/// Layer records in a `layer.list` result.
pub fn parse_layers(msg: &Value) -> Option<Vec<RawLayer>> {
    let mut found = Vec::new();
    walk(msg, "layers", &mut found);
    let list = found.into_iter().find_map(Value::as_array)?;
    Some(
        list.iter()
            .filter_map(|rec| {
                Some(RawLayer {
                    namespace: rec.get("namespace")?.as_str()?.to_owned(),
                    monitor_id: id_of(&json!({"monitor_id": rec.get("monitor_id")?}))?,
                    mapped: rec.get("mapped").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect(),
    )
}

/// Resolve monitor ids to connectors; an unknown id stays as it is.
pub fn resolve(layers: &[RawLayer], monitors: &HashMap<String, String>) -> Vec<LayerSurface> {
    layers
        .iter()
        .map(|l| LayerSurface {
            namespace: l.namespace.clone(),
            connector: monitors.get(&l.monitor_id).cloned().unwrap_or_else(|| l.monitor_id.clone()),
            mapped: l.mapped,
        })
        .collect()
}

pub fn spawn(tx: calloop::channel::Sender<Msg>) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name("bsm-host-layers".into()).spawn(move || {
        let path = socket_path();
        loop {
            match UnixStream::connect(&path) {
                Ok(stream) => {
                    log::info!("connected to {}", path.display());
                    if let Err(e) = session(stream, &tx) {
                        log::warn!("gnoblin control socket: {e}");
                    }
                }
                Err(_) => {}
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    })
}

fn session(stream: UnixStream, tx: &calloop::channel::Sender<Msg>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    let mut w = stream.try_clone()?;
    let mut r = BufReader::with_capacity(64 * 1024, stream);
    let send = |w: &mut UnixStream, v: Value| -> std::io::Result<()> {
        let mut line = v.to_string();
        line.push('\n');
        w.write_all(line.as_bytes())
    };
    send(&mut w, json!({"api_version": version()}))?;
    send(
        &mut w,
        json!({"op":"events","api_version":version(),"events":[
            "gnoblin.layer.created","gnoblin.layer.changed","gnoblin.layer.removed",
            "gnoblin.monitor.added","gnoblin.monitor.changed","gnoblin.monitor.removed",
            "gnoblin.session.lock-requested"]}),
    )?;
    send(&mut w, json!({"op":"monitors","api_version":version()}))?;
    let query = |w: &mut UnixStream| -> std::io::Result<()> {
        send(w, json!({"op":"api","id":"layers","api_version":version(),"method":"layer.list","arguments":{}}))
    };
    query(&mut w)?;

    let mut monitors: HashMap<String, String> = HashMap::new();
    let mut raw: Option<Vec<RawLayer>> = None;
    let mut last_sent: Option<Vec<LayerSurface>> = None;
    let mut last_query = Instant::now();
    let mut line = String::new();
    loop {
        line.clear();
        match r.read_line(&mut line) {
            Ok(0) => return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "closed")),
            Ok(_) => {}
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                if last_query.elapsed() >= REQUERY {
                    last_query = Instant::now();
                    send(&mut w, json!({"op":"monitors","api_version":version()}))?;
                    query(&mut w)?;
                }
                continue;
            }
            Err(e) => return Err(e),
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let event = msg.get("event").and_then(Value::as_str).unwrap_or("");
        if event == "gnoblin.session.lock-requested" {
            let _ = tx.send(Msg::LockRequested);
            continue;
        }
        parse_monitors(&msg, &mut monitors);
        if event.starts_with("gnoblin.layer.") || event.starts_with("gnoblin.monitor.") {
            query(&mut w)?;
        }
        if msg.get("id").and_then(Value::as_str) == Some("layers") {
            if let Some(l) = parse_layers(&msg) {
                raw = Some(l);
            }
        }
        if let Some(raw) = &raw {
            let resolved = resolve(raw, &monitors);
            if last_sent.as_ref() != Some(&resolved) {
                last_sent = Some(resolved.clone());
                let _ = tx.send(Msg::Layers(resolved));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_monitor_snapshot_and_layers() {
        let mut monitors = HashMap::new();
        parse_monitors(
            &json!({"event":"monitors","monitors":[{"id":"m1","output_names":["Meta-0"]}, {"monitor_id":"m2","output_names":["Meta-1"]}]}),
            &mut monitors,
        );
        assert_eq!(monitors["m1"], "Meta-0");
        assert_eq!(monitors["m2"], "Meta-1");
        let msg = json!({"event":"reply","id":"layers","result":{"layers":[
            {"id":"l1","namespace":"bingux-wallpaper","monitor_id":"m1","layer":"background","mapped":true},
            {"id":"l2","namespace":"bingux-dock","monitor_id":"m2","mapped":false}]}});
        let layers = resolve(&parse_layers(&msg).unwrap(), &monitors);
        assert_eq!(
            layers,
            vec![
                LayerSurface { namespace: "bingux-wallpaper".into(), connector: "Meta-0".into(), mapped: true },
                LayerSurface { namespace: "bingux-dock".into(), connector: "Meta-1".into(), mapped: false },
            ]
        );
    }

    #[test]
    fn unmapped_by_default_and_unknown_monitor_keeps_id() {
        let msg = json!({"result":{"layers":[{"namespace":"x","monitor_id":"zz"}]}});
        let layers = resolve(&parse_layers(&msg).unwrap(), &HashMap::new());
        assert!(!layers[0].mapped);
        assert_eq!(layers[0].connector, "zz");
    }
}
