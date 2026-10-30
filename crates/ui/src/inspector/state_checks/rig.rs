//! The hands of the state checks: move the pointer, press, focus, type, and
//! read what the window shows.
//!
//! The animation clock is switched off while a [`Rig`] exists, so every
//! transition (a hover fill, a ring arriving, a menu opening) is finished on
//! the next frame and the sweep waits milliseconds, not the 200 ms of real
//! time a state takes to settle. What a person sees after the transition is
//! what is measured; the transitions themselves are the motion checks' work.

use std::time::{Duration, Instant};

use freya::prelude::*;
use freya_core::accessibility::id::AccessibilityId;
use freya_core::accessibility::tree::ACCESSIBILITY_ROOT_ID;
use freya_core::user_event::UserEvent;
use torin::prelude::Area;

use super::frame::Frame;
use super::view::{self, Target};
use crate::inspector::snapshot::Snapshot as Tree;
use crate::snapshot::Snapshot;

/// One frame step of the settle.
const STEP: Duration = Duration::from_millis(2);

/// Frames a state takes to show once the clock is off.
pub(super) const SETTLE: u32 = 4;

/// Where the pointer rests when it is on nothing: outside the window.
const AWAY: (f64, f64) = (-40.0, -40.0);

pub(super) struct Rig<'a> {
    pub shot: &'a mut Snapshot,
    /// Physical pixels per logical pixel.
    pub scale: f32,
    /// The window's logical box.
    pub window: Area,
    speed: f32,
    started: Instant,
    budget: Duration,
    /// The most controls of one kind (role and component) a run tries.
    pub per_kind: usize,
}

impl<'a> Rig<'a> {
    pub fn new(shot: &'a mut Snapshot, budget: Duration, per_kind: usize) -> Self {
        let scale = crate::snapshot::scale_factor() as f32;
        let speed = shot.runner().animation_clock().speed();
        shot.runner().animation_clock().disable();
        let mut rig = Self { shot, scale, window: Area::zero(), speed, started: Instant::now(), budget, per_kind };
        rig.window = view::window(&rig.tree());
        rig
    }

    /// Whether the run has used the time it is allowed. A window as tall as a
    /// taxonomy page takes a second a frame, so the long parts stop here and
    /// say so.
    pub fn over_budget(&self) -> bool {
        self.started.elapsed() > self.budget
    }

    pub fn tree(&mut self) -> Tree {
        Tree::capture_headless(self.shot.runner(), self.scale)
    }

    pub fn settle(&mut self, frames: u32) {
        let more: u32 = std::env::var("BINGUX_UI_STATE_SETTLE").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        self.shot.runner().poll_n(STEP, frames * more);
    }

    pub fn frame(&mut self) -> Frame {
        Frame::of(self.shot, self.window.width())
    }

    /// Whether the window is narrower than the compact breakpoint.
    pub fn compact(&self) -> bool {
        self.window.width() < super::COMPACT_BELOW
    }

    fn point(&self, at: (f32, f32)) -> (f64, f64) {
        ((at.0 * self.scale) as f64, (at.1 * self.scale) as f64)
    }

    /// Moves the pointer to a logical point and lets the frame settle.
    pub fn hover(&mut self, at: (f32, f32)) {
        let at = self.point(at);
        self.shot.runner().move_cursor(at);
        self.settle(SETTLE);
    }

    /// The cursor the window asks for, with the pointer at a logical point.
    pub fn cursor_at(&mut self, at: (f32, f32)) -> CursorIcon {
        let at = self.point(at);
        self.shot.runner().move_cursor(at);
        self.shot.runner().sync_and_update();
        self.shot.runner().cursor_icon()
    }

    /// Takes the pointer off everything and drops keyboard focus: the window
    /// at rest.
    pub fn park(&mut self) {
        self.shot.runner().move_cursor(AWAY);
        self.shot.runner().run_in(|| {
            let window = Platform::get().current_window();
            let mut mode = window.navigation_mode;
            mode.set(NavigationMode::NotKeyboard);
            window.send(UserEvent::FocusAccessibilityNode(AccessibilityFocusStrategy::Node(
                ACCESSIBILITY_ROOT_ID,
            )));
        });
        self.settle(SETTLE);
    }

    /// The window at rest, to judge the ring of the control `id` against: the
    /// pointer off everything and `id` not focused. A popup puts focus on its
    /// first field and parking does not take it off, so then focus is moved to
    /// another control (its own ring is somewhere else) before the frame is read.
    pub fn rest_for(&mut self, id: AccessibilityId) -> Frame {
        self.park();
        if self.focused() == id {
            let other = view::focusables(&self.tree())
                .into_iter()
                .find(|other| other.focus != id && other.visible.width() >= 1.0);
            if let Some(other) = other {
                self.focus(other.focus);
            }
        }
        self.frame()
    }

    /// Holds the left button down at a logical point, then lets it settle.
    pub fn press(&mut self, at: (f32, f32)) {
        self.shot.press(at_physical(at, self.scale));
        self.settle(SETTLE);
    }

    /// Moves the held button off everything and lets go there: no click.
    pub fn release_away(&mut self) {
        self.shot.move_to((AWAY.0 as f32, AWAY.1 as f32));
        self.shot.release((AWAY.0 as f32, AWAY.1 as f32));
        self.settle(SETTLE);
    }

    /// Puts keyboard focus on `target`, as Tab would.
    pub fn focus(&mut self, id: AccessibilityId) {
        self.shot.runner().run_in(|| {
            let mut mode = Platform::get().current_window().navigation_mode;
            mode.set(NavigationMode::Keyboard);
            id.request_focus();
        });
        // The ring mounts on the first frame after focus and its animation
        // starts on the next, so focus waits longer than a hover.
        self.settle(SETTLE * 3);
    }

    /// The accessibility id that has keyboard focus.
    pub fn focused(&mut self) -> AccessibilityId {
        self.shot
            .runner()
            .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek())
    }

    /// Presses a key on whatever has focus, then lets the frame settle.
    pub fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    pub fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.shot.key_with(key, modifiers);
        self.settle(SETTLE);
    }

    pub fn named(&mut self, key: NamedKey) {
        self.key(Key::Named(key));
    }

    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.key(Key::Character(character.to_string()));
        }
    }

    /// Whether `target` is still `target`: a control with its id in the tree.
    pub fn target_ids(&mut self) -> Vec<u64> {
        view::signature(&self.tree())
    }

    /// Escapes whatever is open until the window is as it was (the same
    /// controls), at most `tries` times. Returns whether it is.
    pub fn restore(&mut self, ids: &[u64], tries: usize) -> bool {
        for _ in 0..tries {
            if self.target_ids() == ids {
                return true;
            }
            self.named(NamedKey::Escape);
            self.park();
        }
        self.target_ids() == ids
    }

    pub fn targets(&mut self) -> Vec<Target> {
        view::targets(&self.tree())
    }
}

fn at_physical(at: (f32, f32), scale: f32) -> (f32, f32) {
    (at.0 * scale, at.1 * scale)
}

impl Drop for Rig<'_> {
    fn drop(&mut self) {
        self.shot.runner().move_cursor(AWAY);
        let speed = self.speed;
        self.shot.runner().animation_clock().set_speed(speed);
    }
}
