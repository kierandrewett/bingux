//! Headless rendering for documentation screenshots and visual checks.
//!
//! Enabled with the `testing` feature. [`snapshot`] runs an app in
//! `freya-testing`'s software renderer, lets animations and background
//! work settle, then writes a PNG. No compositor or window is needed;
//! `Platform` calls that target a native window are dropped.

use std::path::Path;
use std::time::Duration;

use freya::prelude::*;
use freya::torin::prelude::Size2D;
use freya_testing::TestingRunner;

use crate::theme::FONT_FAMILY;

/// Width and height in logical pixels.
pub type SnapshotSize = (f32, f32);

/// The running headless app, for scripted interaction before a capture.
pub struct Snapshot {
    runner: TestingRunner,
}

impl Snapshot {
    /// Start `app` at `size` with the Bingux font registered when the
    /// system has it (`fc-match "Adwaita Sans"`).
    pub fn new(app: impl Fn() -> Element + 'static, size: SnapshotSize) -> Self {
        let (mut runner, ()) = TestingRunner::new(app, Size2D::new(size.0, size.1), |_| {}, 1.0);
        if let Some(font) = crate::theme::system_font(FONT_FAMILY) {
            runner.set_fonts([(FONT_FAMILY, font.as_slice())].into_iter().collect());
            runner.set_default_fonts(&[FONT_FAMILY.into()]);
        }
        Self { runner }
    }

    /// Drive events, tasks and animations for `duration`.
    pub fn settle(&mut self, duration: Duration) -> &mut Self {
        self.runner.poll(Duration::from_millis(16), duration);
        self
    }

    /// Click at a logical point.
    pub fn click(&mut self, x: f32, y: f32) -> &mut Self {
        self.runner.click_cursor((x as f64, y as f64));
        self.runner.sync_and_update();
        self
    }

    /// Hover a logical point.
    pub fn hover(&mut self, x: f32, y: f32) -> &mut Self {
        self.runner.move_cursor((x as f64, y as f64));
        self.runner.sync_and_update();
        self
    }

    /// Press and release a key on the focused element.
    pub fn key(&mut self, key: Key) -> &mut Self {
        self.runner.press_key(key);
        self.runner.sync_and_update();
        self
    }

    /// Write a PNG of the current frame.
    pub fn save(&mut self, path: impl AsRef<Path>) -> &mut Self {
        if let Some(parent) = path.as_ref().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        self.runner.render_to_file(path.as_ref().to_path_buf());
        self
    }

    pub fn runner(&mut self) -> &mut TestingRunner {
        &mut self.runner
    }
}

/// Render `app` once after `settle_ms`, writing the PNG to `path`.
pub fn snapshot(
    app: impl Fn() -> Element + 'static,
    size: SnapshotSize,
    settle_ms: u64,
    path: impl AsRef<Path>,
) {
    Snapshot::new(app, size)
        .settle(Duration::from_millis(settle_ms))
        .save(path);
}
