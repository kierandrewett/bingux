//! Development runner: records usage for a while, then prints what the
//! reader sees.
//!
//!     cargo run --example record -- [--db PATH] [--interval SECS] [--duration SECS]
//!
//! Without `--db` it writes to the real database (`default_path()`), the
//! same file `bingux-statusd` records into; give a temp path when testing.
//! Without `--duration` it runs until interrupted.

use bingux_data_usage::{
    Config, Granularity, Periods, Reader, Recorder, Scope, Stop, UsageSource, Zone, default_path,
    unix_now,
};
use std::{path::PathBuf, process::ExitCode, time::Duration};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("record: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut path: Option<PathBuf> = None;
    let mut interval = Duration::from_secs(60);
    let mut duration: Option<Duration> = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| arguments.next().ok_or(format!("{name} needs a value"));
        match argument.as_str() {
            "--db" => path = Some(PathBuf::from(value("--db")?)),
            "--interval" => {
                interval =
                    Duration::from_secs(value("--interval")?.parse().map_err(|_| "bad --interval")?)
            }
            "--duration" => {
                duration = Some(Duration::from_secs(
                    value("--duration")?.parse().map_err(|_| "bad --duration")?,
                ))
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let path = match path {
        Some(path) => path,
        None => default_path().map_err(|error| error.to_string())?,
    };
    println!("database: {}", path.display());

    let mut config = Config::new(&path);
    config.interval = interval;
    let recorder = std::sync::Arc::new(Recorder::new(config));
    let stop = Stop::new();
    let thread = {
        let (recorder, stop) = (recorder.clone(), stop.clone());
        std::thread::spawn(move || recorder.run(&stop))
    };
    let started = std::time::Instant::now();
    loop {
        if stop.wait(Duration::from_secs(5))
            || duration.is_some_and(|limit| started.elapsed() >= limit)
        {
            break;
        }
    }
    stop.stop();
    thread.join().map_err(|_| "recorder thread panicked")?;

    let reader = Reader::open(&path).map_err(|error| error.to_string())?;
    let now = unix_now();
    let periods = Periods::at(Zone::Local, now, 1);
    println!(
        "last sample: {:?}, live: {:?}",
        reader.last_sample_time(),
        reader.is_live(now)
    );
    for (name, (from, to)) in [
        ("today", periods.today),
        ("this week", periods.week),
        ("this month", periods.month),
    ] {
        let totals = reader
            .totals(&Scope::All, from, to)
            .map_err(|error| error.to_string())?;
        println!("{name}: rx {} B, tx {} B", totals.rx, totals.tx);
    }
    for (interface, totals) in reader
        .interface_totals(periods.month.0, periods.month.1)
        .map_err(|error| error.to_string())?
    {
        println!(
            "  {} ({}): rx {} B, tx {} B",
            interface.name, interface.kind, totals.rx, totals.tx
        );
    }
    for connection in reader.connections().map_err(|error| error.to_string())? {
        println!(
            "  connection {} [{}] metered={}",
            connection.name, connection.uuid, connection.metered
        );
    }
    let minutes = reader
        .series(&Scope::All, Granularity::Minute, now - 600, now + 60)
        .map_err(|error| error.to_string())?;
    println!(
        "last minutes: {:?}",
        minutes
            .iter()
            .map(|bucket| (bucket.rx, bucket.tx))
            .collect::<Vec<_>>()
    );
    println!(
        "since boot: {:?}",
        reader
            .since_boot_total()
            .map_err(|error| error.to_string())?
    );
    Ok(())
}
