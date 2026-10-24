//! The shape of a page while its very first load is still running.
//!
//! A page renders its real groups and rows from a placeholder model, so the
//! layout is already final on the first frame. The veil dims that layout and
//! sits over it to take presses, so a placeholder control can never write a
//! guess to the system. When the data lands the same rows keep their places
//! and only their values change.

use bingux_ui::style::*;
use freya::prelude::*;

use crate::ui::use_tokens;

/// How strongly the placeholder rows are dimmed.
const LOADING_OPACITY: f32 = 0.55;

/// `page` as it is while loading: dimmed, with a pane over it that takes every
/// press. Returns `page` untouched once `loading` is false.
pub fn veiled(page: Rect, loading: bool) -> Rect {
    if !loading {
        return page;
    }
    let tokens = use_tokens();
    page.opacity(LOADING_OPACITY).child(
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .tw(&tokens, &[w_full(), h_full()]),
    )
}

/// Whether this computer has a battery, read from sysfs. It is a directory
/// listing of a handful of entries, cheap enough to ask on the first frame, so
/// the Battery group is placed before UPower has answered.
pub fn has_battery_hint() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
        return false;
    };
    entries.flatten().any(|entry| {
        std::fs::read_to_string(entry.path().join("type"))
            .is_ok_and(|kind| kind.trim() == "Battery")
    })
}

/// Whether a touchpad is attached, from the kernel's device list. This is the
/// fallback `gnoblin::has_touchpad` uses, and it agrees with it on a running
/// session, so the Touchpad group is placed before Gnoblin has answered.
pub fn has_touchpad_hint() -> bool {
    std::fs::read_to_string("/proc/bus/input/devices").is_ok_and(|text| {
        text.lines().any(|line| {
            line.starts_with("N: Name=") && line.to_ascii_lowercase().contains("touchpad")
        })
    })
}
