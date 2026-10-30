//! The interaction-state checks: what a control does when it is pointed at,
//! pressed, focused, tabbed to, opened and navigated by key, found by driving
//! a real headless window rather than reading its declared style.
//!
//! The layout rules (`layout_checks`, `checks`) read one captured tree. These
//! read what the window *does*: they move the pointer onto every interactive
//! node, press it, focus it and render each state, and they walk the keyboard
//! through the page. Each finding cites the rule IDs of `design/rules.json`
//! (`[ST-06]`) so it can be read against what Kieran said.
//!
//! | rule key | IDs | what it holds |
//! |---|---|---|
//! | `state-hover`, `state-press`, `state-focus`, `ring-clipped` | ST-06, FC-01, FC-04 | CK-03, CK-14: rest, hover, pressed and focus look different, the ring is on all four sides |
//! | `state-selected`, `hover-collateral`, `row-hover`, `state-open`, `state-disabled` | ST-01, ST-04, ST-05, ST-06, ST-09 | CK-03: selection reads, hovering a neighbour leaves it alone, only a pressable row has a hover fill, an open menu keeps its button's hover look, a disabled control is dim and still |
//! | `touch-target` | SZ-01, SZ-11, PH-04 | CK-12: every interactive node's box is 44 px at compact density |
//! | `resize-clearance`, `scrollbar-clearance` | SZ-06, SC-01, SC-02 | CK-13: no control under a resize handle or the scrollbar's gutter |
//! | `tab-reach`, `tab-order`, `tab-ring`, `tab-trap`, `popup-escape` | FC-04, FC-05, FC-07 | CK-18: Tab visits every control once, in reading order, with a ring, and does not get stuck |
//! | `menu-keys` | FC-11, MN-02, MN-03, MN-05 | CK-16: arrows, Home, End, type-ahead, Enter and Escape in every menu and dropdown |
//! | `cursor-map` | ST-03 | CK-25: the cursor over each control is the arrow, the I-beam only over text |
//! | `text-under-glyph`, `chevron-size` | AL-12, PH-07, SZ-04 | CK-15: a dropdown's label ends before its chevron, and the chevron is the small one |
//!
//! [`Snapshot::state_report`] runs them over the current frame. With
//! `BINGUX_UI_STATE_CHECK=FILE` set, every capture the run saves appends its
//! report to FILE as one JSON line, and `scripts/state-check.py` reads them.
//! `BINGUX_UI_STATE_SCOPE` (`static,cursor,tab,sweep,menu`, default all)
//! narrows the run and `BINGUX_UI_STATE_LIMIT` caps the nodes it sweeps.
//!
//! What a control is allowed not to do is the [`ALLOW`] list, each with its
//! reason. An entry that matches nothing in a run is reported by the script.

use std::collections::BTreeMap;

use crate::snapshot::Snapshot;

mod cursor;
mod frame;
mod internals;
mod menus;
mod rig;
mod statics;
mod sweep;
mod tabs;
#[cfg(test)]
mod tests;
mod view;

/// Below this window width the apps lay out at compact (finger) density.
pub(super) const COMPACT_BELOW: f32 = 740.0;

/// One rule of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    Hover,
    Press,
    Focus,
    RingClipped,
    Selected,
    Collateral,
    RowHover,
    Open,
    Disabled,
    Touch,
    ResizeClear,
    ScrollbarClear,
    TabReach,
    TabOrder,
    TabRing,
    TabTrap,
    PopupEscape,
    MenuKeys,
    Cursor,
    TextUnderGlyph,
    ChevronSize,
}

/// What a rule says about itself.
pub struct Meta {
    pub key: &'static str,
    pub label: &'static str,
    /// The rule IDs of `design/rules.json` it enforces.
    pub ids: &'static str,
    pub why: &'static str,
    pub fix: &'static str,
}

impl Rule {
    pub const ALL: [Rule; 21] = [
        Rule::Hover,
        Rule::Press,
        Rule::Focus,
        Rule::RingClipped,
        Rule::Selected,
        Rule::Collateral,
        Rule::RowHover,
        Rule::Open,
        Rule::Disabled,
        Rule::Touch,
        Rule::ResizeClear,
        Rule::ScrollbarClear,
        Rule::TabReach,
        Rule::TabOrder,
        Rule::TabRing,
        Rule::TabTrap,
        Rule::PopupEscape,
        Rule::MenuKeys,
        Rule::Cursor,
        Rule::TextUnderGlyph,
        Rule::ChevronSize,
    ];

    pub fn meta(self) -> Meta {
        match self {
            Rule::Hover => Meta {
                key: "state-hover",
                label: "No hover state",
                ids: "ST-06",
                why: "Every control that can be pressed looks different under the pointer, so a person can tell it is live before they press.",
                fix: "Give the face a hover surface (surface(.., hovering(), ..)) or, if the control is meant to look the same, list it in ALLOW with the reason.",
            },
            Rule::Press => Meta {
                key: "state-press",
                label: "No pressed state",
                ids: "ST-06",
                why: "A pressed control looks different from a hovered one, so the press is acknowledged before the action runs.",
                fix: "Pass the pressing state to the surface so the pressed fill differs from the hover fill.",
            },
            Rule::Focus => Meta {
                key: "state-focus",
                label: "No focus ring",
                ids: "FC-04, FC-01",
                why: "Everything that takes keyboard focus shows a ring around its painted box; focus is on by default and an app never opts in.",
                fix: "End the control's render with .focusable(Focus::Keyboard) on the node that carries the fill.",
            },
            Rule::RingClipped => Meta {
                key: "ring-clipped",
                label: "Focus ring clipped",
                ids: "FC-01",
                why: "The ring is drawn whole, on all four sides, and is never cut off by a clipping ancestor or the window edge.",
                fix: "Give the clipping container room for the ring (the outset is Dim::Ring plus the gap), or use focusable_inside for a face that sits on a clipped edge.",
            },
            Rule::Selected => Meta {
                key: "state-selected",
                label: "Selected looks unselected",
                ids: "ST-09, ST-05",
                why: "A selected or checked item looks different from its unselected siblings at rest.",
                fix: "Paint the selected state (surface(.., selected)) on the item.",
            },
            Rule::Collateral => Meta {
                key: "hover-collateral",
                label: "Hover changes another control",
                ids: "ST-05",
                why: "Pointing at one item never changes how the selected item next to it looks.",
                fix: "Keep the selected item's fill the same whether or not another item is hovered.",
            },
            Rule::RowHover => Meta {
                key: "row-hover",
                label: "Hover fill on a row that does nothing",
                ids: "ST-01",
                why: "Only an activatable row (a chevron, on_press, opens something) shows a hover fill. Switch, slider, dropdown and value rows do not.",
                fix: "Gate the row's hover surface on it being activatable.",
            },
            Rule::Open => Meta {
                key: "state-open",
                label: "Open control loses its hover look",
                ids: "ST-06",
                why: "A dropdown or menu button keeps its hover fill while its menu is open.",
                fix: "Pass the open state to the button's surface as hovering.",
            },
            Rule::Disabled => Meta {
                key: "state-disabled",
                label: "Disabled control",
                ids: "ST-04",
                why: "A disabled control is dimmed (DISABLED_OPACITY) and shows no hover.",
                fix: "Apply DISABLED_OPACITY and drop the hover surface while the control is disabled.",
            },
            Rule::Touch => Meta {
                key: "touch-target",
                label: "Touch target too small",
                ids: "SZ-01, SZ-11, PH-04",
                why: "Every control a finger presses is at least 44 px square at compact density (24 px on a desktop window), measured on the rendered box, not by token.",
                fix: "Size the control with its Dim token, or pad the hit area to the minimum; a hand-built box needs the same.",
            },
            Rule::ResizeClear => Meta {
                key: "resize-clearance",
                label: "Control under a resize handle",
                ids: "SZ-06, SC-02",
                why: "No control overlaps a window or popup resize edge, so grabbing the edge never presses a control and pressing a control never starts a resize.",
                fix: "Inset the control from the panel edge by the handle's inside band, or narrow the handle.",
            },
            Rule::ScrollbarClear => Meta {
                key: "scrollbar-clearance",
                label: "Scrollbar overlaps",
                ids: "SC-01, SC-02",
                why: "The scrollbar sits inside the scroll view below the header and clear of the resize handles, and no control is pressable only through it.",
                fix: "Keep the scrollbar's start inset below the floating header and its end inset clear of the corner handle.",
            },
            Rule::TabReach => Meta {
                key: "tab-reach",
                label: "Tab does not reach every control once",
                ids: "FC-04, FC-05",
                why: "Tab visits every interactive control exactly once, one stop per control, and Shift+Tab visits them in the reverse order.",
                fix: "Make the control focusable (one a11y id), and give a composite control one stop (roving focus), not one per part.",
            },
            Rule::TabOrder => Meta {
                key: "tab-order",
                label: "Tab order is not reading order",
                ids: "FC-05",
                why: "Tab moves forward through the page the way it reads: top to bottom, start to end, never back up the page.",
                fix: "Build the controls in reading order, or declare the order with the focus regions.",
            },
            Rule::TabRing => Meta {
                key: "tab-ring",
                label: "No ring at a Tab stop",
                ids: "FC-04, FC-01",
                why: "Every stop of the Tab walk shows its focus ring.",
                fix: "As for state-focus.",
            },
            Rule::TabTrap => Meta {
                key: "tab-trap",
                label: "Tab trap",
                ids: "FC-07, FC-05",
                why: "Tab always leaves a control and comes round again; a popup or dialog holds the focus inside itself.",
                fix: "Do not swallow Tab in the control; scope a popup with a modal trap, so nothing behind it takes a stop.",
            },
            Rule::PopupEscape => Meta {
                key: "popup-escape",
                label: "Escape does not close the popup",
                ids: "FC-07, MN-01",
                why: "Escape closes a popup, sheet or dialog.",
                fix: "Give the popup its on_escape/dismiss handler.",
            },
            Rule::MenuKeys => Meta {
                key: "menu-keys",
                label: "Menu keyboard",
                ids: "FC-11, MN-02, MN-03",
                why: "In a menu or dropdown the arrows, Home and End move the highlight, typing filters it and keeps the query, Enter picks and Escape closes and returns focus to the button.",
                fix: "Route the keys through the menu's navigation (shell::menu_nav) and give the button back its focus on close.",
            },
            Rule::Cursor => Meta {
                key: "cursor-map",
                label: "Wrong cursor",
                ids: "ST-03",
                why: "Cursors are quiet: the arrow over rows, buttons and the controls inside a field, the I-beam only over text that can be edited or selected, resize cursors only on resize handles.",
                fix: "Declare .cursor(CursorIcon::default()) on a control inside a text field, and do not set Pointer on a row.",
            },
            Rule::TextUnderGlyph => Meta {
                key: "text-under-glyph",
                label: "Text under a glyph",
                ids: "AL-12, PH-07",
                why: "The label of a dropdown, menu button or field never runs under the chevron or icon beside it: the glyph takes real layout space and the text ends before it, at every width and with long data.",
                fix: "Put the glyph in the row's flow (not absolute) and let the label take the room that is left, with an ellipsis.",
            },
            Rule::ChevronSize => Meta {
                key: "chevron-size",
                label: "Chevron too big",
                ids: "SZ-04",
                why: "The chevron in a dropdown or split button is the small one, IconSize::Mark (12 px, 14 at compact density).",
                fix: "Draw the chevron at IconSize::Mark and measure the button's chrome with the same size.",
            },
        }
    }

    pub fn key(self) -> &'static str {
        self.meta().key
    }
}

/// One thing wrong, with where it is.
#[derive(Clone, Debug)]
pub struct Finding {
    pub rule: Rule,
    /// The control, as a reader names it.
    pub target: String,
    pub detail: String,
    /// The control's box, `[x, y, width, height]`; zeros for a window-wide one.
    pub area: [f32; 4],
    /// The reason an entry of [`ALLOW`] covers it.
    pub allowed: Option<&'static str>,
}

impl Finding {
    pub fn json(&self) -> serde_json::Value {
        let meta = self.rule.meta();
        serde_json::json!({
            "rule": meta.key,
            "ids": meta.ids,
            "target": self.target,
            "detail": format!("[{}] {}", meta.ids, self.detail),
            "area": self.area,
            "allowed": self.allowed,
        })
    }
}

/// A control that is meant not to do what a rule asks, with why. A finding is
/// allowed when its rule matches, its path (the components it is built in)
/// contains `component` and its name contains `name`.
pub struct Allow {
    pub rule: Rule,
    pub component: &'static str,
    pub name: &'static str,
    pub reason: &'static str,
}

/// Findings that are right by design. Every entry says why. The fixes are the
/// shared components', so nothing here hides a defect: an entry is a control
/// that is stateless, small or quiet on purpose.
pub const ALLOW: &[Allow] = &[
    Allow {
        rule: Rule::Hover,
        component: "Segment",
        name: "",
        reason: "Only the selected segment is a Tab stop and a target, and the selected segment is raised already: a hover brightens an unselected segment's text and leaves the selected one alone, so it never reads as a second selection (controls/segmented.rs).",
    },
    Allow {
        rule: Rule::Press,
        component: "Segment",
        name: "",
        reason: "The selected segment is the one that is a target; pressing it selects what is selected, so it has no pressed look (controls/segmented.rs).",
    },
    Allow {
        rule: Rule::Hover,
        component: "TabStrip > Tab",
        name: "",
        reason: "The selected tab already has the selection fill (design/tabs.md); only the other tabs take a hover fill, and a roving strip makes the selected tab its one Tab stop.",
    },
    Allow {
        rule: Rule::Press,
        component: "TabStrip > Tab",
        name: "",
        reason: "A press on the selected tab selects what is selected: no pressed look (design/tabs.md).",
    },
    Allow {
        rule: Rule::Hover,
        component: "Stepper",
        name: "",
        reason: "The middle of a stepper is its value, plain text; the minus and plus zones at its ends take the hover and press fills (controls/stepper.rs).",
    },
    Allow {
        rule: Rule::Press,
        component: "Stepper",
        name: "",
        reason: "The middle of a stepper is its value; the minus and plus zones take the pressed fill.",
    },
    Allow {
        rule: Rule::Press,
        component: "TextField",
        name: "",
        reason: "A press in a text field puts the caret in it and moves focus there: the caret and the focus ring are the response, a text field has no pressed fill.",
    },
    Allow {
        rule: Rule::Disabled,
        component: "ActionButton",
        name: "",
        reason: "A button that is busy (`loading`) is disabled while its spinner shows and keeps its colour so the label and the spinner read; a button disabled with `enabled(false)` is dimmed (controls/button.rs).",
    },
    Allow {
        rule: Rule::TabReach,
        component: "AddressEdit",
        name: "",
        reason: "In the path field Tab completes the path and then walks the completions (files/address_edit.rs, SF-05); focus leaves the field with Escape or Enter, so a lap that reaches the field does not go on from it.",
    },
    Allow {
        rule: Rule::Hover,
        component: "ColourPlane",
        name: "",
        reason: "The colour plane is a two-axis drag surface whose handle is the control: it has no hover look of its own and its handle takes the press (ColourPicker owns it).",
    },
    Allow {
        rule: Rule::PopupEscape,
        component: "Window",
        name: "Checking for updates",
        reason: "A progress popup for a running task has no Cancel and cannot be dismissed until the task ends (taxonomy specimen of an indeterminate bar in a popup).",
    },
];

fn allowed_by(rule: Rule, target: &str) -> Option<&'static str> {
    ALLOW
        .iter()
        .find(|entry| entry.rule == rule && target.contains(entry.component) && target.contains(entry.name))
        .map(|entry| entry.reason)
}

/// What a run measured and found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Interactive nodes in the frame.
    pub interactive: usize,
    /// Of those, the ones whose states were rendered.
    pub swept: usize,
    /// Tab stops visited going forward.
    pub stops: usize,
    /// Menus and dropdowns opened and walked.
    pub menus: usize,
    /// Controls whose cursor was read.
    pub cursors: usize,
    pub findings: Vec<Finding>,
    /// Things the run could not do, which are not findings: a press that
    /// changed the window, a control that never opened.
    pub notes: Vec<String>,
    /// How many times each rule was measured (for a rule with a population).
    pub measured: BTreeMap<&'static str, usize>,
    /// The interactive nodes by accessibility role and component, to read what
    /// the frame holds.
    pub kinds: BTreeMap<String, usize>,
}

impl Report {
    pub(super) fn push(&mut self, rule: Rule, target: String, detail: String, area: [f32; 4]) {
        let allowed = allowed_by(rule, &target);
        self.findings.push(Finding { rule, target, detail, area, allowed });
    }

    pub(super) fn measure(&mut self, rule: Rule) {
        *self.measured.entry(rule.key()).or_default() += 1;
    }

    pub(super) fn note(&mut self, text: impl Into<String>) {
        self.notes.push(text.into());
    }

    /// The findings no allowlist entry covers.
    pub fn failures(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|finding| finding.allowed.is_none())
    }

    pub fn json(&self, capture: &str) -> serde_json::Value {
        serde_json::json!({
            "capture": capture,
            "interactive": self.interactive,
            "swept": self.swept,
            "stops": self.stops,
            "menus": self.menus,
            "cursors": self.cursors,
            "findings": self.findings.iter().map(Finding::json).collect::<Vec<_>>(),
            "notes": self.notes,
            "measured": self.measured,
            "kinds": self.kinds,
        })
    }
}

/// Which parts of the run are on.
#[derive(Clone, Copy, Debug)]
pub struct Scope {
    pub statics: bool,
    pub cursor: bool,
    pub tab: bool,
    pub sweep: bool,
    pub menu: bool,
    /// The most nodes the sweep renders.
    pub limit: usize,
    /// The most controls of one kind (role and component) it tries.
    pub per_kind: usize,
    /// Seconds the sweep, the Tab walk and the menu walk may take together.
    pub budget: u64,
}

impl Scope {
    pub const ALL: Scope = Scope { statics: true, cursor: true, tab: true, sweep: true, menu: true, limit: 160, per_kind: 3, budget: 60 };

    pub fn from_env() -> Self {
        let mut scope = Scope::ALL;
        if let Ok(list) = std::env::var("BINGUX_UI_STATE_SCOPE") {
            let on = |name: &str| list.split(',').any(|part| part.trim() == name);
            scope.statics = on("static");
            scope.cursor = on("cursor");
            scope.tab = on("tab");
            scope.sweep = on("sweep");
            scope.menu = on("menu");
        }
        let number = |name: &str| std::env::var(name).ok().and_then(|value| value.parse().ok());
        scope.limit = number("BINGUX_UI_STATE_LIMIT").unwrap_or(scope.limit);
        scope.per_kind = number("BINGUX_UI_STATE_PER_KIND").unwrap_or(scope.per_kind);
        scope.budget = number("BINGUX_UI_STATE_BUDGET").unwrap_or(scope.budget as usize) as u64;
        scope
    }
}

impl Snapshot {
    /// Runs the interaction-state checks over the current frame, with the
    /// pointer and focus where a run leaves them: off everything, and the
    /// window as it was (a trial that changes it is undone, or the run stops
    /// with a note).
    pub fn state_report(&mut self, scope: Scope) -> Report {
        let mut report = Report::default();
        let mut rig = rig::Rig::new(self, std::time::Duration::from_secs(scope.budget), scope.per_kind);
        let tree = rig.tree();
        let targets = view::targets(&tree);
        report.interactive = targets.len();
        for target in &targets {
            *report.kinds.entry(format!("{} {}", target.role, target.component)).or_default() += 1;
        }
        let ids = view::signature(&tree);
        rig.park();
        // A capture with a menu open (a context menu the script raised) has
        // the menu over part of the page, so the controls under it cannot be
        // pointed at or ringed: close it, the menu walk opens its own.
        for _ in 0..2 {
            let tree = rig.tree();
            // A popup or dialog is what the capture is of: leave it.
            let modal = tree.nodes.iter().any(|node| node.a11y.modal);
            let open = !modal && (menus::is_open(&mut rig) || view::panel_open(&tree));
            if !open {
                break;
            }
            rig.named(freya::prelude::NamedKey::Escape);
            rig.park();
        }
        if scope.statics {
            statics::run(&mut rig, &targets, &mut report);
            internals::run(&tree, &targets, &mut report);
        }
        if scope.cursor {
            cursor::run(&mut rig, &targets, &mut report);
        }
        let mut trialed = std::collections::HashSet::new();
        if scope.sweep {
            if rig.restore(&ids, 3) {
                trialed = sweep::run(&mut rig, &ids, scope.limit, &mut report);
            } else {
                report.note("the sweep did not start: the window was not as captured after the static checks");
            }
        }
        if scope.tab {
            if rig.restore(&ids, 3) {
                tabs::walk(&mut rig, &ids, &trialed, &mut report);
            } else {
                report.note("the Tab walk did not start: the window was not as captured");
            }
        }
        if scope.menu {
            if rig.restore(&ids, 3) {
                menus::run(&mut rig, &ids, &mut report);
            } else {
                report.note("the menu walk did not start: the window was not as captured");
            }
        }
        if scope.tab && rig.restore(&ids, 3) {
            tabs::escape(&mut rig, &mut report);
        }
        report
    }

    /// With `BINGUX_UI_STATE_CHECK=FILE` set, appends the state report of this
    /// capture to FILE as a JSON line, after the PNG is written.
    pub(crate) fn lint_states(&mut self, shot: &std::path::Path) {
        use std::io::Write;
        let Some(file) = std::env::var_os("BINGUX_UI_STATE_CHECK") else {
            return;
        };
        let report = self.state_report(Scope::from_env());
        let line = report.json(&shot.to_string_lossy());
        if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(out, "{line}");
        }
    }
}
