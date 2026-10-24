//! The world map on the time zone page. Every zone is a dot at its city, the
//! chosen zone has a ring, a click picks the nearest zone within reach, and
//! hovering a dot names that city and shows its current local time.

use std::sync::LazyLock;

use bingux_ui::style::*;
use bingux_ui::surface::{over, use_tokens};
use bingux_ui::theme::Tokens;
use bytes::Bytes;
use freya::prelude::*;

use super::backend;
use super::{Ctx, pick_zone, zone_label};
use crate::system::background_then;
use crate::ui::{styled_label, typography};

const WORLD: &str = include_str!("../../../assets/timezone/world-map.svg");
const ZONES: &str = include_str!("../../../assets/timezone/timezones.json");
/// The map's viewBox is 360 by 145 units: longitude -180 to 180, latitude
/// 85 down to -60.
const ASPECT: f32 = 145.0 / 360.0;
/// The width used until the map has been measured.
const FALLBACK_WIDTH: f32 = 640.0;
/// A click further than this many pixels from every zone picks nothing.
const SNAP: f32 = 56.0;
/// The land colour in the bundled map, replaced by the palette's colour.
const LAND: &str = "#8a939e";
/// The gap between the pointer and the tooltip.
const TIP_GAP: f32 = 14.0;
/// The tooltip's size, estimated, to decide which side of the pointer it sits.
const TIP_WIDTH: f32 = 240.0;
const TIP_HEIGHT: f32 = 52.0;

struct Place {
    zone: String,
    lat: f32,
    lon: f32,
}

/// Every zone with its city's coordinates, read once from the bundled list.
static PLACES: LazyLock<Vec<Place>> = LazyLock::new(|| {
    let value: serde_json::Value =
        serde_json::from_str(ZONES).expect("the bundled timezones.json parses");
    value
        .as_array()
        .expect("the bundled timezones.json is a list")
        .iter()
        .map(|item| Place {
            zone: item["zone"]
                .as_str()
                .expect("each zone has a name")
                .to_string(),
            lat: item["lat"].as_f64().expect("each zone has a latitude") as f32,
            lon: item["lon"].as_f64().expect("each zone has a longitude") as f32,
        })
        .collect()
});

fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

/// The map's SVG for the current palette: an ocean behind the land, land, a
/// dot per zone, and a ring and dot on the current zone.
fn map_svg(tokens: &Tokens, current: &str) -> String {
    let land = hex(Paint::Outline.color(tokens));
    let ocean = hex(over(
        Paint::Accent.color(tokens).with_af32(0.07),
        Paint::Card.color(tokens),
    ));
    let accent = hex(Paint::Accent.color(tokens));
    let body = WORLD
        .trim_end()
        .strip_suffix("</svg>")
        .expect("the bundled world map is an SVG")
        .replace(LAND, &land);
    let root = body
        .find("<svg")
        .expect("the bundled world map has an <svg> root");
    let open = root
        + body[root..]
            .find('>')
            .expect("the bundled world map's root tag closes")
        + 1;
    let mut svg = String::with_capacity(body.len() + 4096);
    svg.push_str(&body[..open]);
    // Larger than any viewBox, so the ocean fills the whole card.
    svg.push_str(&format!(
        r#"<rect x="-1000" y="-1000" width="3000" height="3000" fill="{ocean}"/>"#
    ));
    svg.push_str(&body[open..]);
    svg.push_str(&format!(r#"<g fill="{accent}" fill-opacity="0.55">"#));
    for place in PLACES.iter() {
        svg.push_str(&format!(
            r#"<circle cx="{}" cy="{}" r="1.5"/>"#,
            place.lon, -place.lat
        ));
    }
    svg.push_str("</g>");
    if let Some(place) = PLACES.iter().find(|place| place.zone == current) {
        svg.push_str(&format!(
            r#"<circle cx="{x}" cy="{y}" r="5.2" fill="none" stroke="{accent}" stroke-width="1.2"/><circle cx="{x}" cy="{y}" r="2.8" fill="{accent}"/>"#,
            x = place.lon,
            y = -place.lat,
        ));
    }
    svg.push_str("</svg>");
    svg
}

/// The zone whose dot is nearest the pointer at (`x`, `y`) on a map of the
/// given size, if one is within `SNAP` pixels. The map wraps at the edges.
fn nearest(x: f32, y: f32, width: f32, height: f32) -> Option<&'static str> {
    let mut best: Option<(f32, &str)> = None;
    for place in PLACES.iter() {
        let px = (place.lon + 180.0) / 360.0 * width;
        let py = (85.0 - place.lat) / 145.0 * height;
        let dx = (px - x).abs();
        let dx = dx.min(width - dx);
        let distance = dx.hypot(py - y);
        if distance <= SNAP && best.is_none_or(|(near, _)| distance < near) {
            best = Some((distance, place.zone.as_str()));
        }
    }
    best.map(|(_, zone)| zone)
}

/// The zone under the pointer, and where the pointer is on the map.
#[derive(Clone, Copy, PartialEq)]
struct Hover {
    zone: &'static str,
    x: f32,
    y: f32,
}

#[derive(PartialEq)]
pub struct TimeZoneMap {
    pub current: String,
    ctx: Ctx,
}

impl TimeZoneMap {
    pub fn new(current: String, ctx: Ctx) -> Self {
        Self { current, ctx }
    }
}

impl Component for TimeZoneMap {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut size = use_state(Area::default);
        let measured = size.read().width();
        let width = if measured > 0.0 {
            measured
        } else {
            FALLBACK_WIDTH
        };
        let height = width * ASPECT;
        let ctx = self.ctx;
        let mut hover = use_state(|| None::<Hover>);
        // The local time of the zone the pointer last rested on, with its zone.
        let mut local_time = use_state(|| None::<(&'static str, String)>);

        let hovered = *hover.read();
        let time = hovered.and_then(|hovered| {
            local_time
                .read()
                .as_ref()
                .filter(|(zone, _)| *zone == hovered.zone)
                .map(|(_, time)| time.clone())
        });

        let mut card = rect()
            .width(Size::fill())
            .height(Size::px(height))
            .tw(
                &tokens,
                &[
                    rounded(Radius::Card),
                    bg(Paint::Card),
                    border(Line::Hairline),
                    clip(),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let left = matches!(
                    event.data(),
                    PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Left)
                );
                if !left {
                    return;
                }
                let at = event.element_location();
                if let Some(zone) = nearest(at.x as f32, at.y as f32, width, height) {
                    pick_zone(ctx, zone.to_string());
                }
            })
            .on_pointer_move(move |event: Event<PointerEventData>| {
                let at = event.element_location();
                let (x, y) = (at.x as f32, at.y as f32);
                let Some(zone) = nearest(x, y, width, height) else {
                    hover.set(None);
                    return;
                };
                let previous = hover.read().as_ref().map(|hovered| hovered.zone);
                hover.set(Some(Hover { zone, x, y }));
                if previous != Some(zone) {
                    let asked = zone.to_string();
                    background_then(
                        move || backend::date(Some(&asked), "+%H:%M"),
                        move |result| {
                            if let Ok(clock) = result {
                                local_time.set(Some((zone, clock)));
                            }
                        },
                    );
                }
            })
            .on_pointer_leave(move |_: Event<PointerEventData>| hover.set(None))
            .child(MapArt {
                current: self.current.clone(),
                height,
            });

        if let Some(hovered) = hovered {
            card = card.child(Tooltip {
                hover: hovered,
                time,
                width,
                height,
            });
        }
        card
    }
}

/// The map's picture. Its own component, so hovering the map does not redraw
/// the SVG.
#[derive(PartialEq)]
struct MapArt {
    current: String,
    height: f32,
}

impl Component for MapArt {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let svg = map_svg(&tokens, &self.current);
        SvgViewer::new((svg.clone(), Bytes::from(svg)))
            .width(Size::fill())
            .height(Size::px(self.height))
    }
}

/// The name and local time of the zone under the pointer, beside the pointer
/// and flipped to the other side near the map's edges.
#[derive(PartialEq)]
struct Tooltip {
    hover: Hover,
    time: Option<String>,
    width: f32,
    height: f32,
}

impl Component for Tooltip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let Hover { zone, x, y } = self.hover;
        let left = if x + TIP_GAP + TIP_WIDTH > self.width {
            x - TIP_GAP - TIP_WIDTH
        } else {
            x + TIP_GAP
        };
        let top = if y + TIP_GAP + TIP_HEIGHT > self.height {
            y - TIP_GAP - TIP_HEIGHT
        } else {
            y + TIP_GAP
        };
        let mut tip = rect()
            .position(
                Position::new_absolute()
                    .left(left.max(0.0))
                    .top(top.max(0.0)),
            )
            .tw(
                &tokens,
                &[
                    flex_col(),
                    rounded(Radius::Overlay),
                    bg(Paint::Popover),
                    border(Line::Hairline),
                    p(S2),
                ],
            )
            .child(styled_label(typography::BODY, zone_label(zone)));
        if let Some(time) = &self.time {
            tip = tip.child(styled_label(
                typography::CAPTION,
                format!("Local time {time}"),
            ));
        }
        tip
    }
}
