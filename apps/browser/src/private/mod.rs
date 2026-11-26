//! The private window: another copy of the browser, started with `--private`,
//! whose tabs keep nothing. It is a process of its own so that its cookies,
//! storage and engine are not shared with this window's in any way. It starts
//! with the settings of `profile` (this window's), which it reads and never writes.

#[cfg(test)]
mod tests;

use bingux_ui::task::spawn_blocking;
use freya::prelude::spawn;

/// Start a private window. Nothing waits for it.
pub fn open(profile: Option<String>) {
    spawn(async move {
        spawn_blocking(move || {
            let Ok(program) = std::env::current_exe() else {
                return;
            };
            let mut command = std::process::Command::new(program);
            command.arg("--private");
            if let Some(profile) = profile {
                command.arg("--profile").arg(profile);
            }
            if let Ok(mut child) = command.spawn() {
                // Reaped on a thread of its own, so it never becomes a zombie.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
        })
        .await;
    });
}
