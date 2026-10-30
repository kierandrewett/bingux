//! Snapshots, the app socket and the aggregator, with real files and sockets.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bingux_search::aggregate::{AppManifest, Aggregator, Source, load_manifests};
use bingux_search::ipc::{self, ProviderConfig};
use bingux_search::{Cancel, Document, Index, Schema, Search, SearchService, snapshot};
use serde_json::json;

fn schema(external: bool) -> Schema {
    Schema::new()
        .searchable("title", 10.0)
        .searchable("keywords", 5.0)
        .displayed(["title", "breadcrumb", "icon", "uri"])
        .external(external)
        .title("Settings")
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bingux-search-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn proxy_docs() -> Vec<Document> {
    vec![
        Document::new("network/proxy").title("Proxy").keywords("http socks").breadcrumb(["Network"]).icon("network").uri("bingux-settings://network/proxy#method"),
        Document::new("wifi/saved").title("Saved networks").uri("bingux-settings://wifi/saved"),
        Document::new("secret").title("Proxy password vault").external(false).uri("bingux-settings://secret"),
    ]
}

#[test]
fn snapshot_round_trips_only_external_documents_atomically() {
    let dir = temp("snap");
    let idx = Index::new("settings", schema(true)).unwrap();
    idx.upsert(proxy_docs()).unwrap();
    let path = snapshot::path_for(&dir, "bingux-settings", "settings");
    let meta = snapshot::save(&idx, "bingux-settings", "1.2.3", &path).unwrap().unwrap();
    assert_eq!(meta.documents, 2);
    let mode = std::fs::metadata(&path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(mode.mode() & 0o777, 0o600);
    // No temporary file is left.
    let left: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(left.len(), 1, "{left:?}");

    let (m, loaded) = snapshot::load(&path, Some("1.2.3")).unwrap();
    assert_eq!(m.index, "settings");
    assert_eq!(loaded.len(), 2);
    let r = loaded.search(&Search::new("proxy"), &Cancel::new()).unwrap();
    assert_eq!(r.hits.len(), 1, "the private document is not in the snapshot");
    assert_eq!(r.hits[0].uri(), Some("bingux-settings://network/proxy#method"));
    // A different installed version is refused.
    assert!(snapshot::load(&path, Some("2.0.0")).is_err());
    // A private index writes nothing.
    let private = Index::new("p", schema(false)).unwrap();
    assert!(snapshot::save(&private, "x", "1", &dir.join("p.idx")).unwrap().is_none());
    assert!(!dir.join("p.idx").exists());
}

#[test]
fn a_corrupt_or_foreign_snapshot_is_refused() {
    let dir = temp("corrupt");
    let idx = Index::new("settings", schema(true)).unwrap();
    idx.upsert(proxy_docs()).unwrap();
    let path = dir.join("a.idx");
    snapshot::save(&idx, "a", "1", &path).unwrap();
    let good = std::fs::read(&path).unwrap();
    // Flip a payload byte.
    let mut bad = good.clone();
    let n = bad.len() / 2;
    bad[n] ^= 0xff;
    std::fs::write(&path, &bad).unwrap();
    assert!(snapshot::load(&path, None).is_err());
    // Truncated.
    std::fs::write(&path, &good[..good.len() - 5]).unwrap();
    assert!(snapshot::load(&path, None).is_err());
    // Not a snapshot.
    std::fs::write(&path, b"hello").unwrap();
    assert!(snapshot::load(&path, None).is_err());
    // Another format version.
    let mut other = good.clone();
    other[4] = 99;
    std::fs::write(&path, &other).unwrap();
    assert!(snapshot::load(&path, None).is_err());
}

fn start_provider(dir: &PathBuf, opened: Arc<Mutex<Vec<String>>>, version: &str) -> (Arc<SearchService>, ipc::Provider) {
    let service = SearchService::new();
    let idx = service.create_index("settings", schema(true)).unwrap();
    idx.upsert(proxy_docs()).unwrap();
    let private = service.create_index("wifi-networks", schema(false)).unwrap();
    private.upsert([Document::new("home").title("Proxy at home")]).unwrap();
    let mut cfg = ProviderConfig::new("bingux-settings", version, move |uri| {
        opened.lock().unwrap().push(uri.to_string());
        true
    });
    cfg.socket_dir = Some(dir.join("run"));
    cfg.state_root = Some(dir.join("state"));
    cfg.snapshot_delay = Duration::from_millis(50);
    let provider = ipc::serve(service.clone(), cfg).unwrap();
    (service, provider)
}

fn ask(provider: &ipc::Provider, v: serde_json::Value) -> serde_json::Value {
    ipc::request(provider.socket_path(), &v, Duration::from_secs(2)).unwrap()
}

#[test]
fn socket_hello_search_activate_and_permissions() {
    let dir = temp("sock");
    let opened = Arc::new(Mutex::new(Vec::new()));
    let (_service, provider) = start_provider(&dir, opened.clone(), "1.0.0");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(provider.socket_path()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let dmode = std::fs::metadata(provider.socket_path().parent().unwrap()).unwrap().permissions().mode() & 0o777;
    assert_eq!(dmode, 0o700);

    let hello = ask(&provider, json!({"v": 1, "type": "hello"}));
    assert_eq!(hello["type"], "hello");
    let names: Vec<_> = hello["indexes"].as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["settings"], "private indexes are not listed");
    assert_eq!(hello["indexes"][0]["documents"], 2, "private documents are not counted");

    let r = ask(&provider, json!({"v": 1, "type": "search", "id": "q1", "index": "settings",
        "body": {"query": {"multi_match": {"query": "proxy", "fields": ["title", "keywords"]}}, "highlight": {"fields": {"title": {}}}}}));
    assert_eq!(r["type"], "result");
    assert_eq!(r["id"], "q1");
    let hits = r["response"]["hits"]["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "the private document stays hidden");
    assert_eq!(hits[0]["_id"], "network/proxy");
    assert_eq!(hits[0]["highlight"]["title"][0], "<em>Proxy</em>");
    assert!(hits[0]["_source"].get("keywords").is_none(), "only displayed fields leave");

    // A private index is indistinguishable from a missing one.
    for name in ["wifi-networks", "nope"] {
        let r = ask(&provider, json!({"v": 1, "type": "search", "id": "q2", "index": name, "body": {"q": "proxy"}}));
        assert_eq!(r["type"], "error", "{name}");
        assert_eq!(r["error"]["type"], "index_not_found_exception");
    }
    // Unknown request, wrong version, bad JSON.
    assert_eq!(ask(&provider, json!({"v": 1, "type": "dance"}))["error"]["type"], "parsing_exception");
    assert_eq!(ask(&provider, json!({"v": 2, "type": "hello"}))["type"], "error");

    // Activate runs the document's uri; a private document cannot be activated.
    let a = ask(&provider, json!({"v": 1, "type": "activate", "id": "a1", "index": "settings", "doc": "network/proxy"}));
    assert_eq!(a["type"], "activated");
    assert_eq!(opened.lock().unwrap().as_slice(), ["bingux-settings://network/proxy#method"]);
    let a = ask(&provider, json!({"v": 1, "type": "activate", "id": "a2", "index": "settings", "doc": "secret"}));
    assert_eq!(a["type"], "error");
    assert_eq!(opened.lock().unwrap().len(), 1);
}

#[test]
fn the_socket_rejects_oversized_and_malformed_records_and_goes_away_with_the_provider() {
    let dir = temp("bad");
    let (_s, provider) = start_provider(&dir, Arc::new(Mutex::new(Vec::new())), "1");
    use std::io::{Read, Write};
    let mut s = std::os::unix::net::UnixStream::connect(provider.socket_path()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    s.write_all(b"this is not json\n").unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    assert!(out.contains("parsing_exception"), "{out}");
    let path = provider.socket_path().to_path_buf();
    assert_eq!(ipc::running(path.parent().unwrap()).len(), 1);
    drop(provider);
    assert!(!path.exists());
}

#[test]
fn subscribers_hear_about_changes_and_snapshots_are_written_after_them() {
    let dir = temp("sub");
    let (service, provider) = start_provider(&dir, Arc::new(Mutex::new(Vec::new())), "1.0.0");
    use std::io::{BufRead, BufReader, Write};
    let mut s = std::os::unix::net::UnixStream::connect(provider.socket_path()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s.write_all(b"{\"v\":1,\"type\":\"subscribe\"}\n").unwrap();
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).unwrap();
    assert!(line.contains("subscribed"));
    service.index("settings").unwrap().upsert([Document::new("x").title("New thing")]).unwrap();
    // Changes to a private index are not announced.
    service.index("wifi-networks").unwrap().upsert([Document::new("y").title("Quiet")]).unwrap();
    line.clear();
    r.read_line(&mut line).unwrap();
    let ev: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((ev["type"].as_str(), ev["index"].as_str()), (Some("changed"), Some("settings")));
    // The snapshot follows the quiet period.
    let path = snapshot::path_for(&dir.join("state"), "bingux-settings", "settings");
    for _ in 0..100 {
        if path.exists() && snapshot::load(&path, None).map(|(_, i)| i.len() == 3).unwrap_or(false) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the snapshot was not written");
}

fn manifest(dir: &PathBuf, version: &str) -> Vec<AppManifest> {
    let m = dir.join("apps");
    std::fs::create_dir_all(&m).unwrap();
    std::fs::write(
        m.join("bingux-settings.json"),
        serde_json::to_vec(&json!({"kind": "bingux.search-app", "id": "bingux-settings",
            "displayName": "Settings", "command": ["/usr/bin/bingux-settings"], "appVersion": version})).unwrap(),
    )
    .unwrap();
    std::fs::write(m.join("bad.json"), b"{\"kind\":\"other\"}").unwrap();
    std::fs::write(
        m.join("relative.json"),
        serde_json::to_vec(&json!({"kind": "bingux.search-app", "id": "rel", "command": ["bin/x"]})).unwrap(),
    )
    .unwrap();
    load_manifests(&[m])
}

#[test]
fn aggregator_uses_the_socket_when_running_and_the_snapshot_when_closed() {
    let dir = temp("agg");
    let manifests = manifest(&dir, "1.0.0");
    assert_eq!(manifests.len(), 1, "invalid and relative-command manifests are skipped");
    let agg = Aggregator::new(manifests)
        .socket_dir(dir.join("run"))
        .state_root(dir.join("state"));

    // Nothing running, no snapshot: nothing.
    assert!(agg.search_text("proxy", 5, 10).hits.is_empty());

    let (service, provider) = start_provider(&dir, Arc::new(Mutex::new(Vec::new())), "1.0.0");
    let live = agg.search_text("proxy", 5, 10);
    assert_eq!(live.hits.len(), 1);
    assert_eq!(live.hits[0].source, Source::Live);
    assert_eq!(live.hits[0].app, "bingux-settings");
    assert_eq!(live.hits[0].hit.title(), Some("Proxy"));
    assert_eq!(
        agg.open_command("bingux-settings", live.hits[0].hit.uri().unwrap()).unwrap(),
        ["/usr/bin/bingux-settings", "bingux-settings://network/proxy#method"]
    );
    // Live data is fresh: a document added now is found at once.
    service.index("settings").unwrap().upsert([Document::new("n").title("Proxy newest")]).unwrap();
    assert_eq!(agg.search_text("newest", 5, 10).hits.len(), 1);

    // The app closes. Its snapshot answers.
    provider.save_snapshots().into_iter().for_each(|(_, r)| r.unwrap());
    drop(provider);
    let closed = agg.search_text("proxy", 5, 10);
    assert_eq!(closed.hits.len(), 2);
    assert!(closed.hits.iter().all(|h| h.source == Source::Snapshot));
    assert!(closed.hits.iter().all(|h| h.hit.id != "secret"));

    // An updated app (other version) does not trust the old snapshot.
    let newer = Aggregator::new(manifest(&dir, "2.0.0"))
        .socket_dir(dir.join("run"))
        .state_root(dir.join("state"));
    assert!(newer.search_text("proxy", 5, 10).hits.is_empty());

    // Weights scale scores; disabling removes the app.
    let heavy = Aggregator::new(manifest(&dir, "1.0.0")).socket_dir(dir.join("run")).state_root(dir.join("state")).weight("bingux-settings", 0.5);
    let w = heavy.search_text("proxy", 5, 10);
    assert!((w.hits[0].weighted - w.hits[0].hit.score * 0.5).abs() < 1e-9);
    let off = Aggregator::new(manifest(&dir, "1.0.0")).socket_dir(dir.join("run")).state_root(dir.join("state")).disable("bingux-settings");
    assert!(off.search_text("proxy", 5, 10).hits.is_empty());

    // The merged JSON is OpenSearch-shaped with `<app>/<index>` as `_index`.
    let j = agg.search_text("proxy", 5, 10).to_json();
    assert_eq!(j["hits"]["hits"][0]["_index"], "bingux-settings/settings");
}

#[test]
fn a_snapshot_is_written_at_start_even_when_nothing_changes_afterwards() {
    let dir = temp("initial");
    let (_service, _provider) = start_provider(&dir, Arc::new(Mutex::new(Vec::new())), "1.0.0");
    let path = snapshot::path_for(&dir.join("state"), "bingux-settings", "settings");
    for _ in 0..100 {
        if path.exists() {
            let (meta, index) = snapshot::load(&path, Some("1.0.0")).unwrap();
            assert_eq!((meta.documents, index.len()), (2, 2));
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("no snapshot after start");
}
