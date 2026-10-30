use bingux_statusd::microphone::{CaptureSnapshot, parse_snapshot};

#[test]
fn snapshot_keeps_real_microphone_owners_and_excludes_monitors_and_peaks() {
    let sources = r#"[
      {"index":1,"name":"alsa_input.pci","description":"Built-in Mic","mute":false},
      {"index":2,"name":"alsa_output.pci.monitor","description":"Monitor","mute":false}
    ]"#;
    let outputs = r#"[
      {"source":1,"corked":false,"mute":false,"properties":{"application.name":"Recorder","application.id":"org.example.Recorder"}},
      {"source":2,"corked":false,"mute":false,"properties":{"application.name":"Meter"}},
      {"source":1,"corked":false,"mute":false,"properties":{"application.name":"Peaks","resample.peaks":true}},
      {"source":1,"corked":true,"mute":false,"properties":{"application.name":"Paused"}},
      {"source":1,"corked":false,"mute":true,"properties":{"application.name":"Recorder","application.id":"org.example.Recorder"}}
    ]"#;

    assert_eq!(
        parse_snapshot(sources, outputs).unwrap(),
        CaptureSnapshot {
            available: true,
            captures: vec![
                bingux_statusd::microphone::Capture {
                    app: "Recorder".to_owned(),
                    app_id: "org.example.Recorder".to_owned(),
                    device: "Built-in Mic".to_owned(),
                    muted: false,
                },
                bingux_statusd::microphone::Capture {
                    app: "Recorder".to_owned(),
                    app_id: "org.example.Recorder".to_owned(),
                    device: "Built-in Mic".to_owned(),
                    muted: true,
                },
            ],
        }
    );
}

#[test]
fn snapshot_rejects_unbounded_or_malformed_input() {
    assert!(parse_snapshot("not json", "[]").is_err());
    let sources = r#"[{"index":1,"name":"mic","description":"Mic","mute":false}]"#;
    let outputs = format!(
        "[{{\"source\":1,\"corked\":false,\"mute\":false,\"properties\":{{\"application.name\":\"{}\"}}}}]",
        "x".repeat(257)
    );
    assert!(parse_snapshot(sources, &outputs).is_err());
}
