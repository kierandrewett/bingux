//! The Accessibility tab: the problems the checks found, grouped by kind. The
//! checks themselves live in [`a11y_checks`](super::a11y_checks), so this file
//! only draws them.

use freya::prelude::*;

pub use super::a11y_checks::{FrameHistory, Issue, IssueKind, Severity, check};
use super::nav::Nav;
use crate::group::{Group, Status, caption, status};
use crate::layout::page_column;
use crate::rows::row;
use crate::style::*;
use crate::theme::Tokens;
use crate::typography::CAPTION;

/// Lists `issues` grouped by kind. Pressing a row reveals its node in the
/// Elements tab.
pub fn view(tokens: &Tokens, issues: &[Issue], nav: Nav) -> Element {
    let mut sections: Vec<Element> = Vec::new();
    if issues.is_empty() {
        sections.push(status(Status::Info("No accessibility problems found.".to_string())).into());
    } else {
        sections.push(caption(summary(issues)));
        for kind in IssueKind::ALL {
            let of_kind: Vec<&Issue> = issues.iter().filter(|issue| issue.kind == kind).collect();
            if of_kind.is_empty() {
                continue;
            }
            let mut group = Group::new(Some(format!("{} ({})", kind.label(), of_kind.len())));
            for issue in of_kind {
                group = group.row(issue_row(tokens, issue, nav));
            }
            sections.push(group.into());
        }
    }
    ScrollView::new()
        .drag_scrolling(false)
        .width(Size::fill())
        .height(Size::fill())
        .child(
            rect()
                .tw_layout(&[w_full(), px(S4), py(S3)])
                .child(page_column().children(sections)),
        )
        .into()
}

/// One issue: what is wrong as the title, the node as the subtitle, and why and
/// how to fix as the detail.
fn issue_row(tokens: &Tokens, issue: &Issue, nav: Nav) -> crate::rows::Row {
    let item = row(issue.what.clone())
        .subtitle(issue.target.clone())
        .badge(issue.severity.label())
        .nested(
            true,
            vec![
                CAPTION.paragraph(tokens, issue.why.to_string()).into(),
                CAPTION.paragraph(tokens, issue.fix.to_string()).into(),
            ],
        );
    match issue.node {
        Some(node) => item.on_press(move |_| nav.reveal(node)),
        None => item,
    }
}

/// A one-line count of the problems, by severity.
fn summary(issues: &[Issue]) -> String {
    let errors = issues
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .count();
    let warnings = issues
        .iter()
        .filter(|issue| issue.severity == Severity::Warning)
        .count();
    let notes = issues.len() - errors - warnings;
    format!(
        "{} problems: {errors} errors, {warnings} warnings, {notes} notes",
        issues.len()
    )
}
