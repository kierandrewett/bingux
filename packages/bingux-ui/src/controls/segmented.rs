//! The shell's segmented control (`SegmentedControl.qml`), also used as the
//! tab rail of a popup and an app's tab bar (`tabs`). Only the selected segment is in the tab order. Left
//! and Right arrows move the selection and focus to the neighbouring segment,
//! and Home and End go to the first and last.
//!
//! Only the selected segment is raised. An unselected segment has no fill of
//! its own, even on hover: its text brightens instead, so a hovered segment
//! never looks like a second selection.

use super::press::use_press;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};
use crate::typography::{BODY, Ink};

/// The shell's segmented control: a quiet `control` rail where only the
/// selected segment is raised. `tabs` makes the same rail an app's tab bar:
/// its segments are tabs, it never becomes a dropdown, and `scroll` lets a
/// narrow container scroll it sideways rather than squeeze the segments.
#[derive(PartialEq)]
pub struct Segmented {
    options: Vec<(String, String)>,
    selected: String,
    on_select: EventHandler<String>,
    fill: bool,
    tabs: bool,
    scroll: bool,
}

pub fn segmented(
    options: Vec<(String, String)>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> Segmented {
    Segmented {
        options,
        selected: selected.to_string(),
        on_select: on_select.into(),
        fill: false,
        tabs: false,
        scroll: false,
    }
}

impl Segmented {
    /// Span the full width with equal segments, as a popup's tab rail does.
    /// A filled control stays a rail on a phone instead of turning into a
    /// dropdown, because it switches pages rather than picking a value.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    /// Use the rail as a tab bar. Tabs switch views, so like a filled rail
    /// they stay a rail on a phone.
    pub fn tabs(mut self) -> Self {
        self.tabs = true;
        self
    }

    /// Scroll the rail sideways when its container is too narrow for it.
    pub fn scroll(mut self) -> Self {
        self.scroll = true;
        self
    }
}

impl Component for Segmented {
    fn render(&self) -> impl IntoElement {
        // On a phone a row has no room for a track of segments beside its
        // title, so the same choice is a value picker, as the dropdown is.
        // The density is read here, so a change swaps the child below.
        if is_compact() && !self.fill && !self.tabs {
            return crate::controls::dropdown::dropdown(
                self.options.clone(),
                self.selected.clone(),
                self.on_select.clone(),
            )
            .into_element();
        }
        if self.scroll {
            return ScrollingRail {
                options: self.options.clone(),
                selected: self.selected.clone(),
                on_select: self.on_select.clone(),
            }
            .into_element();
        }
        SegmentRail {
            options: self.options.clone(),
            selected: self.selected.clone(),
            on_select: self.on_select.clone(),
            fill: self.fill,
            measure: None,
        }
        .into_element()
    }
}

/// A rail that scrolls sideways when its container is too narrow for it, and
/// brings the selected segment into view whenever the selection moves (a
/// keyboard shortcut, an arrow key) or the rail is first measured, so the
/// selected tab is never the one scrolled out of sight. A scrolling rail is its
/// own component so its hooks are not skipped by the compact branch above.
#[derive(PartialEq)]
struct ScrollingRail {
    options: Vec<(String, String)>,
    selected: String,
    on_select: EventHandler<String>,
}

impl Component for ScrollingRail {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let scroll = use_scroll_controller(ScrollConfig::default);
        let mut viewport = use_state(|| 0.0_f32);
        let widths = use_state(Vec::<f32>::new);
        let selected_index = self.options.iter().position(|(key, _)| *key == self.selected);
        // The rail pads its segments and sets them apart by `S1`.
        let step = S1.px();
        // Where the rail is scrolled to, read here so the effect below does not
        // subscribe to every scroll and pull a rail the person scrolled back.
        let offset = use_hook(|| Rc::new(Cell::new(0.0_f32)));
        offset.set(-(<(i32, i32)>::from(scroll).0 as f32));
        let deps = (
            selected_index,
            widths.read().iter().map(|width| width.round() as i32).collect::<Vec<_>>(),
            viewport().round() as i32,
            step.round() as i32,
        );
        use_side_effect_with_deps(&deps, move |(index, widths, region, step)| {
            let Some(index) = *index else { return };
            if widths.len() <= index || widths.iter().any(|width| *width <= 0) {
                return;
            }
            let widths: Vec<f32> = widths.iter().map(|width| *width as f32).collect();
            let mut scroll = scroll;
            let current = offset.get();
            let to = reveal(&widths, index, *step as f32, *region as f32, current);
            if (to - current).abs() >= 1.0 {
                scroll.scroll_to_x(-(to.round() as i32));
            }
        });
        let rail = SegmentRail {
            options: self.options.clone(),
            selected: self.selected.clone(),
            on_select: self.on_select.clone(),
            fill: false,
            measure: Some(widths),
        };
        rect()
            .tw(&tokens, &[w_full(), h(Dim::Control)])
            .on_sized(move |event: Event<SizedEventData>| {
                viewport.set_if_modified(event.area.width());
            })
            .child(
                scroll_x_controlled(scroll)
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(rail),
            )
    }
}

/// The scroll offset, from the rail's start, that shows segment `index` of a
/// rail whose segments are `widths` wide, with `step` of padding at its ends
/// and between segments, in a region `region` wide, moving no further than it
/// must. `current` is where the region starts now. One `step` of the rail
/// beside the segment is shown too, so its neighbour is in sight.
fn reveal(widths: &[f32], index: usize, step: f32, region: f32, current: f32) -> f32 {
    let content = 2.0 * step + widths.iter().sum::<f32>() + step * widths.len().saturating_sub(1) as f32;
    let left = step + widths[..index].iter().sum::<f32>() + step * index as f32;
    let (near, far) = (left - step, left + widths[index] + step);
    let to = if near < current {
        near
    } else if far > current + region {
        far - region
    } else {
        current
    };
    to.clamp(0.0, (content - region).max(0.0))
}

impl Component for SegmentRail {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fitted = crate::rows::use_row_fitted();
        let enabled = use_enabled(true);
        let options = self.options.clone();
        let selected = self.selected.clone();
        let on_select = self.on_select.clone();
        // One accessibility id per key, kept for the life of the control, so
        // the arrow keys can move focus to a neighbouring segment.
        let ids = use_hook(|| Rc::new(RefCell::new(HashMap::<String, AccessibilityId>::new())));
        let ids: Vec<AccessibilityId> = {
            let mut map = ids.borrow_mut();
            self.options
                .iter()
                .map(|(key, _)| {
                    *map.entry(key.clone())
                        .or_insert_with(AccessibilityId::new_unique)
                })
                .collect()
        };
        let key_ids = ids.clone();
        Element::from(
            rect()
                .tw(
                    &tokens,
                    &[
                        h(if fitted { Dim::RowButton } else { Dim::Control }),
                        flex_row(),
                        items(Align::Center),
                        p(S1),
                        gap(S1),
                        rounded(if fitted { Radius::Inner } else { Radius::Control }),
                        bg(Paint::Control),
                    ],
                )
                .maybe(self.fill, |element| {
                    element.tw(&tokens, &[w_full(), grow_children()])
                })
                .maybe(!enabled, |element| element.opacity(DISABLED_OPACITY))
                .on_key_down(move |event: Event<KeyboardEventData>| {
                    if !enabled {
                        return;
                    }
                    let Some(index) = options.iter().position(|(key, _)| *key == selected) else {
                        return;
                    };
                    let last = options.len().saturating_sub(1);
                    let next = match &event.key {
                        Key::Named(NamedKey::ArrowLeft) => index.saturating_sub(1),
                        Key::Named(NamedKey::ArrowRight) => (index + 1).min(last),
                        Key::Named(NamedKey::Home) => 0,
                        Key::Named(NamedKey::End) => last,
                        _ => return,
                    };
                    event.stop_propagation();
                    if next != index {
                        on_select.call(options[next].0.clone());
                    }
                    key_ids[next].request_focus();
                })
                .children(self.options.iter().enumerate().map(|(index, (key, text))| {
                    Element::from(Segment {
                        key: key.clone(),
                        text: text.clone(),
                        selected: *key == self.selected,
                        fill: self.fill,
                        on_select: self.on_select.clone(),
                        a11y_id: ids[index],
                        measure: self.measure.map(|widths| (index, widths)),
                    })
                })),
        )
    }
}

/// The segment rail. [`Segmented`] chooses between it and the dropdown, so a
/// density change swaps one child for the other and no component changes its
/// hooks while it is mounted.
#[derive(PartialEq)]
struct SegmentRail {
    options: Vec<(String, String)>,
    selected: String,
    on_select: EventHandler<String>,
    fill: bool,
    /// Collects each segment's width; `Some` for a rail that scrolls.
    measure: Option<State<Vec<f32>>>,
}

#[derive(PartialEq)]
struct Segment {
    key: String,
    text: String,
    selected: bool,
    /// Share the rail's width equally with the other segments.
    fill: bool,
    on_select: EventHandler<String>,
    a11y_id: AccessibilityId,
    /// Where to report this segment's width, by its place in the rail, for a
    /// rail that scrolls and must know where each segment lies.
    measure: Option<(usize, State<Vec<f32>>)>,
}

impl Component for Segment {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        // Inside a rail that fills a row, the rail's radius less its padding.
        let fitted = crate::rows::use_row_fitted();
        // The face and its ring share one radius, so the ring follows the fill.
        let radius = if fitted { Radius::Mark } else { Radius::Inner };
        let mut hovering = use_state(|| false);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(true);
        let key = self.key.clone();
        let on_select = self.on_select.clone();
        let press = use_press();
        rect()
            .component("Segment")
            .a11y_id(a11y_id)
            .a11y_focusable(enabled && self.selected)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(|builder| builder.set_selected(self.selected))
            .tw(
                &tokens,
                &[
                    h_full(),
                    px(S2),
                    center(),
                    rounded(radius),
                    bg(if self.selected {
                        Paint::Raised
                    } else {
                        Paint::Transparent
                    }),
                ],
            )
            .maybe(self.fill, |element| element.tw(&tokens, &[w_flex()]))
            .maybe(self.measure.is_some(), |element| {
                let Some((index, widths)) = self.measure else {
                    return element;
                };
                element.on_sized(move |event: Event<SizedEventData>| {
                    let width = event.area.width();
                    let known = widths.peek().get(index).copied();
                    if known.is_none_or(|known| (known - width).abs() > 0.5) {
                        let mut widths = widths;
                        let mut widths = widths.write();
                        if widths.len() <= index {
                            widths.resize(index + 1, 0.0);
                        }
                        widths[index] = width;
                    }
                })
            })
            .maybe(self.selected, |element| {
                element.tw(&tokens, &[shadow(Elevation::Knob)])
            })
            .maybe(enabled, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| hovering.set(false))
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        if !press.tap(&event) {
                            return;
                        }
                        on_select.call(key.clone());
                        a11y_id.request_focus();
                    })
            })
            .child(
                // The selected segment is raised; a hovered one only brightens
                // its text, so it never reads as a second selection.
                BODY.in_ink(if self.selected || hovering() {
                    Ink::Text
                } else {
                    Ink::Muted
                })
                .label(&tokens, self.text.clone())
                .max_lines(1),
            ).focusable(focus())
            .maybe(self.fill, |wrapper| wrapper.tw(&tokens, &[w_flex()]))
            .tw(&tokens, &[h_full()])
    }
}

#[cfg(test)]
mod reveal_tests {
    use super::reveal;

    const WIDTHS: [f32; 4] = [60.0, 100.0, 70.0, 80.0];
    // Padding 4: the segments start at 4, 68, 172 and 246, and end at 64, 168, 242 and 326.

    #[test]
    fn a_segment_in_view_leaves_the_rail_where_it_is() {
        assert_eq!(reveal(&WIDTHS, 1, 4.0, 200.0, 0.0), 0.0);
    }

    #[test]
    fn a_segment_past_the_end_scrolls_in_with_a_step_of_rail_beside_it() {
        // The last segment ends at 326; a step of rail beyond it is 330, the rail's end.
        assert_eq!(reveal(&WIDTHS, 3, 4.0, 200.0, 0.0), 130.0);
        // The third ends at 242, and 246 shows the step beside it.
        assert_eq!(reveal(&WIDTHS, 2, 4.0, 200.0, 0.0), 46.0);
    }

    #[test]
    fn a_segment_before_the_start_scrolls_back_to_it() {
        assert_eq!(reveal(&WIDTHS, 0, 4.0, 200.0, 130.0), 0.0);
        assert_eq!(reveal(&WIDTHS, 1, 4.0, 200.0, 130.0), 64.0);
    }

    #[test]
    fn the_offset_never_leaves_the_rail() {
        // A region wider than the rail has nothing to scroll.
        assert_eq!(reveal(&WIDTHS, 3, 4.0, 400.0, 0.0), 0.0);
        assert_eq!(reveal(&WIDTHS, 3, 4.0, 200.0, 500.0), 130.0);
    }
}

#[cfg(all(test, feature = "testing"))]
mod scroll_tests {
    use std::time::Duration;

    use super::*;
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    const TABS: [&str; 4] = ["Basic", "Permissions", "Details", "Contents"];

    #[derive(PartialEq)]
    struct Root(usize);

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _ = provide_tokens();
            WindowHost::new(Host(self.0))
        }
    }

    #[derive(PartialEq)]
    struct Host(usize);

    impl Component for Host {
        fn render(&self) -> impl IntoElement {
            let options = TABS.iter().map(|tab| (tab.to_string(), tab.to_string())).collect();
            segmented(options, TABS[self.0], |_: String| {})
                .tabs()
                .scroll()
        }
    }

    /// The horizontal extent of the label `text`, once the rail has settled in
    /// a window `width` wide with `selected` chosen.
    fn label_x(text: &str, selected: usize, width: f32) -> (f32, f32) {
        let mut shot = Snapshot::new(move || Root(selected).into_element(), (width, 120.0));
        shot.settle(Duration::from_millis(600));
        let text = text.to_owned();
        let area = shot
            .runner()
            .find_many(move |node, element| {
                let label = Label::try_downcast(element)?;
                (label.text == text).then(|| node.layout().area)
            })
            .into_iter()
            .next()
            .expect("the segment's label");
        (area.min_x(), area.max_x())
    }

    #[test]
    fn a_scrolling_rail_shows_the_selected_segment_however_far_along_it_is() {
        // 200 px holds neither the four labels nor their padding.
        for (index, tab) in TABS.iter().enumerate() {
            let (min_x, max_x) = label_x(tab, index, 200.0);
            assert!(
                min_x >= 0.0 && max_x <= 200.0,
                "the selected segment {tab} is scrolled out of sight: it spans {min_x} to {max_x} in a 200 px window"
            );
        }
    }
}
