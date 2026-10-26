//! The details pane: the selected node under three tabs. Properties edits the
//! node as bingux-ui code, Layout draws its box, Computed lists every value
//! the application resolved.

use std::rc::Rc;

use freya::prelude::*;
use freya::winit::window::WindowId;

use super::box_model;
use super::props::PropertiesPane;
use super::snapshot::{Kind, Node, Snapshot};
use crate::controls::colour_picker::colour_chip;
use crate::controls::{property_row, search_field, segmented};
use crate::group::Group;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::Tokens;
use crate::typography::{CAPTION, CODE};

#[derive(Clone, Copy, PartialEq)]
enum DetailsTab {
    Properties,
    Layout,
    Computed,
}

impl DetailsTab {
    const ALL: [DetailsTab; 3] = [
        DetailsTab::Properties,
        DetailsTab::Layout,
        DetailsTab::Computed,
    ];

    fn title(self) -> &'static str {
        match self {
            DetailsTab::Properties => "Properties",
            DetailsTab::Layout => "Layout",
            DetailsTab::Computed => "Computed",
        }
    }

    /// The key in the segmented control: the title, which is unique.
    fn key(self) -> &'static str {
        self.title()
    }

    fn from_key(key: &str) -> Option<DetailsTab> {
        DetailsTab::ALL.into_iter().find(|tab| tab.key() == key)
    }
}

/// The pane for one selected node of the application window.
pub struct DetailsPane {
    pub snap: Rc<Snapshot>,
    pub id: u64,
    pub window: WindowId,
}

impl PartialEq for DetailsPane {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.snap, &other.snap) && self.id == other.id && self.window == other.window
    }
}

impl Component for DetailsPane {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let tab = use_state(|| DetailsTab::Properties);
        // The Computed tab's filter; held here so it survives tab switches.
        let filter = use_state(String::new);
        let selected = *tab.read();
        let class = super::app_density(self.window);
        let body: Element = match self.snap.node(self.id) {
            Some(node) => match selected {
                DetailsTab::Properties => PropertiesPane {
                    snap: self.snap.clone(),
                    id: self.id,
                    window: self.window,
                }
                .into(),
                DetailsTab::Layout => box_model::view(&tokens, self.window, self.id, node, class),
                DetailsTab::Computed => computed_tab(&tokens, node, filter),
            },
            None => rect()
                .tw(&tokens, &[w_full(), h_full(), p(S4)])
                .child(CAPTION.label(&tokens, "This element is gone"))
                .into(),
        };
        let bar = segmented(
            DetailsTab::ALL
                .iter()
                .map(|tab| (tab.key().to_string(), tab.title().to_string()))
                .collect(),
            selected.key(),
            move |picked: String| {
                if let Some(picked) = DetailsTab::from_key(&picked) {
                    let mut tab = tab;
                    tab.set(picked);
                }
            },
        )
        .tabs()
        .fill();
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col()])
            .child(
                rect()
                    .tw(&tokens, &[w_full(), px(S4), pt(S3), pb(S2)])
                    .child(bar),
            )
            .child(body)
    }
}

/// One line of the Computed tab: a name, and a value that may be a colour.
struct Line_ {
    name: &'static str,
    value: String,
    colour: Option<Color>,
}

fn line(name: &'static str, value: impl Into<String>) -> Line_ {
    Line_ {
        name,
        value: value.into(),
        colour: None,
    }
}

fn colour_line(name: &'static str, colour: Color) -> Line_ {
    Line_ {
        name,
        value: colour_to_hex(colour),
        colour: Some(colour),
    }
}

/// Every value the node resolved to, grouped as the Properties pane groups them.
fn computed_groups(node: &Node) -> Vec<(&'static str, Vec<Line_>)> {
    let mut element = vec![
        line("element", node.title()),
        line("kind", node.kind.name()),
        line("node id", node.id.to_string()),
        line(
            "parent",
            node.parent.map_or("none".to_string(), |id| id.to_string()),
        ),
        line("children", node.children.len().to_string()),
    ];
    if let Some(area) = node.area {
        element.push(line(
            "position",
            format!("{:.0}, {:.0}", area.min_x(), area.min_y()),
        ));
        element.push(line(
            "size",
            format!("{:.0} × {:.0}", area.width(), area.height()),
        ));
    }
    if let Some(text) = &node.text {
        element.push(line("text", text.clone()));
    }
    let mut groups = vec![("Element", element)];
    if let Some(props) = &node.props {
        let sides = |edges: &[f32; 4]| {
            format!(
                "{} {} {} {}",
                super::editor::number_text(edges[0]),
                super::editor::number_text(edges[1]),
                super::editor::number_text(edges[2]),
                super::editor::number_text(edges[3])
            )
        };
        groups.push((
            "Box",
            vec![
                line("width", props.width.clone()),
                line("height", props.height.clone()),
                line("min width", props.min_width.clone()),
                line("min height", props.min_height.clone()),
                line("max width", props.max_width.clone()),
                line("max height", props.max_height.clone()),
                line("padding", sides(&props.padding)),
                line("margin", sides(&props.margin)),
                line("gap", super::editor::number_text(props.gap)),
            ],
        ));
        groups.push((
            "Layout",
            vec![
                line("direction", props.direction),
                line("main axis", props.main_align),
                line("cross axis", props.cross_align),
                line("clip", props.clip.to_string()),
            ],
        ));
        if node.kind == Kind::Rect {
            let mut paint = Vec::new();
            match props.background {
                Some(colour) => paint.push(colour_line("background", colour)),
                None => paint.push(line("background", "gradient")),
            }
            paint.push(line(
                "corner radius",
                super::editor::number_text(props.corner_radius),
            ));
            paint.push(line("shadows", props.shadows.len().to_string()));
            paint.push(line("borders", props.borders.len().to_string()));
            groups.push(("Paint", paint));
        }
        let mut text = Vec::new();
        text.push(line(
            "font size",
            props
                .font_size
                .map_or("inherited".to_string(), super::editor::number_text),
        ));
        text.push(line(
            "font weight",
            props
                .font_weight
                .map_or("inherited".to_string(), |weight| format!("{weight:?}")),
        ));
        text.push(match props.text_color {
            Some(colour) => colour_line("colour", colour),
            None => line("colour", "inherited"),
        });
        text.push(line(
            "typeface",
            if props.mono { "monospace" } else { "default" },
        ));
        groups.push(("Text", text));
    }
    groups
}

/// The Computed tab: every resolved value, under a filter. The list scrolls.
fn computed_tab(tokens: &Tokens, node: &Node, filter: State<String>) -> Element {
    let query = filter.read().trim().to_lowercase();
    let mut column = rect().tw(tokens, &[w_full(), flex_col(), gap(S4), p(S4)]);
    for (title, lines) in computed_groups(node) {
        let rows: Vec<Element> = lines
            .into_iter()
            .filter(|found| {
                query.is_empty()
                    || found.name.to_lowercase().contains(&query)
                    || found.value.to_lowercase().contains(&query)
            })
            .map(|found| value_row(tokens, found))
            .collect();
        if !rows.is_empty() {
            column = column.child(Group::titled(title).rows(rows).unfiltered());
        }
    }
    let scroll = ScrollView::new()
        .drag_scrolling(false)
        .width(Size::fill())
        .height(Size::fill())
        .child(column);
    rect()
        .tw(tokens, &[w_full(), h_full(), flex_col(), gap(S1)])
        .child(
            rect()
                .tw(tokens, &[w_full(), px(S4), pt(S3)])
                .child(search_field(filter, |_: String| {}).dense()),
        )
        .child(scroll)
        .into()
}

/// A resolved value: its name at the left and the value, with a swatch for a
/// colour, at the right.
fn value_row(tokens: &Tokens, found: Line_) -> Element {
    let mut value = rect().tw(tokens, &[flex_row(), items(Align::Center), gap(S2)]);
    if let Some(colour) = found.colour {
        value = value.child(colour_chip(*tokens, colour, Dim::Swatch.px()));
    }
    let value = value.child(
        CODE.label(tokens, found.value)
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis),
    );
    property_row(found.name).control(value).into()
}
