//! The tooltip check: hover every icon button of a frame and measure the
//! bubble that opens.
//!
//! An icon button is named by its tooltip, so the frame's icon buttons are the
//! controls that have nothing but their tooltip to say what they do. For each
//! one the check moves the pointer onto it, waits for the bubble (the label
//! with the control's name that was not in the frame before, drawn opaque) and
//! reports what is wrong with it:
//!
//! - **Wrapped narrow.** The text is on more than one line although the
//!   bubble is narrower than [`Dim::TooltipMax`]: a bubble is as wide as its
//!   text on one line, up to that cap, so it was laid out in a room that was
//!   too small (the width of the control it hangs from, or of a clipped box).
//! - **Vertical.** A wrapped bubble with fewer than three characters a line.
//! - **Out of the window.** The bubble is not inside the window.
//! - **Over its control.** The bubble covers the control it describes.
//!
//! A control that shows no bubble (covered by an overlay, scrolled out of the
//! window, a touch layout) is counted, not reported: it is not this check's
//! rule. `BINGUX_UI_TOOLTIP_CHECK=FILE` makes [`Snapshot::save`] append the
//! result of each capture to FILE for `scripts/tooltip-check.py`.

use std::time::Duration;

use freya::prelude::*;
use torin::prelude::Area;

use crate::snapshot::Snapshot;
use crate::style::*;
use crate::typography::CAPTION;

/// The most frames the check waits for one bubble: the 500 ms rest and the fade.
const FRAMES_TO_SHOW: usize = 70;
const FRAME: Duration = Duration::from_millis(16);
/// Fewer characters on a line than this is a column of letters, not text.
const MIN_PER_LINE: usize = 3;
/// How wide a bubble may be and still count as capped by `Dim::TooltipMax`.
const CAP_SLACK: f32 = 2.0;
/// The edge margin the placement keeps from the window.
const EDGE: f32 = 8.0;
/// Windows narrower than this lay out at the compact density.
const COMPACT_BELOW: f32 = 600.0;

/// One thing wrong with one bubble.
#[derive(Clone, Debug)]
pub struct TooltipFinding {
    /// The control's accessible name, which is its tooltip text.
    pub name: String,
    /// The rule: `wrapped narrow`, `vertical`, `out of the window` or `over its control`.
    pub rule: &'static str,
    pub detail: String,
    pub control: Area,
    pub bubble: Area,
}

/// What a frame's tooltip check saw.
#[derive(Clone, Debug, Default)]
pub struct TooltipReport {
    /// Icon buttons with a name that were inside the window.
    pub controls: usize,
    /// Of those, the ones whose bubble was drawn and measured.
    pub shown: usize,
    /// The names of the controls whose bubble never drew (covered by an
    /// overlay, disabled, or a name that is not a tooltip).
    pub missing: Vec<String>,
    pub findings: Vec<TooltipFinding>,
}

/// The labels with `text` in this frame: each one's area.
fn labels(shot: &mut Snapshot, text: &str) -> Vec<Area> {
    shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        (label.text == text).then(|| node.layout().area)
    })
}

/// The bubble around the label at `label`: the smallest box with an opacity of
/// its own that holds the label's middle, when it is opaque.
fn bubble_around(shot: &mut Snapshot, label: Area) -> Option<Area> {
    let centre = Area::center(&label);
    let around = shot.runner().find_many(|node, element| {
        let opacity = element.effect()?.opacity?;
        let area = node.layout().area;
        area.contains(centre).then(|| (area, opacity))
    });
    let (area, opacity) = around
        .into_iter()
        .min_by(|a, b| (a.0.width() * a.0.height()).total_cmp(&(b.0.width() * b.0.height())))?;
    (opacity > 0.99).then_some(area)
}

/// The window's size, from the largest box of the frame.
fn window(shot: &mut Snapshot) -> (f32, f32) {
    shot.runner()
        .find_many(|node, _| {
            let area = node.layout().area;
            Some((area.max_x(), area.max_y()))
        })
        .into_iter()
        .fold((0.0, 0.0), |(w, h), (x, y)| (w.max(x), h.max(y)))
}

impl Snapshot {
    /// Hovers every named icon button of the current frame and checks its
    /// bubble. The pointer ends off every control, with the bubbles gone.
    pub fn tooltip_findings(&mut self) -> TooltipReport {
        let (width, height) = window(self);
        // A snapshot has no window context to read the size class from: the
        // phone size is the compact one.
        let class = if width < COMPACT_BELOW { SizeClass::Compact } else { SizeClass::Regular };
        let line = CAPTION.size.px_in(class) * CAPTION.leading;
        let cap = Dim::TooltipMax.px_in(class);
        let mut report = TooltipReport::default();
        let mut seen: Vec<(String, Area)> = Vec::new();
        for button in self.icon_buttons() {
            let name = button.name.clone();
            let area = button.area;
            let centre = Area::center(&area);
            let inside = area.width() > 0.0
                && area.height() > 0.0
                && centre.x > 0.0
                && centre.y > 0.0
                && centre.x < width
                && centre.y < height;
            if name.is_empty() || !inside || seen.iter().any(|(n, a)| *n == name && *a == area) {
                continue;
            }
            seen.push((name.clone(), area));
            report.controls += 1;

            let before = labels(self, &name);
            self.runner().move_cursor((centre.x as f64, centre.y as f64));
            let mut found = None;
            for _ in 0..FRAMES_TO_SHOW {
                self.runner().poll_n(FRAME, 1);
                let now = labels(self, &name);
                found = now
                    .into_iter()
                    .filter(|label| !before.contains(label))
                    .find_map(|label| bubble_around(self, label).map(|bubble| (label, bubble)));
                if found.is_some() {
                    break;
                }
            }
            if found.is_none() {
                report.missing.push(name.clone());
            }
            if let Some((label, bubble)) = found {
                report.shown += 1;
                let lines = ((label.height() / line).round() as usize).max(1);
                let characters = name.chars().count();
                let wrapped_narrow = lines > 1 && bubble.width() < cap - CAP_SLACK;
                let mut flag = |rule: &'static str, detail: String| {
                    report.findings.push(TooltipFinding {
                        name: name.clone(),
                        rule,
                        detail,
                        control: area,
                        bubble,
                    })
                };
                if wrapped_narrow {
                    flag(
                        "wrapped narrow",
                        format!(
                            "{characters} characters on {lines} lines in a bubble {:.0} px wide, under the {cap:.0} px cap",
                            bubble.width()
                        ),
                    );
                }
                if lines > 1 && characters / lines < MIN_PER_LINE {
                    flag(
                        "vertical",
                        format!("{characters} characters on {lines} lines: under {MIN_PER_LINE} a line"),
                    );
                }
                if bubble.min_x() < EDGE - 0.5
                    || bubble.min_y() < EDGE - 0.5
                    || bubble.max_x() > width - EDGE + 0.5
                    || bubble.max_y() > height - EDGE + 0.5
                {
                    flag(
                        "out of the window",
                        format!(
                            "bubble x {:.0}..{:.0} y {:.0}..{:.0} in a {width:.0}x{height:.0} window",
                            bubble.min_x(),
                            bubble.max_x(),
                            bubble.min_y(),
                            bubble.max_y()
                        ),
                    );
                }
                if bubble.intersects(&area) {
                    flag("over its control", "the bubble covers the control".to_string());
                }
            }
            // Off the control and past the grace time: the next one waits its rest
            // like a person's pointer arriving, and no bubble is left to confuse it.
            self.runner().move_cursor(((width - 1.0) as f64, (height - 1.0) as f64));
            self.runner().poll(FRAME, Duration::from_millis(450));
        }
        report
    }
}
