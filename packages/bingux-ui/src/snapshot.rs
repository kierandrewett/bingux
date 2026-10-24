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
use freya_testing::prelude::{MouseEventName, PlatformEvent};

use crate::theme::font_family;

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
        let scale = scale_factor();
        let (mut runner, ()) = TestingRunner::new(
            app,
            Size2D::new(size.0 * scale as f32, size.1 * scale as f32),
            |_| {},
            scale,
        );
        let family = font_family();
        if let Some(font) = crate::theme::font_files(family, 1).pop() {
            runner.set_fonts([(family, font.as_slice())].into_iter().collect());
            runner.set_default_fonts(&[family.into()]);
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

    /// Press and release the right mouse button at a logical point.
    pub fn right_click(&mut self, x: f32, y: f32) -> &mut Self {
        let cursor = CursorPoint::new(x as f64, y as f64);
        for name in [MouseEventName::MouseDown, MouseEventName::MouseUp] {
            self.runner.send_event(PlatformEvent::Mouse {
                name,
                cursor,
                button: Some(MouseButton::Right),
            });
            self.runner.sync_and_update();
        }
        self
    }

    /// Press the left button at `from`, drag to `to` in steps, and release.
    /// The steps let a selection grow as it does under a real pointer.
    pub fn drag(&mut self, from: (f32, f32), to: (f32, f32)) -> &mut Self {
        const STEPS: u32 = 10;
        let start = CursorPoint::new(from.0 as f64, from.1 as f64);
        self.runner.move_cursor((from.0 as f64, from.1 as f64));
        self.runner.sync_and_update();
        self.runner.send_event(PlatformEvent::Mouse {
            name: MouseEventName::MouseDown,
            cursor: start,
            button: Some(MouseButton::Left),
        });
        self.runner.sync_and_update();
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            let x = from.0 + (to.0 - from.0) * t;
            let y = from.1 + (to.1 - from.1) * t;
            self.runner.move_cursor((x as f64, y as f64));
            self.runner.sync_and_update();
        }
        self.runner.send_event(PlatformEvent::Mouse {
            name: MouseEventName::MouseUp,
            cursor: CursorPoint::new(to.0 as f64, to.1 as f64),
            button: Some(MouseButton::Left),
        });
        self.runner.sync_and_update();
        self
    }

    /// Hover a logical point.
    pub fn hover(&mut self, x: f32, y: f32) -> &mut Self {
        self.runner.move_cursor((x as f64, y as f64));
        self.runner.sync_and_update();
        self
    }

    /// Scroll the wheel by `dy` logical pixels over a point; negative moves
    /// the content up.
    pub fn scroll(&mut self, x: f32, y: f32, dy: f32) -> &mut Self {
        self.runner.scroll((x as f64, y as f64), (0.0, dy as f64));
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

/// The display scale to render at: `BINGUX_SNAPSHOT_SCALE`, default 1.0. A
/// fractional value such as 1.25 or 1.5 shows pixel-rounding problems that a
/// scale of 1 hides.
fn scale_factor() -> f64 {
    std::env::var("BINGUX_SNAPSHOT_SCALE")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|scale: &f64| *scale >= 1.0)
        .unwrap_or(1.0)
}
