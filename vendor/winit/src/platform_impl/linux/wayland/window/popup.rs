//! Placing an `xdg_popup`: the positioner a popup is created and moved with.
//!
//! Bingux patch, see [`Role`](super::role::Role).

use sctk::error::GlobalError;
use sctk::globals::ProvidesBoundGlobal;
use sctk::reexports::client::Proxy;
use sctk::reexports::protocols::xdg::shell::client::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity,
};
use sctk::reexports::protocols::xdg::shell::client::xdg_wm_base::XdgWmBase;
use sctk::shell::xdg::{XdgPositioner, XdgShell};

use crate::platform_impl::{PopupAdjust, PopupAttributes, PopupEdge};

/// The bound `xdg_wm_base`, for making positioners after the popup exists.
#[derive(Debug, Clone)]
pub struct WmBase(pub XdgWmBase);

impl ProvidesBoundGlobal<XdgWmBase, { XdgShell::API_VERSION_MAX }> for WmBase {
    fn bound_global(&self) -> Result<XdgWmBase, GlobalError> {
        Ok(self.0.clone())
    }
}

/// What a popup was asked for.
#[derive(Debug, Clone)]
pub struct PopupPlacement {
    pub attributes: PopupAttributes,
}

impl PopupPlacement {
    /// A positioner for a popup of `size` (logical pixels).
    pub fn positioner(
        &self,
        wm_base: &WmBase,
        size: (u32, u32),
    ) -> Result<XdgPositioner, GlobalError> {
        let attributes = &self.attributes;
        let positioner = XdgPositioner::new(wm_base)?;
        positioner.set_size(size.0.max(1) as i32, size.1.max(1) as i32);
        let (x, y, width, height) = attributes.anchor_rect;
        positioner.set_anchor_rect(x, y, width.max(1), height.max(1));
        positioner.set_anchor(anchor(attributes.anchor));
        positioner.set_gravity(gravity(attributes.gravity));
        positioner.set_offset(attributes.offset.0, attributes.offset.1);
        positioner.set_constraint_adjustment(adjustment(attributes.adjust));
        if attributes.reactive && positioner.version() >= 3 {
            positioner.set_reactive();
        }
        Ok(positioner)
    }
}

fn anchor(edge: PopupEdge) -> Anchor {
    match edge {
        PopupEdge::None => Anchor::None,
        PopupEdge::Top => Anchor::Top,
        PopupEdge::Bottom => Anchor::Bottom,
        PopupEdge::Left => Anchor::Left,
        PopupEdge::Right => Anchor::Right,
        PopupEdge::TopLeft => Anchor::TopLeft,
        PopupEdge::BottomLeft => Anchor::BottomLeft,
        PopupEdge::TopRight => Anchor::TopRight,
        PopupEdge::BottomRight => Anchor::BottomRight,
    }
}

fn gravity(edge: PopupEdge) -> Gravity {
    match edge {
        PopupEdge::None => Gravity::None,
        PopupEdge::Top => Gravity::Top,
        PopupEdge::Bottom => Gravity::Bottom,
        PopupEdge::Left => Gravity::Left,
        PopupEdge::Right => Gravity::Right,
        PopupEdge::TopLeft => Gravity::TopLeft,
        PopupEdge::BottomLeft => Gravity::BottomLeft,
        PopupEdge::TopRight => Gravity::TopRight,
        PopupEdge::BottomRight => Gravity::BottomRight,
    }
}

fn adjustment(adjust: PopupAdjust) -> ConstraintAdjustment {
    let mut flags = ConstraintAdjustment::None;
    for (wanted, flag) in [
        (adjust.slide_x, ConstraintAdjustment::SlideX),
        (adjust.slide_y, ConstraintAdjustment::SlideY),
        (adjust.flip_x, ConstraintAdjustment::FlipX),
        (adjust.flip_y, ConstraintAdjustment::FlipY),
        (adjust.resize_x, ConstraintAdjustment::ResizeX),
        (adjust.resize_y, ConstraintAdjustment::ResizeY),
    ] {
        if wanted {
            flags |= flag;
        }
    }
    flags
}
