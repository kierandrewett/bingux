//! The Accessibility tab: the problems the checks found, summarised by
//! severity, filtered, and grouped by kind in groups that fold. The checks
//! themselves live in [`a11y_checks`](super::a11y_checks), so this file only
//! draws them.
//!
//! Pressing an issue (or Enter on it) reveals its element in the Elements tab
//! and leaves it outlined in the application window. Hovering an issue
//! outlines the element in the application too. Up and Down walk the rows.

use std::collections::HashSet;

use freya::prelude::*;

pub use super::a11y_checks::{FrameHistory, Issue, IssueKind, Severity, check};
use super::nav::Nav;
use crate::controls::{SpinnerSize, action_button, segmented, spinner, stat_tile};
use crate::focus::step_focus;
use crate::group::{Group, Status, caption, heading, status};
use crate::icons::{self, Icon};
use crate::rows::{Row, row};
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, Ink, styled_label};

/// The tab. `issues` come from the plugin's last snapshot; before there is
/// one the tab shows a loading state.
pub fn view(_tokens: &Tokens, issues: &[Issue], nav: Nav) -> Element {
    A11yPane {
        issues: issues.to_vec(),
        loading: super::snapshot().1.is_none(),
        nav,
    }
    .into()
}

struct A11yPane {
    issues: Vec<Issue>,
    loading: bool,
    nav: Nav,
}

impl PartialEq for A11yPane {
    /// The nav state is one handle for the window's life, so it never differs.
    fn eq(&self, other: &Self) -> bool {
        self.loading == other.loading && self.issues == other.issues
    }
}

impl Component for A11yPane {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut only = use_state(|| None::<Severity>);
        let mut folded = use_state(HashSet::<IssueKind>::new);
        if self.loading {
            return message(&tokens, MessageKind::Loading).into_element();
        }
        if self.issues.is_empty() {
            return message(&tokens, MessageKind::Clean).into_element();
        }

        let count_of = |severity: Severity| {
            self.issues
                .iter()
                .filter(|issue| issue.severity == severity)
                .count()
        };
        let (errors, warnings, notes) = (
            count_of(Severity::Error),
            count_of(Severity::Warning),
            count_of(Severity::Info),
        );
        let shown: Vec<&Issue> = self
            .issues
            .iter()
            .filter(|issue| only().is_none_or(|severity| issue.severity == severity))
            .collect();
        let kinds: Vec<IssueKind> = IssueKind::ALL
            .into_iter()
            .filter(|kind| shown.iter().any(|issue| issue.kind == *kind))
            .collect();
        let all_folded = !kinds.is_empty() && kinds.iter().all(|kind| folded.read().contains(kind));

        let tile = |count: usize, label: &str, icon: Icon, tone: Tone| {
            let tile = stat_tile(count.to_string(), label).icon(icon);
            rect()
                .tw(&tokens, &[w_flex()])
                .child(if count > 0 { tile.tone(tone) } else { tile })
        };
        let summary = rect()
            .tw(&tokens, &[w_full(), flex_row(), grow_children(), gap(S3)])
            .child(tile(errors, "Errors", Icon::DialogError, Tone::Danger))
            .child(tile(
                warnings,
                "Warnings",
                Icon::DialogWarning,
                Tone::Warning,
            ))
            .child(tile(notes, "Notes", Icon::DialogInformation, Tone::Accent));

        let filter = segmented(
            vec![
                ("all".to_string(), format!("All {}", self.issues.len())),
                ("error".to_string(), format!("Errors {errors}")),
                ("warning".to_string(), format!("Warnings {warnings}")),
                ("info".to_string(), format!("Notes {notes}")),
            ],
            match only() {
                None => "all",
                Some(Severity::Error) => "error",
                Some(Severity::Warning) => "warning",
                Some(Severity::Info) => "info",
            },
            move |key: String| {
                only.set(match key.as_str() {
                    "error" => Some(Severity::Error),
                    "warning" => Some(Severity::Warning),
                    "info" => Some(Severity::Info),
                    _ => None,
                });
            },
        )
        .tabs();
        let fold_all = {
            let kinds = kinds.clone();
            action_button(
                if all_folded {
                    "Expand all"
                } else {
                    "Collapse all"
                },
                move |_| {
                    if all_folded {
                        folded.write().clear();
                    } else {
                        folded.write().extend(kinds.iter().copied());
                    }
                },
            )
            .flat()
            .compact()
        };
        let controls = rect()
            .tw(
                &tokens,
                &[w_full(), flex_row(), items(Align::Center), gap(S3)],
            )
            .child(filter)
            .child(rect().tw(&tokens, &[w_flex()]))
            .maybe_child((!kinds.is_empty()).then_some(fold_all));

        let nav = self.nav;
        let groups: Vec<Element> = if kinds.is_empty() {
            vec![
                status(Status::Empty(match only() {
                    Some(Severity::Error) => "No errors.".to_string(),
                    Some(Severity::Warning) => "No warnings.".to_string(),
                    _ => "No notes.".to_string(),
                }))
                .into(),
            ]
        } else {
            kinds
                .iter()
                .map(|&kind| {
                    let of_kind: Vec<&Issue> =
                        shown.iter().copied().filter(|i| i.kind == kind).collect();
                    let open = !folded.read().contains(&kind);
                    issue_group(&tokens, kind, &of_kind, open, nav, folded)
                })
                .collect()
        };

        rect()
            .tw(&tokens, &[w_full(), h_flex(), flex_col(), grow_children()])
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[w_full(), flex_col(), gap(S3), px(S4), pt(S3), pb(S3)],
                    )
                    .child(summary)
                    .child(controls),
            )
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect().tw(&tokens, &[w_full(), h_flex()]).child(
                    ScrollView::new()
                        .drag_scrolling(false)
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(
                            rect()
                                .tw(&tokens, &[w_full(), flex_col(), gap(S3), px(S4), py(S3)])
                                .children(groups),
                        ),
                ),
            )
            .into_element()
    }
}

/// One kind's issues as a card: a header row that folds the rows beneath it.
fn issue_group(
    tokens: &Tokens,
    kind: IssueKind,
    issues: &[&Issue],
    open: bool,
    nav: Nav,
    folded: State<HashSet<IssueKind>>,
) -> Element {
    let rows: Vec<Element> = issues
        .iter()
        .map(|issue| issue_row(tokens, issue, nav))
        .collect();
    let chevron = icons::icon(
        if open { Icon::PanDown } else { Icon::PanEnd },
        IconSize::Mark,
        Paint::Muted.color(tokens),
    );
    let header = row(kind.label())
        .leading(kind_icon(kind))
        .badge(issues.len().to_string())
        .subtitle(kind.why())
        .trailing(
            rect()
                .tw(tokens, &[flex_row(), items(Align::Center), gap(S3)])
                .child(styled_label(CAPTION, severity_counts(issues)))
                .child(chevron),
        )
        .on_press(move |_| {
            let mut folded = folded;
            let mut set = folded.write();
            if !set.remove(&kind) {
                set.insert(kind);
            }
        })
        .nested(open, rows);
    // A ScrollView takes the arrow keys itself, so the keys are answered here,
    // between the focused row and the scroll view: Up and Down move focus to
    // the neighbouring row.
    rect()
        .tw(tokens, &[w_full()])
        .on_key_down(move |event: Event<KeyboardEventData>| {
            let back = match &event.key {
                Key::Named(NamedKey::ArrowDown) => false,
                Key::Named(NamedKey::ArrowUp) => true,
                _ => return,
            };
            event.stop_propagation();
            step_focus(back);
        })
        .child(Group::new(None).row(header))
        .into()
}

/// One issue: what is wrong, the element it is about, and why and how to fix
/// it. The row reveals the element; the chevron says so.
fn issue_row(tokens: &Tokens, issue: &Issue, nav: Nav) -> Element {
    let detail = format!("{}\n{}", issue.target, issue.fix);
    let item: Row = row(issue.what.clone())
        .leading(severity_icon(issue.severity))
        .leading_tone(severity_tone(issue.severity))
        .subtitle(detail);
    let Some(node) = issue.node else {
        return item.into();
    };
    rect()
        .tw(tokens, &[w_full()])
        .on_pointer_enter(move |_| super::set_hover(Some(node)))
        .on_pointer_leave(move |_| super::set_hover(None))
        .child(item.navigates().on_press(move |_| jump(nav, node)))
        .into()
}

/// Reveals `node` in the Elements tab and outlines it in the application,
/// where it stays outlined until the pointer moves over the tree.
fn jump(nav: Nav, node: u64) {
    nav.reveal(node);
    super::set_hover(Some(node));
}

fn severity_icon(severity: Severity) -> Icon {
    match severity {
        Severity::Error => Icon::DialogError,
        Severity::Warning => Icon::DialogWarning,
        Severity::Info => Icon::DialogInformation,
    }
}

fn severity_tone(severity: Severity) -> Tone {
    match severity {
        Severity::Error => Tone::Danger,
        Severity::Warning => Tone::Warning,
        Severity::Info => Tone::Accent,
    }
}

fn kind_icon(kind: IssueKind) -> Icon {
    match kind {
        IssueKind::Naming => Icon::Accessibility,
        IssueKind::Focus => Icon::Keyboard,
        IssueKind::TargetSize => Icon::Mouse,
        IssueKind::Contrast => Icon::Appearance,
        IssueKind::Geometry => Icon::Display,
        IssueKind::LayoutShift => Icon::Desktop,
        IssueKind::Motion => Icon::Refresh,
        IssueKind::Performance => Icon::Clock,
    }
}

/// "2 errors, 1 warning" for the issues of a group.
fn severity_counts(issues: &[&Issue]) -> String {
    let count = |severity: Severity| {
        issues
            .iter()
            .filter(|issue| issue.severity == severity)
            .count()
    };
    let part = |n: usize, one: &str, many: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    };
    [
        part(count(Severity::Error), "error", "errors"),
        part(count(Severity::Warning), "warning", "warnings"),
        part(count(Severity::Info), "note", "notes"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ")
}

enum MessageKind {
    Loading,
    Clean,
}

/// The tab with nothing to list: waiting for the first snapshot, or a window
/// that passes every check.
fn message(tokens: &Tokens, kind: MessageKind) -> Rect {
    let body: Vec<Element> = match kind {
        MessageKind::Loading => vec![
            spinner(SpinnerSize::Medium).into(),
            styled_label(CAPTION, "Waiting for an application window"),
        ],
        MessageKind::Clean => vec![
            icons::icon(Icon::Check, IconSize::Hero, Ink::Success.color(tokens)).into(),
            heading("No accessibility problems"),
            caption(
                "Names, focus, target size, contrast, geometry, layout shift, motion and \
                 render cost all pass for the window as it is now. The checks run again \
                 as the window changes.",
            ),
        ],
    };
    rect()
        .tw(
            tokens,
            &[
                w_full(),
                h_flex(),
                flex_col(),
                items(Align::Center),
                justify(Align::Center),
                gap(S3),
                px(S12),
            ],
        )
        .children(body)
}
