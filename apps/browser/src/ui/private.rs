//! The private window: another copy of the browser, started with `--private`,
//! whose tabs keep nothing. It is a process of its own so that its cookies,
//! storage and engine are not shared with this window's in any way.

use bingux_ui::task::spawn_blocking;
use freya::prelude::spawn;

/// Start a private window. Nothing waits for it.
pub fn open() {
    spawn(async move {
        spawn_blocking(|| {
            let Ok(program) = std::env::current_exe() else {
                return;
            };
            if let Ok(mut child) = std::process::Command::new(program).arg("--private").spawn() {
                // Reaped on a thread of its own, so it never becomes a zombie.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
        })
        .await;
    });
}
