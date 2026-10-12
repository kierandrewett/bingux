#!/usr/bin/env python3
"""Compile the actual std-only reaper and observe its Linux task wakeups."""

import argparse
from pathlib import Path
import subprocess
import re
import tempfile

ROOT = Path(__file__).resolve().parents[1]

HARNESS = r"""
use std::{process::{Child, Command}, sync::{Arc, atomic::{AtomicUsize, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError}}, thread, time::{Duration, Instant}};
unsafe extern "C" { fn gettid() -> i32; }
fn switches(tid: i32) -> u64 {
    let text = std::fs::read_to_string(format!("/proc/self/task/{tid}/status")).unwrap();
    text.lines().find_map(|line| line.strip_prefix("voluntary_ctxt_switches:"))
        .unwrap().trim().parse().unwrap()
}
fn main() {
    let (sender, receiver) = mpsc::channel();
    let (started, ready) = mpsc::channel();
    let worker = thread::spawn(move || {
        started.send(unsafe { gettid() }).unwrap();
        reap_launched_programs(receiver);
    });
    let tid = ready.recv().unwrap();
    thread::sleep(Duration::from_millis(50));
    let before = switches(tid);
    thread::sleep(Duration::from_millis(650));
    let idle_wakeups = switches(tid) - before;
    println!("idle_reaper_voluntary_switches={idle_wakeups} interval_ms=650");
    let active = Arc::new(AtomicUsize::new(1));
    let child = Command::new("/bin/sleep").arg("0.15").spawn().unwrap();
    sender.send(ReapedChild { child, _reservation: ProgramReservation {
        active: Arc::clone(&active) } }).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while active.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(active.load(Ordering::Acquire), 0, "new child must wake and be reaped");
    // Disconnect while a live child remains: drain it before exiting.
    active.store(1, Ordering::Release);
    let child = Command::new("/bin/sleep").arg("0.15").spawn().unwrap();
    sender.send(ReapedChild { child, _reservation: ProgramReservation {
        active: Arc::clone(&active) } }).unwrap();
    drop(sender);
    worker.join().unwrap();
    assert_eq!(active.load(Ordering::Acquire), 0);
    assert!(idle_wakeups <= 1, "empty reaper must block, not poll");
    println!("PASS: idle block, new-child wake/reap, live-child drain on disconnect");
}
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    source = (ROOT / "packages/bingux-searchd/src/main.rs").read_text()
    start = source.index("struct ReapedChild {")
    structs = source[start : source.index("impl ProgramReaper {", start)]
    start = source.index("fn reap_launched_programs(")
    function = source[start : source.index("fn copy_to_clipboard(", start)]
    with tempfile.TemporaryDirectory(prefix="bingux-reaper-") as temporary:
        output = args.output or Path(temporary)
        output.mkdir(parents=True, exist_ok=True)
        code = output / "reaper.rs"
        code.write_text(
            HARNESS
            + "\n"
            + "\n".join(
                re.findall(r"^const (?:MAX_REAPED_PROGRAMS|PROGRAM_REAPER_POLL_INTERVAL):.*$", source, re.MULTILINE)
            )
            + "\n"
            + structs
            + function
        )
        binary = output / "reaper"
        subprocess.run(["rustc", "--edition=2024", "-Dwarnings", str(code), "-o", str(binary)], check=True)
        return subprocess.run([str(binary)], timeout=5).returncode


if __name__ == "__main__":
    raise SystemExit(main())
