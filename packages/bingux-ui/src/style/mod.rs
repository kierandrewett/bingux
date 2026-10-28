//! The style system: tokens, utilities and recipes.
//!
//! A box is styled by data, as Tailwind classes style HTML. A utility is one
//! fact about a box (`px(S4)`, `gap(S2)`, `rounded(Radius::Card)`,
//! `bg(Paint::Card)`); a recipe is a named list of them in `recipes.rs`.
//! Both are applied with [`Tw::tw`]:
//!
//! ```ignore
//! rect().tw(&tokens, ROW).tw(&tokens, &[bg(Paint::Card)])
//! ```
//!
//! Later utilities override earlier ones; padding and margin merge side by
//! side, so `py(S3)` after `px(S4)` keeps the horizontal padding.
//!
//! The rules, which `scripts/ui-lint.py` enforces:
//!
//! - Outside this module nothing calls Freya's styling methods directly
//!   (`padding`, `spacing`, `margin`, `corner_radius`, `background`,
//!   `border`, `font_size`, `font_weight`, `color`): use a utility, a recipe
//!   or a [`crate::typography::TextStyle`].
//! - Application code (`bingux-settings`) uses recipes and components, and
//!   utilities only for layout that has no recipe yet.
//! - A size is a [`Dim`]. A raw `Size::px` is allowed only for a value
//!   computed at run time (a measured width, a drag offset, a chart).

pub mod recipes;
pub mod catalogue;
pub mod tokens;

use freya::prelude::*;

use crate::theme::Tokens;

pub use recipes::*;
pub use tokens::*;

/// Cross-axis or main-axis alignment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Start,
    Center,
    End,
    Between,
    Around,
}

impl From<Align> for Alignment {
    fn from(align: Align) -> Self {
        match align {
            Align::Start => Alignment::Start,
            Align::Center => Alignment::Center,
            Align::End => Alignment::End,
            Align::Between => Alignment::SpaceBetween,
            Align::Around => Alignment::SpaceAround,
        }
    }
}

/// How a box's size is set on one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    /// A token size.
    Dim(Dim),
    /// All the room the parent gives.
    Full,
    /// A share of the room left in a flex parent.
    Flex,
    /// As big as the content.
    Auto,
    /// The whole window, for an overlay layer.
    Screen,
}

impl Extent {
    fn size(self) -> Size {
        match self {
            Extent::Dim(dim) => Size::px(dim.px()),
            Extent::Full => Size::fill(),
            Extent::Flex => Size::flex(1.0),
            Extent::Auto => Size::auto(),
            Extent::Screen => Size::window_percent(100.0),
        }
    }
}

/// Which sides a space applies to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sides {
    All,
    X,
    Y,
    Top,
    Right,
    Bottom,
    Left,
}

/// The page sheet's leading-edge shadow (`Util::SheetEdgeShadow`): offset in
/// px, blur in px and black opacity at full amount.
const SHEET_EDGE_OFFSET: f32 = 24.0;
const SHEET_EDGE_BLUR: f32 = 32.0;
const SHEET_EDGE_OPACITY: f32 = 0.5;
/// The shadow of a lifted screen (`Util::LiftShadow`): offset down in px,
/// blur in px and black opacity at full amount.
const LIFT_OFFSET: f32 = 12.0;
const LIFT_BLUR: f32 = 32.0;
const LIFT_OPACITY: f32 = 0.35;


/// One utility: a single fact about a box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Util {
    Pad(Sides, Space),
    Margin(Sides, Space),
    Gap(Space),
    Rounded(Radius),
    /// `Radius` plus a run-time number of pixels.
    RoundedBy(Radius, f32),
    /// `Radius` times a run-time amount from 0 to 1, for a box whose corners
    /// animate in and out.
    RoundedScaled(Radius, f32),
    /// A top margin of minus `space` times the amount, for a box that pulls
    /// back the gap its column puts above it while it grows in.
    PullUp(Space, f32),
    Bg(Paint),
    /// A run-time colour fill, for a colour chosen while the app runs (a
    /// picker's value, a swatch). Fixed colours are `Bg(Paint::..)`.
    Fill(Color),
    /// The text colour children inherit.
    Ink(Paint),
    /// The type children inherit: size, weight and colour of a text style.
    Font(crate::typography::TextStyle),
    /// An interactive surface: the base paint with the selection, then the
    /// press or hover layer composed over it.
    Surface {
        base: Paint,
        hover: bool,
        press: bool,
        selected: bool,
    },
    Border(Line),
    Shadow(Elevation),
    /// A soft shadow cast to the left by a page sheet that slides across
    /// another page, scaled by a run-time amount from 0 to 1 so it vanishes
    /// at rest. Stronger and tighter than `Elevation::Floating`, so the
    /// edge reads while the sheet moves.
    SheetEdgeShadow(f32),
    /// A soft shadow all round that lifts a screen off the one beneath it
    /// while it is dragged away (predictive back), scaled by a run-time
    /// amount from 0 to 1 so it vanishes at rest.
    LiftShadow(f32),
    /// `Radius` times a run-time amount on the left corners only, for a card
    /// that slides in from the right.
    RoundedLeftScaled(Radius, f32),
    /// `Radius` plus a run-time number of pixels on some corners only.
    RoundedOn(Radius, Corners, f32),
    W(Extent),
    H(Extent),
    MinW(Dim),
    MinH(Dim),
    MaxW(Dim),
    MaxH(Dim),
    /// Children side by side.
    Row,
    /// Children stacked.
    Col,
    /// Children share leftover room by `Extent::Flex`.
    Flex,
    /// Children wrap onto new lines, `Space` apart.
    Wrap(Space),
    Items(Align),
    Justify(Align),
    /// Centre children on both axes.
    Center,
    /// Clip children to the box and its radius.
    Clip,
}

pub const fn p(space: Space) -> Util {
    Util::Pad(Sides::All, space)
}
pub const fn px(space: Space) -> Util {
    Util::Pad(Sides::X, space)
}
pub const fn py(space: Space) -> Util {
    Util::Pad(Sides::Y, space)
}
pub const fn pt(space: Space) -> Util {
    Util::Pad(Sides::Top, space)
}
pub const fn pr(space: Space) -> Util {
    Util::Pad(Sides::Right, space)
}
pub const fn pb(space: Space) -> Util {
    Util::Pad(Sides::Bottom, space)
}
pub const fn pl(space: Space) -> Util {
    Util::Pad(Sides::Left, space)
}
pub const fn m(space: Space) -> Util {
    Util::Margin(Sides::All, space)
}
pub const fn mx(space: Space) -> Util {
    Util::Margin(Sides::X, space)
}
pub const fn my(space: Space) -> Util {
    Util::Margin(Sides::Y, space)
}
pub const fn mt(space: Space) -> Util {
    Util::Margin(Sides::Top, space)
}
pub const fn mb(space: Space) -> Util {
    Util::Margin(Sides::Bottom, space)
}
pub const fn ml(space: Space) -> Util {
    Util::Margin(Sides::Left, space)
}
pub const fn mr(space: Space) -> Util {
    Util::Margin(Sides::Right, space)
}
pub const fn gap(space: Space) -> Util {
    Util::Gap(space)
}
pub const fn rounded(radius: Radius) -> Util {
    Util::Rounded(radius)
}
/// `radius` plus `extra` pixels, for a box whose outer edge sits `extra` px
/// outside a control, such as a focus ring. `extra` may change at run time.
pub fn rounded_by(radius: Radius, extra: f32) -> Util {
    Util::RoundedBy(radius, extra)
}
/// A top margin of minus `space` times `amount` (0 to 1): the box takes back
/// part of the gap above it. `amount` may change at run time.
pub fn pull_up(space: Space, amount: f32) -> Util {
    Util::PullUp(space, amount)
}
/// `radius` scaled by `amount` (0 to 1), for a box that animates its corners
/// in and out. `amount` may change at run time.
pub fn rounded_scaled(radius: Radius, amount: f32) -> Util {
    Util::RoundedScaled(radius, amount)
}
pub const fn bg(paint: Paint) -> Util {
    Util::Bg(paint)
}
/// A run-time colour fill; see `Util::Fill`.
pub const fn fill(colour: Color) -> Util {
    Util::Fill(colour)
}
/// A gradient background for a box whose stops are run-time colours, such as
/// a colour picker's strips. Gradients are not tokens, so this is the one
/// place the style system paints one.
pub fn gradient_fill(rect: Rect, gradient: LinearGradient) -> Rect {
    rect.background(gradient)
}
/// The text colour that text inside the box inherits.
pub const fn ink(paint: Paint) -> Util {
    Util::Ink(paint)
}
/// The text style that text inside the box inherits, for a component that
/// draws its own text (a Freya input).
pub const fn font(style: crate::typography::TextStyle) -> Util {
    Util::Font(style)
}
pub const fn border(line: Line) -> Util {
    Util::Border(line)
}
pub const fn shadow(elevation: Elevation) -> Util {
    Util::Shadow(elevation)
}
pub fn sheet_edge_shadow(amount: f32) -> Util {
    Util::SheetEdgeShadow(amount)
}
/// A soft shadow all round, scaled by `amount` (0 to 1): the screen that
/// predictive back lifts and shrinks. See `Util::LiftShadow`.
pub fn lift_shadow(amount: f32) -> Util {
    Util::LiftShadow(amount)
}
/// `radius` scaled by `amount` (0 to 1) on the left corners only; see
/// `Util::RoundedLeftScaled`.
pub fn rounded_left_scaled(radius: Radius, amount: f32) -> Util {
    Util::RoundedLeftScaled(radius, amount)
}
/// The corners of a box that take a radius, for a box that sits flush against
/// a neighbour (a segment of a split button): the outer corners are rounded
/// and the corners at the seam are square. `None` is a square box, such as a
/// flat text field's line, whose focus ring is square too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corners {
    All,
    Left,
    Right,
    None,
}
/// `radius` plus `extra` pixels on `corners` only, square elsewhere; see
/// [`Corners`].
pub fn rounded_on(radius: Radius, corners: Corners, extra: f32) -> Util {
    Util::RoundedOn(radius, corners, extra)
}
pub const fn surface(base: Paint, hover: bool, press: bool, selected: bool) -> Util {
    Util::Surface {
        base,
        hover,
        press,
        selected,
    }
}
pub const fn w(dim: Dim) -> Util {
    Util::W(Extent::Dim(dim))
}
pub const fn h(dim: Dim) -> Util {
    Util::H(Extent::Dim(dim))
}
pub const fn w_full() -> Util {
    Util::W(Extent::Full)
}
pub const fn h_full() -> Util {
    Util::H(Extent::Full)
}
pub const fn w_flex() -> Util {
    Util::W(Extent::Flex)
}
pub const fn h_flex() -> Util {
    Util::H(Extent::Flex)
}
/// As wide as the window, for an overlay layer (`w-screen`).
pub const fn w_screen() -> Util {
    Util::W(Extent::Screen)
}
/// As tall as the window, for an overlay layer (`h-screen`).
pub const fn h_screen() -> Util {
    Util::H(Extent::Screen)
}
pub const fn w_auto() -> Util {
    Util::W(Extent::Auto)
}
pub const fn h_auto() -> Util {
    Util::H(Extent::Auto)
}
pub const fn min_w(dim: Dim) -> Util {
    Util::MinW(dim)
}
pub const fn min_h(dim: Dim) -> Util {
    Util::MinH(dim)
}
pub const fn max_w(dim: Dim) -> Util {
    Util::MaxW(dim)
}
pub const fn max_h(dim: Dim) -> Util {
    Util::MaxH(dim)
}
pub const fn flex_row() -> Util {
    Util::Row
}
pub const fn flex_col() -> Util {
    Util::Col
}
/// Children sized `w_flex()` or `h_flex()` share the room that is left.
pub const fn grow_children() -> Util {
    Util::Flex
}
pub const fn wrap(space: Space) -> Util {
    Util::Wrap(space)
}
pub const fn items(align: Align) -> Util {
    Util::Items(align)
}
pub const fn justify(align: Align) -> Util {
    Util::Justify(align)
}
pub const fn center() -> Util {
    Util::Center
}
pub const fn clip() -> Util {
    Util::Clip
}

/// A horizontally scrolling region, for a row too wide for its space, such
/// as a breadcrumb. Its children flow left to right.
pub fn scroll_x() -> ScrollView {
    ScrollView::new().direction(Direction::Horizontal)
}

/// [`scroll_x`] driven by `scroll`, for a row whose owner scrolls it to a
/// place (a tab strip bringing the selected tab into view).
pub fn scroll_x_controlled(scroll: ScrollController) -> ScrollView {
    ScrollView::new_controlled(scroll).direction(Direction::Horizontal)
}

/// Applies utilities and recipes to a box.
pub trait Tw: Sized {
    /// Apply `utils` in order; later ones override earlier ones.
    fn tw(self, tokens: &Tokens, utils: &[Util]) -> Self;

    /// Apply layout utilities, for a helper that has no tokens. A paint,
    /// surface or border utility in `utils` is a bug: it panics in debug
    /// builds and is skipped in release builds.
    fn tw_layout(self, utils: &[Util]) -> Self;
}

impl Tw for Rect {
    fn tw(mut self, tokens: &Tokens, utils: &[Util]) -> Self {
        for util in utils {
            self = apply(self, Some(tokens), *util);
        }
        self
    }

    fn tw_layout(mut self, utils: &[Util]) -> Self {
        for util in utils {
            self = apply(self, None, *util);
        }
        self
    }
}

/// Sets the sides of `gaps` that `sides` names to `value`.
fn set_sides(gaps: &mut [f32; 4], sides: Sides, value: f32) {
    let [top, right, bottom, left] = gaps;
    match sides {
        Sides::All => {
            *top = value;
            *right = value;
            *bottom = value;
            *left = value;
        }
        Sides::X => {
            *right = value;
            *left = value;
        }
        Sides::Y => {
            *top = value;
            *bottom = value;
        }
        Sides::Top => *top = value,
        Sides::Right => *right = value,
        Sides::Bottom => *bottom = value,
        Sides::Left => *left = value,
    }
}

fn sides_of(gaps: &Gaps) -> [f32; 4] {
    [gaps.top(), gaps.right(), gaps.bottom(), gaps.left()]
}

/// The shadow an elevation draws. The inspector compares it with a box's own
/// shadow to name the elevation.
pub(crate) fn elevation_shadow(elevation: Elevation) -> Shadow {
    let (y, blur, opacity) = elevation.params();
    Shadow::new()
        .position(ShadowPosition::Normal)
        .y(y)
        .blur(blur)
        .color(Color::BLACK.with_af32(opacity))
}

pub(crate) fn line_border(tokens: &Tokens, line: Line) -> Border {
    let one = |width: BorderWidth, color: Color| {
        Border::new()
            .width(width)
            .alignment(BorderAlignment::Inner)
            .fill(color)
    };
    let side = |top: f32, right: f32, bottom: f32, left: f32| BorderWidth {
        top,
        right,
        bottom,
        left,
    };
    let hair = Dim::Hairline.px();
    let ring = Dim::Ring.px();
    match line {
        Line::Hairline => one(side(hair, hair, hair, hair), tokens.separator),
        Line::HairlineTop => one(side(hair, 0.0, 0.0, 0.0), tokens.separator),
        Line::HairlineRight => one(side(0.0, hair, 0.0, 0.0), tokens.separator),
        Line::HairlineBottom => one(side(0.0, 0.0, hair, 0.0), tokens.separator),
        Line::Outline => one(side(hair, hair, hair, hair), tokens.outline),
        Line::Edge(paint) => one(side(hair, hair, hair, hair), paint.color(tokens)),
        Line::Ring(paint) => one(side(ring, ring, ring, ring), paint.color(tokens)),
        Line::Stroke(width, paint) => {
            let width = width.px();
            one(side(width, width, width, width), paint.color(tokens))
        }
    }
}

fn apply(rect: Rect, tokens: Option<&Tokens>, util: Util) -> Rect {
    let painted = matches!(
        util,
        Util::Bg(_)
            | Util::Fill(_)
            | Util::Ink(_)
            | Util::Font(_)
            | Util::Surface { .. }
            | Util::Border(_)
    );
    let tokens = match tokens {
        Some(tokens) => tokens,
        None if painted => {
            debug_assert!(false, "tw_layout was given a paint utility: {util:?}");
            return rect;
        }
        None => return apply_layout(rect, util),
    };
    if !painted {
        return apply_layout(rect, util);
    }
    match util {
        Util::Bg(paint) => rect.background(paint.color(tokens)),
        Util::Fill(colour) => rect.background(colour),
        Util::Ink(paint) => rect.color(paint.color(tokens)),
        Util::Font(style) => rect
            .font_size(style.size.px())
            .font_weight(style.weight)
            .color(style.ink.color(tokens)),
        Util::Surface {
            base,
            hover,
            press,
            selected,
        } => rect.background(crate::surface::surface_color(
            tokens,
            base.color(tokens),
            hover,
            press,
            selected,
        )),
        Util::Border(line) => rect.border(line_border(tokens, line)),
        _ => rect,
    }
}

fn apply_layout(rect: Rect, util: Util) -> Rect {
    match util {
        Util::Pad(sides, space) => {
            let mut rect = rect;
            let mut gaps = sides_of(&rect.get_layout().layout.padding);
            set_sides(&mut gaps, sides, space.px());
            let [top, right, bottom, left] = gaps;
            rect.padding((top, right, bottom, left))
        }
        Util::Margin(sides, space) => {
            let mut rect = rect;
            let mut gaps = sides_of(&rect.get_layout().layout.margin);
            set_sides(&mut gaps, sides, space.px());
            let [top, right, bottom, left] = gaps;
            rect.margin((top, right, bottom, left))
        }
        Util::Gap(space) => rect.spacing(space.px()),
        Util::Rounded(radius) => rect.corner_radius(radius.px()),
        Util::RoundedBy(radius, extra) => rect.corner_radius(radius.px() + extra),
        Util::RoundedScaled(radius, amount) => rect.corner_radius(radius.px() * amount),
        Util::PullUp(space, amount) => {
            let mut rect = rect;
            let mut gaps = sides_of(&rect.get_layout().layout.margin);
            set_sides(&mut gaps, Sides::Top, -(space.px() * amount));
            let [top, right, bottom, left] = gaps;
            rect.margin((top, right, bottom, left))
        }
        Util::RoundedLeftScaled(radius, amount) => {
            let r = radius.px() * amount;
            rect.corner_radius((r, 0.0, 0.0, r))
        }
        Util::RoundedOn(radius, corners, extra) => {
            let r = radius.px() + extra;
            match corners {
                Corners::All => rect.corner_radius(r),
                Corners::Left => rect.corner_radius((r, 0.0, 0.0, r)),
                Corners::Right => rect.corner_radius((0.0, r, r, 0.0)),
                Corners::None => rect.corner_radius(0.0),
            }
        }
        Util::Bg(_)
        | Util::Fill(_)
        | Util::Ink(_)
        | Util::Font(_)
        | Util::Surface { .. }
        | Util::Border(_) => rect,
        Util::Shadow(elevation) => rect.shadow(elevation_shadow(elevation)),
        Util::SheetEdgeShadow(amount) => rect.shadow(
            Shadow::new()
                .position(ShadowPosition::Normal)
                .x(-SHEET_EDGE_OFFSET)
                .y(0.0)
                .blur(SHEET_EDGE_BLUR)
                .color(Color::BLACK.with_af32(SHEET_EDGE_OPACITY * amount)),
        ),
        Util::LiftShadow(amount) => rect.shadow(
            Shadow::new()
                .position(ShadowPosition::Normal)
                .x(0.0)
                .y(LIFT_OFFSET)
                .blur(LIFT_BLUR)
                .color(Color::BLACK.with_af32(LIFT_OPACITY * amount)),
        ),
        Util::W(extent) => rect.width(extent.size()),
        Util::H(extent) => rect.height(extent.size()),
        Util::MinW(dim) => rect.min_width(Size::px(dim.px())),
        Util::MinH(dim) => rect.min_height(Size::px(dim.px())),
        Util::MaxW(dim) => rect.max_width(Size::px(dim.px())),
        Util::MaxH(dim) => rect.max_height(Size::px(dim.px())),
        Util::Row => rect.direction(Direction::Horizontal),
        Util::Col => rect.direction(Direction::Vertical),
        Util::Flex => rect.content(Content::Flex),
        Util::Wrap(space) => rect.content(Content::wrap_spacing(space.px())),
        Util::Items(align) => rect.cross_align(Alignment::from(align)),
        Util::Justify(align) => rect.main_align(Alignment::from(align)),
        Util::Center => rect.center(),
        Util::Clip => rect.overflow(Overflow::Clip),
    }
}
