//! Daily rollups across a real local midnight, with `TZ` set.
//!
//! This is its own test binary with a single test because `TZ` is process
//! wide: nothing else may read the clock zone while it runs.

use bingux_data_usage::{
    Config, Date, Granularity, NoConnections, Reader, Recorder, Scope, UsageSource, Zone,
};
use std::{fs, path::Path};

fn interface(root: &Path, rx: u64) {
    let dir = root.join("class/net/enp3s0");
    fs::create_dir_all(dir.join("statistics")).unwrap();
    fs::create_dir_all(dir.join("device")).unwrap();
    fs::write(dir.join("type"), "1\n").unwrap();
    fs::write(dir.join("ifindex"), "2\n").unwrap();
    fs::write(dir.join("statistics/rx_bytes"), format!("{rx}\n")).unwrap();
    fs::write(dir.join("statistics/tx_bytes"), "0\n").unwrap();
}

#[test]
fn days_follow_local_midnight_and_dst() {
    // SAFETY: this binary has exactly one test and no other thread has been
    // started before this line, so nothing reads the environment concurrently.
    unsafe { std::env::set_var("TZ", "Europe/London") };

    let dir = std::env::temp_dir().join(format!("bingux-data-usage-tz-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let sysfs = dir.join("sys");
    let proc_root = dir.join("proc");
    fs::create_dir_all(proc_root.join("sys/kernel/random")).unwrap();
    fs::write(proc_root.join("sys/kernel/random/boot_id"), "tz\n").unwrap();
    interface(&sysfs, 0);

    let mut config = Config::new(dir.join("usage.sqlite3"));
    config.sysfs_root = sysfs.clone();
    config.proc_root = proc_root;
    config.resolver = Box::new(NoConnections);
    config.log = Box::new(|_| {});
    assert_eq!(config.zone, Zone::Local);
    let recorder = Recorder::new(config);

    // Clocks go back at 02:00 on 2026-10-25: that local day is 25 hours long.
    let zone = Zone::Local;
    let saturday = zone.date_start(Date::new(2026, 10, 24).unwrap());
    let sunday = zone.date_start(Date::new(2026, 10, 25).unwrap());
    let monday = zone.date_start(Date::new(2026, 10, 26).unwrap());
    assert_eq!(sunday - saturday, 24 * 3600, "BST day");
    assert_eq!(monday - sunday, 25 * 3600, "DST ends: 25 hour day");

    // One byte a minute from 23:00 on Saturday to midnight on Tuesday.
    let begin = sunday - 3600;
    let end = monday + 86_400;
    recorder.sample_at(begin).unwrap();
    let mut raw = 0;
    let mut minute = begin + 60;
    while minute < end {
        raw += 1;
        interface(&sysfs, raw);
        recorder.sample_at(minute).unwrap();
        minute += 60;
    }

    let reader = Reader::open(dir.join("usage.sqlite3")).unwrap();
    assert_eq!(reader.zone(), Zone::Local);
    let days = reader
        .series(&Scope::All, Granularity::Day, saturday, end)
        .unwrap();
    assert_eq!(
        days.iter().map(|day| day.start).collect::<Vec<_>>(),
        [saturday, sunday, monday]
    );
    // Saturday: the 60 minutes to midnight (the sample at 00:00 sharp
    // belongs to the new day's bucket). Sunday: 25 hours. Monday: 24.
    assert_eq!(days[0].rx, 59);
    assert_eq!(days[1].rx, 25 * 60);
    assert_eq!(days[2].rx, 24 * 60);
    let total = reader.totals(&Scope::All, saturday, end).unwrap().rx;
    assert_eq!(total, days.iter().map(|day| day.rx).sum::<u64>());
    assert_eq!(total, raw);
    // Whole-day totals come from the daily table and agree with the minutes.
    let sunday_only = reader.totals(&Scope::All, sunday, monday).unwrap();
    assert_eq!(sunday_only.rx, 25 * 60);

    let _ = fs::remove_dir_all(&dir);
}
