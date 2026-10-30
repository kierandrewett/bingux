//! The Layout tab: the visual defects the layout checks found, grouped by
//! rule in groups that fold. The rules live in
//! [`layout_checks`](super::layout_checks); this file only draws them.
//!
//! Each finding says which rule it breaks and what was measured. Pressing
//! it (or Enter on it) reveals its element in the Elements tab and leaves it
//! outlined in the application window; hovering it outlines the element
//! too. Up and Down walk the rows.

use std::collections::HashSet;

use freya::prelude::*;

use super::layout_checks::{Finding, Rule};
use super::nav::Nav;
use crate::controls::{SpinnerSize, action_button, spinner, stat_tile};
use crate::focus::step_focus;
use crate::group::{Group, caption, heading};
use crate::icons::{self, Icon};
use crate::rows::row;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, Ink, styled_label};

/// The tab. `findings` come from the plugin's last snapshot; before there is
/// one the tab shows a loading state.
pub fn view(findings: &[Finding], nav: Nav) -> Element {
    LayoutPane {
        findings: findings.to_vec(),
        loading: super::snapshot().1.is_none(),
        nav,
    }
    .into()
}

struct LayoutPane {
    findings: Vec<Finding>,
    loading: bool,
    nav: Nav,
}

impl PartialEq for LayoutPane {
    /// The nav state is one handle for the window's life, so it never differs.
    fn eq(&self, other: &Self) -> bool {
        self.loading == other.loading && self.findings == other.findings
    }
}

impl Component for LayoutPane {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut folded = use_state(HashSet::<Rule>::new);
        if self.loading {
            return message(&tokens, true).into_element();
        }
        if self.findings.is_empty() {
            return message(&tokens, false).into_element();
        }
        let rules: Vec<Rule> = Rule::ALL
            .into_iter()
            .filter(|rule| self.findings.iter().any(|finding| finding.rule == *rule))
            .collect();
        let all_folded = rules.iter().all(|rule| folded.read().contains(rule));

        let summary = stat_tile(self.findings.len().to_string(), "Layout problems")
            .icon(Icon::DialogWarning)
            .tone(Tone::Warning);
        let fold_all = {
            let rules = rules.clone();
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
                        folded.write().extend(rules.iter().copied());
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
            .child(rect().tw(&tokens, &[w_flex()]).child(styled_label(
                CAPTION,
                "Not accessibility checks: these are things a person sees.",
            )))
            .child(fold_all);

        let nav = self.nav;
        let groups: Vec<Element> = rules
            .iter()
            .map(|&rule| {
                let of_rule: Vec<&Finding> = self
                    .findings
                    .iter()
                    .filter(|finding| finding.rule == rule)
                    .collect();
                let open = !folded.read().contains(&rule);
                rule_group(&tokens, rule, &of_rule, open, nav, folded)
            })
            .collect();

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

/// One rule's findings as a card: a header row that folds the rows beneath it.
fn rule_group(
    tokens: &Tokens,
    rule: Rule,
    findings: &[&Finding],
    open: bool,
    nav: Nav,
    folded: State<HashSet<Rule>>,
) -> Element {
    let rows: Vec<Element> = findings
        .iter()
        .map(|finding| finding_row(tokens, finding, nav))
        .collect();
    let chevron = icons::icon(
        if open { Icon::PanDown } else { Icon::PanEnd },
        IconSize::Mark,
        Paint::Muted.color(tokens),
    );
    let header = row(rule.label())
        .leading(Icon::DialogWarning)
        .leading_tone(Tone::Warning)
        .badge(findings.len().to_string())
        .subtitle(rule.why())
        .trailing(chevron)
        .on_press(move |_| {
            let mut folded = folded;
            let mut set = folded.write();
            if !set.remove(&rule) {
                set.insert(rule);
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

/// One finding: the measured numbers, the element, the rule's fix. The row
/// reveals the element.
fn finding_row(tokens: &Tokens, finding: &Finding, nav: Nav) -> Element {
    let node = finding.node;
    let place = if finding.path.is_empty() {
        finding.target.clone()
    } else {
        format!("{} in {}", finding.target, finding.path)
    };
    let item = row(finding.what.clone())
        .leading(Icon::DialogWarning)
        .leading_tone(Tone::Warning)
        .subtitle(format!("{place}\n{}", finding.rule.fix()))
        .navigates()
        .on_press(move |_| {
            nav.reveal(node);
            super::set_hover(Some(node));
        });
    rect()
        .tw(tokens, &[w_full()])
        .on_pointer_enter(move |_| super::set_hover(Some(node)))
        .on_pointer_leave(move |_| super::set_hover(None))
        .child(item)
        .into()
}

/// The tab with nothing to list: waiting for the first snapshot, or a window
/// that passes every layout rule.
fn message(tokens: &Tokens, loading: bool) -> Rect {
    let body: Vec<Element> = if loading {
        vec![
            spinner(SpinnerSize::Medium).into(),
            styled_label(CAPTION, "Waiting for an application window"),
        ]
    } else {
        vec![
            icons::icon(Icon::Check, IconSize::Hero, Ink::Success.color(tokens)).into(),
            heading("No layout problems"),
            caption(
                "Padding, centring, sibling heights, empty space, clipping, overlap, cut-off \
                 text and baselines all pass for the window as it is now. The checks run \
                 again as the window changes.",
            ),
        ]
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
