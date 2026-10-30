//! The shape of a page while its very first load is still running.
//!
//! A page renders its real groups and rows from a placeholder model, so the
//! layout is already final on the first frame. The veil dims that layout and
//! sits over it to take presses, so a placeholder control can never write a
//! guess to the system. When the data lands the same rows keep their places
//! and only their values change.

use bingux_ui::style::*;
use freya::prelude::*;

/// How strongly the placeholder rows are dimmed.
const LOADING_OPACITY: f32 = 0.55;

/// `page` as it is while loading: dimmed, with a pane over it that takes every
/// press. Returns `page` untouched once `loading` is false.
pub fn veiled(page: Rect, loading: bool) -> Rect {
    // The page mounts a group a frame (see `bingux_ui::stage`), veiled or not.
    let page = bingux_ui::stage::staged(page);
    if !loading {
        return page;
    }
    // Not a hook: a page returns early from several places, some through
    // `veiled` and some not, and a hook here made the number of hooks depend on
    // the path (a panic when the data landed).
    let tokens = bingux_ui::surface::tokens_now();
    page.opacity(LOADING_OPACITY).child(
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .tw(&tokens, &[w_full(), h_full()]),
    )
}

/// A page whose shape follows what the machine reports (how many devices are
/// paired, which speakers a sound card has) cannot place its rows before the
/// read lands, so it does not pretend to. While `loading` the page is hidden
/// and takes every press; when the data lands it fades in, already at its
/// final layout. Nothing moves, because nothing was on screen to move. With a
/// saved value the page is not loading and shows at once.
pub fn revealed(page: Rect, loading: bool) -> Rect {
    rect().width(Size::fill()).child(Revealed {
        page: page.into(),
        loading,
    })
}

#[derive(PartialEq)]
struct Revealed {
    page: Element,
    loading: bool,
}

impl Component for Revealed {
    fn render(&self) -> impl IntoElement {
        let tokens = bingux_ui::surface::tokens_now();
        let shown = bingux_ui::motion::use_tween(
            if self.loading { 0.0 } else { 1.0 },
            tokens.motion(bingux_ui::motion::MOVE_MS),
        );
        rect()
            .width(Size::fill())
            .opacity(shown)
            .child(self.page.clone())
            .maybe_child((self.loading || shown < 1.0).then(|| {
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .tw(&tokens, &[w_full(), h_full()])
            }))
    }
}

/// What the machine has, read once on a thread of its own at start-up: the
/// placeholder of a page asks for it in `render`, so it must be a lookup, not a
/// read. Each hint has its own cell and thread, so a slow one (a command) does
/// not hold back the fast ones (a file).
static BATTERY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static TOUCHPAD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static POWER_PROFILES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Read the hints on threads of their own. Call once at start-up. Until a
/// hint has answered (a few milliseconds; a command takes longer) it says no.
pub fn warm_hints() {
    std::thread::spawn(|| {
        let _ = BATTERY.set(read_battery());
        let _ = TOUCHPAD.set(read_touchpad());
    });
    std::thread::spawn(|| {
        let _ = POWER_PROFILES.set(read_power_profiles());
    });
}

/// Whether this computer has a battery, from sysfs, so the Battery group is
/// placed before UPower has answered.
pub fn has_battery_hint() -> bool {
    BATTERY.get().copied().unwrap_or(false)
}

/// Whether a touchpad is attached, from the kernel's device list. This is the
/// fallback `gnoblin::has_touchpad` uses, and it agrees with it on a running
/// session, so the Touchpad group is placed before Gnoblin has answered.
pub fn has_touchpad_hint() -> bool {
    TOUCHPAD.get().copied().unwrap_or(false)
}

/// Whether the power profiles service answers, so the Power mode group is
/// placed as cards or as a notice before the page's own read has landed.
pub fn has_power_profiles_hint() -> bool {
    POWER_PROFILES.get().copied().unwrap_or(false)
}

fn read_power_profiles() -> bool {
    crate::system::run(
        "powerprofilesctl",
        &["get"],
        std::time::Duration::from_secs(3),
    )
    .is_ok()
}

fn read_battery() -> bool {
    let Ok(entries) = crate::system::read_dir("/sys/class/power_supply") else {
        return false;
    };
    entries.flatten().any(|entry| {
        crate::system::read_to_string(entry.path().join("type"))
            .is_ok_and(|kind| kind.trim() == "Battery")
    })
}

fn read_touchpad() -> bool {
    crate::system::read_to_string("/proc/bus/input/devices").is_ok_and(|text| {
        text.lines().any(|line| {
            line.starts_with("N: Name=") && line.to_ascii_lowercase().contains("touchpad")
        })
    })
}
