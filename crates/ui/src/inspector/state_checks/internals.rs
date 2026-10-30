//! The internals of a dropdown or a field (CK-15): what is inside the control's
//! box and in what order.
//!
//! - A label never runs under a glyph beside it (`text-under-glyph`, AL-12,
//!   PH-07): the chevron takes real layout space, so the text ends before it
//!   at every width, with long data and at the phone size.
//! - The chevron of a dropdown or a split button is the small one,
//!   `IconSize::Mark` at the window's size class (`chevron-size`, SZ-04).

use torin::prelude::{Area, Size2D};

use super::view::{Target, border, visible_box};
use super::{Report, Rule};
use crate::inspector::snapshot::{Kind, Node, Snapshot as Tree};
use crate::style::{IconSize, SizeClass};

/// The glyphs of a control are no bigger than this: a bigger box is not an icon.
const GLYPH_MAX: f32 = 40.0;

/// How far a text box and a glyph box overlap, when they do by more than a
/// hairline in both axes.
fn overlap(text: Area, glyph: Area) -> Option<f32> {
    let both = text.intersection(&glyph)?;
    (both.width() > 0.5 && both.height() > 0.5).then(|| both.width())
}

/// The text boxes (as wide as the text, not the room it was given) and the
/// glyph boxes below `node`.
fn parts(tree: &Tree, node: &Node, window: Area) -> (Vec<Area>, Vec<Area>) {
    let (mut texts, mut glyphs) = (Vec::new(), Vec::new());
    let mut stack: Vec<u64> = node.children.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        let Some(here) = tree.node(id) else { continue };
        stack.extend(here.children.iter().rev().copied());
        if here.used.hidden {
            continue;
        }
        let Some(area) = visible_box(tree, here, window).or_else(|| border(here)) else { continue };
        match here.kind {
            Kind::Label | Kind::Paragraph if here.text.as_deref().is_some_and(|text| !text.is_empty()) => {
                let width = here
                    .used
                    .text
                    .as_ref()
                    .and_then(|text| text.measured)
                    .map_or(area.width(), |(width, _)| width.min(area.width()));
                texts.push(Area::new(area.origin, Size2D::new(width, area.height())));
            }
            Kind::Other if area.width() > 0.0 && area.width() <= GLYPH_MAX && area.height() <= GLYPH_MAX => {
                glyphs.push(area);
            }
            _ => {}
        }
    }
    (texts, glyphs)
}

pub(super) fn run(tree: &Tree, targets: &[Target], report: &mut Report) {
    let window = super::view::window(tree);
    for target in targets {
        // A phone specimen in a desktop window is compact too: the size class
        // is the control's own, read from its height (36 px, 44 for a finger).
        let class = if target.face.height() >= 43.0 { SizeClass::Compact } else { SizeClass::Regular };
        let mark = IconSize::Mark.px_in(class);
        let dropdown = target.role == "ComboBox"
            || matches!(target.component.as_str(), "Dropdown" | "SplitButtonMenu" | "PlacesMenu");
        let field = target.role.contains("Input") || target.component == "TextField";
        if !dropdown && !field && target.component != "MenuButton" {
            continue;
        }
        let Some(node) = tree.node(target.id) else { continue };
        let (texts, glyphs) = parts(tree, node, window);
        if glyphs.is_empty() {
            continue;
        }
        report.measure(Rule::TextUnderGlyph);
        'texts: for text in &texts {
            for glyph in &glyphs {
                if let Some(width) = overlap(*text, *glyph) {
                    report.push(
                        Rule::TextUnderGlyph,
                        target.label(),
                        format!(
                            "text at x {:.0}..{:.0} runs {width:.0} px under a glyph at x {:.0}..{:.0}",
                            text.min_x(),
                            text.max_x(),
                            glyph.min_x(),
                            glyph.max_x()
                        ),
                        target.rect(),
                    );
                    break 'texts;
                }
            }
        }
        // The trailing glyph of a dropdown is its chevron.
        if dropdown && let Some(chevron) = glyphs.iter().max_by(|a, b| a.max_x().total_cmp(&b.max_x())) {
            report.measure(Rule::ChevronSize);
            // A chevron is a wide glyph in its box: the width is the size.
            if (chevron.width() - mark).abs() > 0.5 {
                report.push(
                    Rule::ChevronSize,
                    target.label(),
                    format!(
                        "the chevron is {:.0} px wide; the small one, IconSize::Mark, is {mark:.0} at this size class",
                        chevron.width()
                    ),
                    target.rect(),
                );
            }
        }
    }
}
