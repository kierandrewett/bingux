//! Headless rendering for documentation screenshots and visual checks.
//!
//! Enabled with the `testing` feature. [`snapshot`] runs an app in
//! `freya-testing`'s software renderer, lets animations and background
//! work settle, then writes a PNG. No compositor or window is needed;
//! `Platform` calls that target a native window are dropped.
//!
//! [`Snapshot::record_frames`] writes every frame of the 16 ms animation
//! clock to a directory, so a run can be checked frame by frame for layout
//! shifts. [`Snapshot::run_script`] drives a whole walk (clicks, scrolls,
//! keys, waits and navigation) from one string.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use freya::prelude::*;
use freya::torin::prelude::Size2D;
use freya_components::integration::integration;
use freya_testing::TestingRunner;
use freya_testing::prelude::{
    KeyboardEventName, MouseEventName, PlatformEvent, TouchEventName, TouchPhase,
};

use crate::nav::Location;
use crate::theme::font_family;

/// Width and height in logical pixels.
pub type SnapshotSize = (f32, f32);

/// One icon button of a frame, from [`Snapshot::icon_buttons`].
#[derive(Clone, Debug)]
pub struct IconButtonLook {
    /// `IconButton`, or `IconMenuButton` for a `menu_button` with an icon.
    pub component: String,
    /// Its accessible name: the tooltip or the menu button's label.
    pub name: String,
    pub area: freya::torin::prelude::Area,
    /// The fill it paints now, `None` when it paints nothing.
    pub fill: Option<Color>,
}

/// One step of the animation clock. Each step is one recorded frame.
const FRAME: Duration = Duration::from_millis(16);

/// The running headless app, for scripted interaction before a capture.
pub struct Snapshot {
    runner: TestingRunner,
    recorder: Option<FrameRecorder>,
    /// When the last pointer or key input was sent. Frames record how long
    /// before them that was, so a shift can be judged as idle or as input.
    last_input: Option<Instant>,
}

impl Snapshot {
    /// Start `app` at `size` with the Bingux font registered when the
    /// system has it (`fc-match "Adwaita Sans"`).
    pub fn new(app: impl Fn() -> Element + 'static, size: SnapshotSize) -> Self {
        Self::with_scale(app, size, scale_factor())
    }

    /// [`Snapshot::new`] at the display scale `scale` instead of the
    /// environment's, for a test of how a control holds at a fractional one.
    /// The pointer methods take the runner's own units, which are physical
    /// pixels at this scale; the layout and the PNG are in physical pixels
    /// too.
    pub fn with_scale(app: impl Fn() -> Element + 'static, size: SnapshotSize, scale: f64) -> Self {
        // A headless run presses real controls: the Settings pages write through the
        // `gsettings` command, and a test, a sweep or a capture must never change the
        // person's dconf.
        crate::runtime::isolate_system_settings();
        // Mount the same root integration the windowed app gets, so Tab and
        // arrow keys move accessibility focus exactly as they do on screen.
        let app: std::rc::Rc<dyn Fn() -> Element> = std::rc::Rc::new(app);
        // The harness is a window like any other: panic hook installed, and the
        // app inside the same last-resort error boundary `Application::run`
        // gives it, so a failing specimen shows an error card and the test goes on.
        crate::resilience::install();
        let (mut runner, ()) = TestingRunner::new(
            move || {
                let app = app.clone();
                integration(
                    (move || crate::resilience::window_boundary("The app", app.clone())).into(),
                )
                .into_element()
            },
            Size2D::new(size.0 * scale as f32, size.1 * scale as f32),
            |_| {},
            scale,
        );
        let family = font_family();
        if let Some(font) = crate::theme::font_files(family, 1).pop() {
            runner.set_fonts([(family, font.as_slice())].into_iter().collect());
            runner.set_default_fonts(&[family.into()]);
        }
        Self {
            runner,
            recorder: None,
            last_input: None,
        }
    }

    /// Write every frame from now on to `dir`, one PNG per frame, and a
    /// `frames.jsonl` manifest with one line per frame and per marker.
    ///
    /// A frame whose PNG is byte-identical to the previous one reuses that
    /// file, so a settled page costs one file, not one per tick. Each frame
    /// line gives its index, time, `since_input_ms` (time since the last
    /// pointer or key input, or `null`) and `file`.
    pub fn record_frames(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).expect("create the frame directory");
        let manifest = File::create(dir.join("frames.jsonl")).expect("create frames.jsonl");
        self.recorder = Some(FrameRecorder {
            dir,
            manifest,
            started: Instant::now(),
            frames: 0,
            previous: None,
        });
        self
    }

    /// Note `text` in the frame manifest, such as the page being walked. Does
    /// nothing when frames are not being recorded.
    pub fn mark(&mut self, text: &str) -> &mut Self {
        let now = Instant::now();
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.marker(now, text);
        }
        self
    }

    /// Drive events, tasks and animations for `duration`. With frames on,
    /// every 16 ms step is recorded; without, this is one plain poll.
    pub fn settle(&mut self, duration: Duration) -> &mut Self {
        if self.recorder.is_none() {
            self.runner.poll(FRAME, duration);
            return self;
        }
        let end = Instant::now() + duration;
        while Instant::now() < end {
            self.runner.poll_n(FRAME, 1);
            self.capture_frame();
        }
        self
    }

    /// Drive the runner one 16 ms step at a time until `done` is true, or until `cap` has passed.
    /// Returns whether `done` was reached.
    ///
    /// [`Snapshot::settle`] waits out its whole duration whatever is on screen: a duration long
    /// enough for a loaded machine is a long wait on an idle one, and one that suits the idle
    /// machine fails on a loaded one. A test that waits for something to appear says what, with a
    /// generous cap, and goes on the moment it is there. The caller asserts on the result.
    pub fn settle_until(
        &mut self,
        cap: Duration,
        mut done: impl FnMut(&mut Snapshot) -> bool,
    ) -> bool {
        let end = Instant::now() + cap;
        loop {
            if done(self) {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            self.settle(FRAME);
        }
    }

    /// Click at a logical point.
    pub fn click(&mut self, x: f32, y: f32) -> &mut Self {
        self.runner.run_in(|| {
            Platform::get()
                .current_window()
                .navigation_mode
                .set(NavigationMode::NotKeyboard);
        });
        self.input(format!("click {x},{y}"));
        self.runner.click_cursor((x as f64, y as f64));
        self.runner.sync_and_update();
        self.capture_frame();
        self
    }

    /// Click the centre of the label whose text is `text`, preferring an
    /// exact match. Returns false, and notes it in the manifest, when no
    /// label has text containing `text`.
    pub fn click_text(&mut self, text: &str) -> bool {
        let Some((x, y)) = self.label_centre(text) else {
            self.mark(&format!("click-text not found: {text}"));
            return false;
        };
        self.click(x, y);
        true
    }

    /// Click the centre of the first box a component named `name` made, for a
    /// control that has no text of its own to click (a colour swatch, a
    /// slider). Returns false, and notes it in the manifest, when none is
    /// on screen.
    pub fn click_component(&mut self, name: &str) -> bool {
        let tree = crate::inspector::snapshot::Snapshot::capture_headless(&self.runner, 1.0);
        let found = tree
            .nodes
            .iter()
            .filter(|node| node.a11y.component.as_deref() == Some(name))
            .find_map(|node| node.area);
        let Some(area) = found else {
            self.mark(&format!("click-component not found: {name}"));
            return false;
        };
        let centre = area.center();
        self.click(centre.x, centre.y);
        true
    }

    /// Click the centre of the first box whose accessible name is `name`: a
    /// control that is named and has no text of its own (the chevron of a split
    /// button is "View options" to a screen reader and draws only an icon).
    /// Returns false, and notes it in the manifest, when none is on screen.
    pub fn click_name(&mut self, name: &str) -> bool {
        let tree = crate::inspector::snapshot::Snapshot::capture_headless(&self.runner, 1.0);
        let found = tree
            .nodes
            .iter()
            .filter(|node| node.a11y.name.as_deref() == Some(name))
            .find_map(|node| node.area);
        let Some(area) = found else {
            self.mark(&format!("click-name not found: {name}"));
            return false;
        };
        let centre = area.center();
        self.click(centre.x, centre.y);
        true
    }

    /// [`Snapshot::click_text`] with the right mouse button.
    pub fn right_click_text(&mut self, text: &str) -> bool {
        let Some((x, y)) = self.label_centre(text) else {
            self.mark(&format!("rclick-text not found: {text}"));
            return false;
        };
        self.right_click(x, y);
        true
    }

    /// The centre of the label whose text is `text`, preferring an exact
    /// match to one that only contains it; with no such label, the first
    /// paragraph whose text contains `text` (a tree row is one).
    fn label_centre(&mut self, text: &str) -> Option<(f32, f32)> {
        let hits: Vec<(bool, f32, f32)> = self.runner.find_many(|node, element| {
            let label = Label::try_downcast(element)?;
            if !label.text.contains(text) {
                return None;
            }
            let centre = node.layout().area.center();
            Some((label.text == text, centre.x, centre.y))
        });
        if let Some(&(_, x, y)) = hits.iter().find(|hit| hit.0).or_else(|| hits.first()) {
            return Some((x, y));
        }
        let paragraphs: Vec<(f32, f32)> = self.runner.find_many(|node, element| {
            let paragraph = Paragraph::try_downcast(element)?;
            let plain: String = paragraph
                .spans
                .iter()
                .map(|span| span.text.to_string())
                .collect();
            if !plain.contains(text) {
                return None;
            }
            let centre = node.layout().area.center();
            Some((centre.x, centre.y))
        });
        paragraphs.first().copied()
    }

    /// Press and release the right mouse button at a logical point.
    pub fn right_click(&mut self, x: f32, y: f32) -> &mut Self {
        self.input(format!("right_click {x},{y}"));
        let cursor = CursorPoint::new(x as f64, y as f64);
        for name in [MouseEventName::MouseDown, MouseEventName::MouseUp] {
            self.runner.send_event(PlatformEvent::Mouse {
                name,
                cursor,
                button: Some(MouseButton::Right),
            });
            self.runner.sync_and_update();
        }
        self.capture_frame();
        self
    }

    /// Tap a logical point with one finger: touch down, then up.
    pub fn touch(&mut self, x: f32, y: f32) -> &mut Self {
        self.input(format!("touch {x},{y}"));
        self.send_touch(TouchEventName::TouchStart, x, y);
        self.send_touch(TouchEventName::TouchEnd, x, y);
        self.capture_frame();
        self
    }

    /// Drag one finger from `from` to `to` in steps and lift it, as a swipe
    /// that scrolls a page does. A control that acts on a tap must not act on it.
    pub fn swipe(&mut self, from: (f32, f32), to: (f32, f32)) -> &mut Self {
        const STEPS: u32 = 8;
        self.input(format!("swipe {},{},{},{}", from.0, from.1, to.0, to.1));
        self.send_touch(TouchEventName::TouchStart, from.0, from.1);
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            let at = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            self.send_touch(TouchEventName::TouchMove, at.0, at.1);
        }
        self.send_touch(TouchEventName::TouchEnd, to.0, to.1);
        self.capture_frame();
        self
    }

    /// Rest one finger on a logical point past the long-press threshold,
    /// then lift it. Runs the long-press action in place of a right click.
    pub fn long_press(&mut self, x: f32, y: f32) -> &mut Self {
        self.input(format!("long_press {x},{y}"));
        self.send_touch(TouchEventName::TouchStart, x, y);
        self.settle(Duration::from_millis(600));
        self.send_touch(TouchEventName::TouchEnd, x, y);
        self.capture_frame();
        self
    }

    fn send_touch(&mut self, name: TouchEventName, x: f32, y: f32) {
        let phase = match name {
            TouchEventName::TouchStart => TouchPhase::Started,
            TouchEventName::TouchEnd => TouchPhase::Ended,
            TouchEventName::TouchMove => TouchPhase::Moved,
            TouchEventName::TouchCancel => TouchPhase::Cancelled,
        };
        self.runner.send_event(PlatformEvent::Touch {
            name,
            location: CursorPoint::new(x as f64, y as f64),
            finger_id: 0,
            phase,
            force: None,
        });
        self.runner.sync_and_update();
    }

    /// Press the left button at `from`, drag to `to` in steps, and release.
    /// The steps let a selection grow as it does under a real pointer.
    pub fn drag(&mut self, from: (f32, f32), to: (f32, f32)) -> &mut Self {
        const STEPS: u32 = 10;
        self.input(format!("drag {},{},{},{}", from.0, from.1, to.0, to.1));
        self.press(from);
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            self.move_to((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
        }
        self.release(to);
        self
    }

    /// Press the left button at a logical point and keep it held.
    pub fn press(&mut self, at: (f32, f32)) -> &mut Self {
        self.runner.move_cursor((at.0 as f64, at.1 as f64));
        self.runner.sync_and_update();
        self.runner.send_event(PlatformEvent::Mouse {
            name: MouseEventName::MouseDown,
            cursor: CursorPoint::new(at.0 as f64, at.1 as f64),
            button: Some(MouseButton::Left),
        });
        self.runner.sync_and_update();
        self.capture_frame();
        self
    }

    /// Move a held pointer to a logical point.
    pub fn move_to(&mut self, at: (f32, f32)) -> &mut Self {
        self.runner.move_cursor((at.0 as f64, at.1 as f64));
        self.runner.sync_and_update();
        self.capture_frame();
        self
    }

    /// Release the left button at a logical point.
    pub fn release(&mut self, at: (f32, f32)) -> &mut Self {
        self.runner.send_event(PlatformEvent::Mouse {
            name: MouseEventName::MouseUp,
            cursor: CursorPoint::new(at.0 as f64, at.1 as f64),
            button: Some(MouseButton::Left),
        });
        self.runner.sync_and_update();
        self.capture_frame();
        self
    }

    /// Hover a logical point.
    pub fn hover(&mut self, x: f32, y: f32) -> &mut Self {
        self.input(format!("hover {x},{y}"));
        self.runner.move_cursor((x as f64, y as f64));
        self.runner.sync_and_update();
        self.capture_frame();
        self
    }

    /// Hover the centre of the label, or else the paragraph, whose text is
    /// `text`, as [`Snapshot::click_text`] finds it. Returns false when none
    /// contains `text`.
    pub fn hover_text(&mut self, text: &str) -> bool {
        let Some((x, y)) = self.label_centre(text) else {
            self.mark(&format!("hover-text not found: {text}"));
            return false;
        };
        self.hover(x, y);
        true
    }

    /// Scroll the wheel by `dy` logical pixels over a point; negative moves
    /// the content up.
    pub fn scroll(&mut self, x: f32, y: f32, dy: f32) -> &mut Self {
        self.input(format!("scroll {x},{y},{dy}"));
        self.runner.scroll((x as f64, y as f64), (0.0, dy as f64));
        self.runner.sync_and_update();
        self.capture_frame();
        self
    }

    /// Press and release a key on the focused element. Keys mark the window as
    /// keyboard-navigated, as the windowed app does, so focus rings show.
    pub fn key(&mut self, key: Key) -> &mut Self {
        self.key_with(key, Modifiers::default())
    }

    /// [`Snapshot::key`] with modifiers held, such as Shift for Shift+Tab.
    pub fn key_with(&mut self, key: Key, modifiers: Modifiers) -> &mut Self {
        self.input(format!("key {modifiers:?} {key:?}"));
        self.runner.run_in(|| {
            Platform::get()
                .current_window()
                .navigation_mode
                .set(NavigationMode::Keyboard);
        });
        self.runner.send_event(PlatformEvent::Keyboard {
            name: KeyboardEventName::KeyDown,
            key,
            code: Code::Unidentified,
            modifiers,
        });
        // Focus moves on the frame after the key lands, and the ring's
        // transition starts once focus is known, so run a few frames.
        for _ in 0..3 {
            self.runner.sync_and_update();
        }
        self.capture_frame();
        self
    }

    /// Open `location` in the window, as a deep link does: the stack is the
    /// location's ancestors, so Back works from it.
    pub fn navigate(&mut self, location: Location) -> &mut Self {
        self.mark(&format!("address {}", location.path_text()));
        self.runner
            .run_in(move || crate::nav::router().navigate_to(&location));
        self.runner.sync_and_update();
        self.capture_frame();
        self
    }

    /// Run a script of commands separated by `;` or new lines. `scheme` is the
    /// scheme that `address` commands are read in. Commands:
    ///
    /// - `click x,y`, `right_click x,y`, `hover x,y`
    /// - `touch x,y`, `long_press x,y`: one finger, tapped or rested past the threshold
    /// - `touch_down x,y`, `touch_move x,y`, `touch_up x,y`: one finger held across steps
    /// - `scroll x,y,dy`, `drag x1,y1,x2,y2`, `down x,y`/`move x,y`/`up x,y`
    /// - `key escape|return|tab|space|left|right|up|down|CHARACTER`, with `shift+`, `ctrl+` or `alt+` in front
    /// - `type TEXT`: each character of TEXT as a key, to the focused field
    /// - `click-text TEXT`: click the label with that text
    /// - `click-name NAME`: click the control with that accessible name (one with no text of its own)
    /// - `address LOCATION`: open a location (a URI or a bare path of `scheme`),
    ///   e.g. `address network/wifi/saved`
    /// - `wait MS`: let the app run for that long, recording frames
    /// - `mark TEXT`: a note in the frame manifest
    /// - `save PATH`: write a PNG of the current frame
    ///
    /// A `click-text` that finds nothing is noted on stderr and the walk goes
    /// on; a malformed command stops the script with an error.
    pub fn run_script(&mut self, script: &str, scheme: &str) -> Result<(), String> {
        for command in script
            .split([';', '\n'])
            .map(str::trim)
            .filter(|command| !command.is_empty())
        {
            let (verb, rest) = command
                .split_once(char::is_whitespace)
                .map_or((command, ""), |(verb, rest)| (verb, rest.trim()));
            match verb {
                "touch" => {
                    let [x, y] = numbers::<2>(rest)?;
                    self.touch(x, y);
                }
                "long_press" => {
                    let [x, y] = numbers::<2>(rest)?;
                    self.long_press(x, y);
                }
                "touch_down" | "touch_move" | "touch_up" => {
                    let [x, y] = numbers::<2>(rest)?;
                    let name = match verb {
                        "touch_down" => TouchEventName::TouchStart,
                        "touch_move" => TouchEventName::TouchMove,
                        _ => TouchEventName::TouchEnd,
                    };
                    self.input(format!("{verb} {x},{y}"));
                    self.send_touch(name, x, y);
                    self.capture_frame();
                }
                "click" => {
                    let [x, y] = numbers::<2>(rest)?;
                    self.click(x, y);
                }
                "right_click" => {
                    let [x, y] = numbers::<2>(rest)?;
                    self.right_click(x, y);
                }
                "hover" => {
                    let [x, y] = numbers::<2>(rest)?;
                    self.hover(x, y);
                }
                "scroll" => {
                    let [x, y, dy] = numbers::<3>(rest)?;
                    self.scroll(x, y, dy);
                }
                "drag" => {
                    let [x1, y1, x2, y2] = numbers::<4>(rest)?;
                    self.drag((x1, y1), (x2, y2));
                }
                "down" => {
                    let [x, y] = numbers::<2>(rest)?;
                    self.press((x, y));
                }
                "move" => {
                    let [x, y] = numbers::<2>(rest)?;
                    self.move_to((x, y));
                }
                "up" => {
                    let [x, y] = numbers::<2>(rest)?;
                    self.release((x, y));
                }
                "type" => {
                    // Each character of the rest of the command, as a key press.
                    for character in rest.chars() {
                        self.key(Key::Character(character.to_string()));
                    }
                }
                "key" => {
                    // Any of `shift+`, `ctrl+` and `alt+` in front.
                    let mut modifiers = Modifiers::default();
                    let mut key = rest;
                    loop {
                        if let Some(more) = key.strip_prefix("shift+") {
                            modifiers |= Modifiers::SHIFT;
                            key = more;
                        } else if let Some(more) = key.strip_prefix("ctrl+") {
                            modifiers |= Modifiers::CONTROL;
                            key = more;
                        } else if let Some(more) = key.strip_prefix("alt+") {
                            modifiers |= Modifiers::ALT;
                            key = more;
                        } else {
                            break;
                        }
                    }
                    let key = match key {
                        "escape" => Key::Named(NamedKey::Escape),
                        "return" => Key::Named(NamedKey::Enter),
                        "tab" => Key::Named(NamedKey::Tab),
                        "f3" => Key::Named(NamedKey::F3),
                        "f2" => Key::Named(NamedKey::F2),
                        "f10" => Key::Named(NamedKey::F10),
                        "f6" => Key::Named(NamedKey::F6),
                        "f11" => Key::Named(NamedKey::F11),
                        "delete" => Key::Named(NamedKey::Delete),
                        "space" => Key::Character(" ".to_owned()),
                        "left" => Key::Named(NamedKey::ArrowLeft),
                        "right" => Key::Named(NamedKey::ArrowRight),
                        "up" => Key::Named(NamedKey::ArrowUp),
                        "down" => Key::Named(NamedKey::ArrowDown),
                        // A single character, such as the `i` of Ctrl+I.
                        other if other.chars().count() == 1 => Key::Character(other.to_owned()),
                        other => return Err(format!("unknown key: {other}")),
                    };
                    self.key_with(key, modifiers);
                }
                "type" => {
                    for character in rest.chars() {
                        self.key(Key::Character(character.to_string()));
                    }
                }
                "hover-text" => {
                    if !self.hover_text(rest) {
                        eprintln!("[snapshot] hover-text found no label: {rest}");
                    }
                }
                "click-text" => {
                    if !self.click_text(rest) {
                        eprintln!("[snapshot] click-text found no label: {rest}");
                    }
                }
                "click-name" => {
                    if !self.click_name(rest) {
                        eprintln!("[snapshot] click-name found no control: {rest}");
                    }
                }
                "click-component" => {
                    if !self.click_component(rest) {
                        eprintln!("[snapshot] click-component found no component: {rest}");
                    }
                }
                "rclick-text" => {
                    if !self.right_click_text(rest) {
                        eprintln!("[snapshot] rclick-text found no label: {rest}");
                    }
                }
                "address" => {
                    let location = Location::parse_in(scheme, rest)
                        .ok_or_else(|| format!("not a location: {rest}"))?;
                    self.navigate(location);
                }
                "wait" => {
                    let ms: u64 = rest
                        .parse()
                        .map_err(|_| format!("wait needs milliseconds: {rest}"))?;
                    self.settle(Duration::from_millis(ms));
                }
                "slow" => {
                    let speed: f32 = rest
                        .parse()
                        .map_err(|_| format!("slow needs a speed factor: {rest}"))?;
                    self.runner.animation_clock().set_speed(speed);
                }
                "mark" => {
                    self.mark(rest);
                }
                "save" => {
                    self.save(rest);
                }
                other => return Err(format!("unknown command: {other}")),
            }
        }
        Ok(())
    }

    /// Every focus ring of the current frame with the face it belongs to,
    /// for [`crate::focus::ring_check`].
    pub fn focus_rings(&mut self) -> Vec<crate::focus::ring_check::RingProbe> {
        let snapshot = crate::inspector::snapshot::Snapshot::capture_headless(
            &self.runner,
            scale_factor() as f32,
        );
        crate::focus::ring_check::probes(&snapshot)
            .into_iter()
            .map(|(_, probe)| probe)
            .collect()
    }

    /// The rings of the current frame that are not around their face's
    /// painted box. Empty when every ring is where it should be.
    pub fn ring_mismatches(&mut self) -> Vec<crate::focus::ring_check::Mismatch> {
        crate::focus::ring_check::check(&self.focus_rings())
    }

    /// With `BINGUX_UI_RING_CHECK=FILE` set, appends one JSON line to FILE for
    /// each capture: the PNG's path, how many rings the frame has and the ones
    /// that are not around their face. `scripts/focus-ring-check.py` reads it.
    fn lint_rings(&mut self, shot: &Path) {
        let Some(file) = std::env::var_os("BINGUX_UI_RING_CHECK") else {
            return;
        };
        let probes = self.focus_rings();
        let mismatches: Vec<_> = crate::focus::ring_check::check(&probes)
            .iter()
            .map(|found| {
                serde_json::json!({
                    "label": found.label,
                    "what": found.what,
                    "expected": found.expected,
                    "found": found.found,
                })
            })
            .collect();
        let line = serde_json::json!({
            "capture": shot.to_string_lossy(),
            "rings": probes.len(),
            "mismatches": mismatches,
        });
        use std::io::Write;
        if let Ok(mut out) = fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(out, "{line}");
        }
    }

    /// With `BINGUX_UI_TOOLTIP_CHECK=FILE` set, appends one JSON line to FILE
    /// for each capture: the PNG's path, how many named icon buttons the frame
    /// has, how many showed a bubble, and what is wrong with the bubbles
    /// ([`Snapshot::tooltip_findings`]). It runs after the PNG is written, so
    /// the pointer it moves is not in the capture. `scripts/tooltip-check.py`
    /// reads it.
    fn lint_tooltips(&mut self, shot: &Path) {
        let Some(file) = std::env::var_os("BINGUX_UI_TOOLTIP_CHECK") else {
            return;
        };
        let report = self.tooltip_findings();
        let findings: Vec<_> = report
            .findings
            .iter()
            .map(|found| {
                serde_json::json!({
                    "name": found.name,
                    "rule": found.rule,
                    "detail": found.detail,
                    "control": [found.control.min_x(), found.control.min_y(), found.control.width(), found.control.height()],
                    "bubble": [found.bubble.min_x(), found.bubble.min_y(), found.bubble.width(), found.bubble.height()],
                })
            })
            .collect();
        let line = serde_json::json!({
            "capture": shot.to_string_lossy(),
            "controls": report.controls,
            "shown": report.shown,
            "missing": report.missing,
            "findings": findings,
        });
        use std::io::Write;
        if let Ok(mut out) = fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(out, "{line}");
        }
    }

    /// Write a PNG of the current frame.
    pub fn save(&mut self, path: impl AsRef<Path>) -> &mut Self {
        if let Some(parent) = path.as_ref().parent() {
            let _ = fs::create_dir_all(parent);
        }
        self.runner.render_to_file(path.as_ref().to_path_buf());
        self.lint_layout(path.as_ref());
        self.lint_rings(path.as_ref());
        self.lint_tooltips(path.as_ref());
        self.lint_states(path.as_ref());
        self.lint_probe(path.as_ref());
        self
    }

    /// The layout findings of the current frame: what the inspector's Layout
    /// tab would list for this window.
    pub fn layout_findings(&mut self) -> Vec<crate::inspector::layout_checks::Finding> {
        let snapshot = crate::inspector::snapshot::Snapshot::capture_headless(
            &self.runner,
            scale_factor() as f32,
        );
        crate::inspector::layout_checks::check(&snapshot)
    }

    /// Every icon button of the current frame (`icon_button`, and the icon
    /// variant of `menu_button`), with the fill it paints now. A button at
    /// rest (not hovered, pressed, open or `filled()`/`active()` by design)
    /// paints none, in a header and in a toolbar alike.
    pub fn icon_buttons(&mut self) -> Vec<IconButtonLook> {
        let tree = crate::inspector::snapshot::Snapshot::capture_headless(
            &self.runner,
            scale_factor() as f32,
        );
        tree.nodes
            .iter()
            .filter_map(|node| {
                let component = node.a11y.component.as_deref()?;
                if !matches!(component, "IconButton" | "IconMenuButton") {
                    return None;
                }
                let fill = node
                    .props
                    .as_ref()
                    .and_then(|props| props.background)
                    .filter(|colour| colour.a() != 0);
                Some(IconButtonLook {
                    component: component.to_owned(),
                    name: node.a11y.name.clone().unwrap_or_default(),
                    area: node.area?,
                    fill,
                })
            })
            .collect()
    }

    /// With `BINGUX_UI_LAYOUT_LINT=FILE` set, appends one JSON line to FILE
    /// for each capture: the PNG's path and the layout findings of the frame
    /// it shows. `scripts/layout-lint.py` reads it. With
    /// `BINGUX_UI_LAYOUT_TREE` set as well, the frame's tree is written next
    /// to the PNG as `NAME.tree.txt`, to read a finding against.
    ///
    /// With `BINGUX_UI_LAYOUT_MIRROR=DIR` set too, the run is one half of a
    /// mirror check (`scripts/layout-lint.py --rtl`): a run that reads left to
    /// right saves the shape of the frame in DIR as `NAME.json`, and a run
    /// that reads right to left (`BINGUX_DIRECTION=rtl`) compares its frame
    /// with that file and adds the `direction-asymmetric` findings.
    fn lint_layout(&mut self, shot: &Path) {
        use crate::controls::direction::Direction;
        use crate::inspector::layout_checks::{self, Structure};
        let Some(file) = std::env::var_os("BINGUX_UI_LAYOUT_LINT") else {
            return;
        };
        let tree = crate::inspector::snapshot::Snapshot::capture_headless(
            &self.runner,
            scale_factor() as f32,
        );
        if std::env::var_os("BINGUX_UI_LAYOUT_TREE").is_some() {
            let _ = fs::write(shot.with_extension("tree.txt"), tree.outline());
        }
        let mut found = layout_checks::check(&tree);
        if let (Some(dir), Some(name)) = (
            std::env::var_os("BINGUX_UI_LAYOUT_MIRROR"),
            shot.file_name().and_then(|name| name.to_str()),
        ) {
            let kept = Path::new(&dir).join(format!("{name}.json"));
            if Direction::system().is_rtl() {
                let before = fs::read_to_string(&kept)
                    .ok()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                    .and_then(|value| Structure::from_json(&value));
                if let Some(before) = before {
                    found.extend(layout_checks::direction_asymmetric(&before, &tree));
                }
            } else {
                let _ = fs::create_dir_all(&dir);
                let _ = fs::write(&kept, Structure::of(&tree).json().to_string());
            }
        }
        let findings: Vec<_> = found.iter().map(|finding| finding.json()).collect();
        // The named insets by "Class/name", each value with how often it was
        // measured, for `scripts/layout-lint.py --parity`.
        let mut metrics: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, u32>,
        > = Default::default();
        for metric in crate::inspector::layout_checks::metrics(&tree) {
            *metrics
                .entry(format!("{}/{}", metric.class, metric.name))
                .or_default()
                .entry(format!("{:.1}", metric.value))
                .or_default() += 1;
        }
        let line = serde_json::json!({
            "capture": shot.to_string_lossy(),
            "findings": findings,
            "metrics": metrics,
        });
        use std::io::Write;
        if let Ok(mut out) = fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(out, "{line}");
        }
    }

    pub fn runner(&mut self) -> &mut TestingRunner {
        &mut self.runner
    }

    /// Note an input in the manifest and remember when it happened.
    fn input(&mut self, label: String) {
        self.last_input = Some(Instant::now());
        self.mark(&label);
    }

    /// Record the current frame, if frames are being recorded.
    fn capture_frame(&mut self) {
        if self.recorder.is_none() {
            return;
        }
        let image = self.runner.render();
        let png: &[u8] = &image;
        let now = Instant::now();
        let last_input = self.last_input;
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.frame(png, now, last_input);
        }
    }
}

/// Writes frames and markers for one [`Snapshot`].
struct FrameRecorder {
    dir: PathBuf,
    manifest: File,
    started: Instant,
    /// Frame lines written so far; the next frame's index.
    frames: u64,
    /// The last frame's PNG bytes and file name, to reuse when unchanged.
    previous: Option<(Vec<u8>, String)>,
}

impl FrameRecorder {
    fn frame(&mut self, png: &[u8], now: Instant, last_input: Option<Instant>) {
        let file = match &self.previous {
            Some((bytes, name)) if bytes.as_slice() == png => name.clone(),
            _ => {
                let name = format!("{:06}.png", self.frames);
                fs::write(self.dir.join(&name), png).expect("write a frame");
                self.previous = Some((png.to_vec(), name.clone()));
                name
            }
        };
        let t_ms = now.duration_since(self.started).as_millis();
        let since = match last_input {
            Some(at) => now.saturating_duration_since(at).as_millis().to_string(),
            None => "null".to_string(),
        };
        let _ = writeln!(
            self.manifest,
            r#"{{"kind":"frame","frame":{},"t_ms":{t_ms},"since_input_ms":{since},"file":"{file}"}}"#,
            self.frames
        );
        self.frames += 1;
    }

    fn marker(&mut self, now: Instant, text: &str) {
        let t_ms = now.duration_since(self.started).as_millis();
        let _ = writeln!(
            self.manifest,
            r#"{{"kind":"marker","t_ms":{t_ms},"text":{}}}"#,
            json_string(text)
        );
    }
}

/// `text` as a JSON string literal.
fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `N` comma-separated numbers from a script argument.
fn numbers<const N: usize>(text: &str) -> Result<[f32; N], String> {
    let values = text
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<f32>()
                .map_err(|_| format!("not a number: {}", part.trim()))
        })
        .collect::<Result<Vec<f32>, String>>()?;
    values
        .try_into()
        .map_err(|_| format!("expected {N} numbers: {text}"))
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
pub(crate) fn scale_factor() -> f64 {
    std::env::var("BINGUX_SNAPSHOT_SCALE")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|scale: &f64| *scale >= 1.0)
        .unwrap_or(1.0)
}
