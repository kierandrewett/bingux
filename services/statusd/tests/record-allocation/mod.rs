//! Private, synthetic record-encoding checks: no sampler, socket or audio server.
use super::record_json;
use bingux_statusd::{
    DesktopState, InputSource, Metrics,
    extra::ExtraMetrics,
    hardware::Process,
    microphone::{Capture, CaptureSnapshot},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default)]
struct Allocations {
    calls: usize,
    requested_bytes: usize,
}

thread_local! {
    static COUNT: Cell<Option<Allocations>> = const { Cell::new(None) };
}

struct CountingAllocator;

fn count(size: usize) {
    let _ = COUNT.try_with(|count| {
        if let Some(mut total) = count.get() {
            total.calls += 1;
            total.requested_bytes += size;
            count.set(Some(total));
        }
    });
}

// Every request is delegated unchanged to System. Thread-local accounting only
// observes the measured closure, excluding fixture setup and other test threads.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Measuring;

impl Drop for Measuring {
    fn drop(&mut self) {
        COUNT.with(|count| count.set(None));
    }
}

fn measured<T>(operation: impl FnOnce() -> T) -> (T, Allocations) {
    COUNT.with(|count| {
        assert!(count.get().is_none());
        count.set(Some(Allocations::default()));
    });
    let guard = Measuring;
    let value = operation();
    let allocations = COUNT.with(|count| count.get().unwrap());
    drop(guard);
    (value, allocations)
}

fn metrics(processes: usize) -> Metrics {
    let mut extra = ExtraMetrics {
        sampled_at_ms: 1_000,
        ..Default::default()
    };
    extra.hardware.process_count = processes;
    extra.hardware.processes_sampled_at_ms = 900;
    extra.hardware.processes = (0..processes)
        .map(|index| Process {
            pid: index as u32 + 100,
            name: format!("synthetic-{index}"),
            executable: "example".into(),
            argv: vec![
                "/usr/bin/example".into(),
                "two words".into(),
                String::new(),
                "quote\" slash\\ newline\n café".into(),
            ],
            start_time: 123,
            cpu_percent: None,
            memory_bytes: 4096,
            threads: 1,
            state: "S".into(),
        })
        .collect();
    Metrics {
        extra,
        cpu_percent: Some(12.5),
        memory_total_bytes: 16_777_216,
        memory_used_bytes: 12_582_912,
        receive_bytes_per_second: Some(1_234.0),
        transmit_bytes_per_second: None,
    }
}

fn encode(metrics: &Metrics, desktop: &DesktopState, microphone: &CaptureSnapshot) -> String {
    record_json(metrics, desktop, microphone).unwrap()
}

#[test]
fn record_encoding_allocation_budget() {
    let desktop = DesktopState::default();
    let microphone = CaptureSnapshot::default();
    let mut maximum_calls = 0;
    for processes in [0, 1, 512] {
        let metrics = metrics(processes);
        let (record, allocations) = measured(|| encode(&metrics, &desktop, &microphone));
        maximum_calls = maximum_calls.max(allocations.calls);
        let mut timings = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            for _ in 0..64 {
                black_box(encode(black_box(&metrics), &desktop, &microphone));
            }
            timings.push(start.elapsed().as_nanos() / 64);
        }
        timings.sort_unstable();
        println!(
            "RECORD_ALLOCATION processes={processes} calls={} requested_bytes={} record_bytes={} median_ns={}",
            allocations.calls,
            allocations.requested_bytes,
            record.len(),
            timings[3],
        );
        // Optional artifact for byte-for-byte comparison with the baseline.
        if let Some(directory) = std::env::var_os("BINGUX_RECORD_EVIDENCE_DIR") {
            let directory = std::path::Path::new(&directory);
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(directory.join(format!("{processes}.json")), record).unwrap();
        }
    }
    // The output buffers may grow; allocating individual copies of every
    // process/string is not necessary for serialization of an existing sample.
    assert!(
        maximum_calls <= 64,
        "{maximum_calls} allocations per record"
    );
}

#[test]
fn record_encoding_preserves_cached_metrics_and_argv() {
    let metrics = metrics(2);
    let before = metrics.clone();
    let desktop = DesktopState::default();
    let microphone = CaptureSnapshot::default();
    let first = encode(&metrics, &desktop, &microphone);
    assert_eq!(first, encode(&metrics, &desktop, &microphone));
    assert_eq!(metrics, before);
    assert_eq!(first.lines().count(), 1);
    assert!(first.ends_with('\n'));
    let value: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(value["protocolVersion"], 1);
    assert_eq!(value["type"], "metrics");
    assert_eq!(value["cpuPercent"], 12.5);
    assert!(value["networkTransmitBytesPerSecond"].is_null());
    assert_eq!(
        value["extra"],
        serde_json::to_value(&metrics.extra).unwrap()
    );
}

#[test]
fn record_encoding_uses_new_desktop_state_with_the_same_sample() {
    let metrics = metrics(1);
    let before = encode(
        &metrics,
        &DesktopState::default(),
        &CaptureSnapshot::default(),
    );
    let source = InputSource {
        source_type: "xkb".into(),
        id: "gb".into(),
        short_name: "en".into(),
        display_name: "English \"United Kingdom\"".into(),
    };
    let mut desktop = DesktopState {
        available: true,
        input_sources: vec![source.clone()],
        current_input_source: Some(source),
        ..Default::default()
    };
    desktop.privacy.microphone_available = true;
    desktop.privacy.microphone_in_use = true;
    let microphone = CaptureSnapshot {
        available: true,
        captures: vec![Capture {
            app: "Synthetic recorder".into(),
            app_id: "org.example.Recorder".into(),
            device: "Fixture only".into(),
            muted: true,
        }],
    };
    let after = encode(&metrics, &desktop, &microphone);
    let before_value: serde_json::Value = serde_json::from_str(&before).unwrap();
    let after: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(before_value["extra"], after["extra"]);
    assert_eq!(
        after["inputSources"],
        serde_json::to_value(&desktop.input_sources).unwrap()
    );
    assert_eq!(after["currentInputSource"], after["inputSources"][0]);
    assert_eq!(after["microphoneInUse"], true);
    assert_eq!(after["microphoneCapturesAvailable"], true);
    assert_eq!(
        after["microphoneCaptures"],
        serde_json::to_value(&microphone.captures).unwrap()
    );
    assert_eq!(
        encode(
            &metrics,
            &DesktopState::default(),
            &CaptureSnapshot::default()
        ),
        before
    );
}

#[test]
fn record_encoding_omits_default_extra() {
    let mut metrics = metrics(0);
    metrics.extra = ExtraMetrics::default();
    let record = encode(
        &metrics,
        &DesktopState::default(),
        &CaptureSnapshot::default(),
    );
    let value: serde_json::Value = serde_json::from_str(&record).unwrap();
    assert!(value.get("extra").is_none());
    assert_eq!(value["desktopStateAvailable"], false);
    assert!(value["currentInputSource"].is_null());
    assert_eq!(value["microphoneCapturesAvailable"], false);
    assert_eq!(value["microphoneCaptures"], serde_json::json!([]));
}
