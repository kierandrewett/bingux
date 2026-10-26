//! Hosts the network data usage recorder (`bingux-data-usage`) on a thread.
//!
//! The recorder is independent of the metrics feed: it has its own thread,
//! its own database connection and its own error handling, so a full disk
//! or a corrupt database can only ever cost usage history, never the feed.

use bingux_data_usage::{Config, Recorder, Stop};
use std::{sync::Arc, thread};

/// A running recorder. Dropping it stops the thread and joins it.
pub struct UsageRecorder {
    stop: Stop,
    thread: Option<thread::JoinHandle<()>>,
}

/// Starts the recorder at the default database path. Failure to start is
/// logged and leaves the daemon running without usage recording.
pub fn start() -> Option<UsageRecorder> {
    let config = match Config::with_default_path() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("[bingux-statusd] data usage recording disabled: {error}");
            return None;
        }
    };
    // `Recorder::new` does not touch the disk; the thread opens the database
    // and retries every round, logging a failure at most once a minute.
    let recorder = Arc::new(Recorder::new(config));
    let stop = Stop::new();
    let spawned = thread::Builder::new().name("data-usage".into()).spawn({
        let (recorder, stop) = (recorder.clone(), stop.clone());
        move || recorder.run(&stop)
    });
    match spawned {
        Ok(thread) => {
            eprintln!("[bingux-statusd] data usage recorder started");
            Some(UsageRecorder {
                stop,
                thread: Some(thread),
            })
        }
        Err(error) => {
            eprintln!("[bingux-statusd] data usage recording disabled: {error}");
            None
        }
    }
}

impl Drop for UsageRecorder {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(thread) = self.thread.take() {
            // A panic inside the recorder must not take the daemon down with
            // it during shutdown; the join result carries nothing else.
            let _ = thread.join();
        }
    }
}
