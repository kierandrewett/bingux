//! Timings for the file model. Run in release mode:
//!
//! ```sh
//! cargo run --release --example bench -- /path/to/folder [--network]
//! ```
//!
//! It opens the folder with the loader (once without a cache, then again with
//! it), times the first batch and completion, and sorts and filters 100,000
//! made-up entries. `--network` forces the network policy (names first,
//! sizes lazily). The cache lives in a temporary directory.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use bingux_files::cache::Cache;
use bingux_files::loader::{self, Event, LoadOptions, Policy};
use bingux_files::location::{CompleteOptions, Completer};
use bingux_files::mounts::MountKind;
use bingux_files::*;

fn peak_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|line| line.starts_with("VmHWM"))
        .and_then(|line| line.split_whitespace().nth(1)?.parse::<u64>().ok())
        .unwrap_or(0)
        / 1024
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn synth(count: usize) -> Listing {
    let mut listing = Listing::new("/x");
    let exts = [
        "png", "jpg", "txt", "pdf", "mp3", "mkv", "zip", "rs", "md", "conf",
    ];
    let mut x: u64 = 88172645463325252;
    for i in 0..count {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let dir = i % 50 == 0;
        let name = if dir {
            format!("Folder {}", x % 100000)
        } else {
            format!(
                "{}_{:06}.{}",
                ["IMG", "report", "Photo", "file", "scan"][(x % 5) as usize],
                x % 1_000_000,
                exts[(x >> 8) as usize % 10]
            )
        };
        let mut entry = Entry::named(
            PathBuf::from("/x").join(&name),
            name,
            if dir { Kind::Directory } else { Kind::File },
            false,
            i % 17 == 0,
        );
        entry.size = x % 100000;
        entry.modified = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(x % 1_000_000_000));
        entry.sized = true;
        listing.entries.push(entry);
    }
    listing
}

struct Timings {
    kind: Option<f64>,
    cached: Option<f64>,
    first_batch: Option<f64>,
    done: Option<f64>,
    entries: usize,
}

fn open_once(path: &Path, options: LoadOptions) -> Timings {
    let (sender, receiver) = mpsc::channel();
    let start = Instant::now();
    let load = loader::open(path, options, move |event| {
        let _ = sender.send((Instant::now(), event));
    });
    let mut timings = Timings {
        kind: None,
        cached: None,
        first_batch: None,
        done: None,
        entries: 0,
    };
    let mut listing = Listing::new(path);
    loop {
        let (at, event) = receiver
            .recv_timeout(Duration::from_secs(120))
            .expect("event");
        let t = ms(at - start);
        match &event {
            Event::Kind(_) => timings.kind = Some(t),
            Event::Cached { .. } => timings.cached = Some(t),
            Event::Batch(_) => {
                timings.first_batch.get_or_insert(t);
            }
            Event::Done { .. } => timings.done = Some(t),
            Event::Failed(error) => panic!("{error}"),
            _ => {}
        }
        let done = matches!(event, Event::Done { .. });
        listing.apply_event(event);
        if done {
            break;
        }
    }
    timings.entries = listing.entries.len();
    drop(load);
    timings
}

/// Open the folder, then ask for sizes of the first 60 entries, as a view
/// showing 60 rows would, and time until the last arrives.
fn visible_rows(path: &Path, options: LoadOptions) {
    let (sender, receiver) = mpsc::channel();
    let load = loader::open(path, options, move |event| {
        let _ = sender.send(event);
    });
    let mut listing = Listing::new(path);
    while !listing.complete {
        let event = receiver
            .recv_timeout(Duration::from_secs(120))
            .expect("event");
        listing.apply_event(event);
    }
    let wanted: Vec<usize> = (0..listing.entries.len().min(60)).collect();
    let unverified = wanted.len();
    let start = Instant::now();
    load.want_stats(&listing, &wanted);
    let mut updates = 0;
    let mut last = start;
    while updates < unverified {
        match receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(Event::Stats(batch)) => {
                updates += batch.updates.len();
                last = Instant::now();
                listing.apply_event(Event::Stats(batch));
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    println!(
        "visible rows: asked for {unverified}, {updates} changed answers, last one after {:.1} ms (policy: {:?}, workers {})",
        ms(last - start),
        listing.mount,
        load.policy().workers
    );
}

fn show(label: &str, timings: &Timings) {
    let cell = |value: Option<f64>| value.map_or("-".to_string(), |v| format!("{v:.1} ms"));
    println!(
        "{label}: {} entries; cached paint {}, first batch {}, done {}, kind known {}; peak RSS {} MB",
        timings.entries,
        cell(timings.cached),
        cell(timings.first_batch),
        cell(timings.done),
        cell(timings.kind),
        peak_mb()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = PathBuf::from(args.first().map_or("/var/tmp/Bench/d20k", String::as_str));
    let network = args.iter().any(|a| a == "--network");
    let cache_dir = std::env::temp_dir().join(format!("bingux-bench-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);
    let options = || LoadOptions {
        cache: Some(std::sync::Arc::new(Cache::at(
            &cache_dir,
            bingux_files::cache::DEFAULT_BUDGET,
            512,
        ))),
        watch: false,
        policy: network.then(|| Policy::for_kind(MountKind::Network)),
        ..LoadOptions::default()
    };

    println!(
        "== {} ({})",
        path.display(),
        if network {
            "network policy forced"
        } else {
            "policy from the mount table"
        }
    );
    let first = open_once(&path, options());
    show("open 1 (no cache)", &first);
    std::thread::sleep(Duration::from_millis(300));
    for run in 2..=4 {
        let again = open_once(&path, options());
        show(&format!("open {run} (cached)"), &again);
    }
    visible_rows(&path, options());
    let (bytes, files) = Cache::at(&cache_dir, 1 << 30, 1 << 20).disk_usage();
    println!("cache on disk: {bytes} bytes in {files} file(s)");
    let blocking = Instant::now();
    let listing = list_dir(&path, &Cancel::new()).unwrap();
    println!(
        "list_dir (blocking, every entry statx'ed): {:.1} ms for {} entries",
        ms(blocking.elapsed()),
        listing.entries.len()
    );
    let _ = std::fs::remove_dir_all(&cache_dir);

    let big = synth(100_000);
    for (name, key) in [
        ("name", SortKey::Name),
        ("size", SortKey::Size),
        ("modified", SortKey::Modified),
        ("kind", SortKey::Kind),
    ] {
        let view = View {
            show_hidden: true,
            filter: None,
            directories_only: false,
            sort: Sort::by(key),
        };
        let t = Instant::now();
        let order = big.select(&view);
        println!(
            "sort 100k by {name}: {:.1} ms ({})",
            ms(t.elapsed()),
            order.len()
        );
    }
    let globs = Filter::new("Images").glob("*.png").glob("*.jpg");
    let view = View {
        show_hidden: true,
        filter: Some(&globs),
        directories_only: false,
        sort: Sort::by(SortKey::Name),
    };
    let t = Instant::now();
    let order = big.select(&view);
    println!(
        "filter (*.png, *.jpg) + name sort 100k: {:.1} ms ({} shown)",
        ms(t.elapsed()),
        order.len()
    );
    let unsorted = View {
        sort: Sort {
            key: SortKey::Name,
            descending: false,
            folders_first: false,
        },
        ..view
    };
    let _ = unsorted;
    let matcher = globs.matcher();
    let t = Instant::now();
    let shown = big.entries.iter().filter(|e| matcher.matches(e)).count();
    println!(
        "filter only (*.png, *.jpg) 100k: {:.1} ms ({shown} shown)",
        ms(t.elapsed())
    );
    let mimes = Filter::images();
    let matcher = mimes.matcher();
    let t = Instant::now();
    let shown = big.entries.iter().filter(|e| matcher.matches(e)).count();
    println!(
        "filter only (image/*) 100k: {:.1} ms ({shown} shown)",
        ms(t.elapsed())
    );
    let t = Instant::now();
    let counted = big.select_counted(&View {
        show_hidden: false,
        filter: Some(&globs),
        directories_only: false,
        sort: Sort::default(),
    });
    println!(
        "select_counted hidden off + filter + name sort 100k: {:.1} ms ({} shown, {} by filter, {} dotfiles)",
        ms(t.elapsed()),
        counted.order.len(),
        counted.hidden_by_filter,
        counted.hidden_dotfiles
    );

    let mut completer = Completer::new();
    for (typed, label) in [("Photo_1", "prefix"), ("zz", "no match")] {
        let input = format!("{}/{typed}", path.display());
        for run in 0..3 {
            let t = Instant::now();
            let found = completer.complete(
                &input,
                &path,
                &path,
                CompleteOptions::default(),
                &Cancel::new(),
            );
            println!(
                "complete {label} run{run}: {:.2} ms ({} shown, prefix matches {}, truncated {}, incomplete {})",
                ms(t.elapsed()),
                found.candidates.len(),
                found.prefix_matches,
                found.truncated,
                found.incomplete
            );
        }
    }
    println!("peak RSS {} MB", peak_mb());
}
