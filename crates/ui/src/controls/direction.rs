//! The reading direction of the interface: left to right, or right to left.
//!
//! The window's root says which way it reads ([`window_directed`]), and the
//! engine does the rest, as a browser does for `dir="rtl"`: inside a box that
//! reads right to left the children are laid out from the right (rows run
//! right to left, a column's children are aligned to its right edge, a start
//! padding, margin or absolute `left` is on the right), text is set in a
//! right to left paragraph (so a line is aligned to its box's right edge and a
//! trailing `.` or `:` falls on the left), and a scrolled content starts at its
//! right end. Code is written as for left to right and read as *start* and
//! *end*: `pl` is the padding at the start edge, `Align::Start` the start.
//! `Torin`'s `Node::rtl` and freya-core's `TextDirection` are that engine
//! (`vendor/README.md`).
//!
//! What the engine cannot know is physical: where a pointer is, what is drawn
//! on a canvas, which key means forward, which icon is a left arrow. A control
//! with that asks [`use_direction`]: the direction a [`directed`] ancestor gave
//! it, else the person's language ([`Direction::system`]; `BINGUX_DIRECTION=rtl`
//! or `ltr` wins, then `LC_ALL`, `LC_MESSAGES`, `LANGUAGE` and `LANG`, so
//! Arabic, Hebrew, Persian, Urdu and the other right-to-left scripts read from
//! the right). `design/rtl.md` lists what mirrors and what does not.

use std::sync::LazyLock;

use freya::prelude::*;

/// Which way the interface reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// From the left edge to the right: the start is on the left.
    #[default]
    Ltr,
    /// From the right edge to the left: the start is on the right.
    Rtl,
}

/// The environment's direction: `BINGUX_DIRECTION` when set, else the first of
/// the locale variables that has a value.
static SYSTEM: LazyLock<Direction> = LazyLock::new(|| {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.trim().is_empty());
    if let Some(forced) = var("BINGUX_DIRECTION") {
        return match forced.trim().to_ascii_lowercase().as_str() {
            "rtl" | "right-to-left" => Direction::Rtl,
            _ => Direction::Ltr,
        };
    }
    ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .iter()
        .find_map(|name| var(name))
        .map_or(Direction::Ltr, |tag| {
            // LANGUAGE is a list; its first entry is the preferred one.
            Direction::from_language(tag.split(':').next().unwrap_or(""))
        })
});

impl Direction {
    /// Whether the start edge is the right one.
    pub const fn is_rtl(self) -> bool {
        matches!(self, Direction::Rtl)
    }

    /// The direction as the text engine and the layout engine take it.
    pub const fn text(self) -> TextDirection {
        match self {
            Direction::Ltr => TextDirection::Ltr,
            Direction::Rtl => TextDirection::Rtl,
        }
    }

    /// `x`, a left edge measured in the box's reading direction (from its start
    /// edge) for something `width` wide in a box `box_width` wide, as a left
    /// edge on screen: `x` left to right, the reflection of it right to left.
    /// For code that places by pixels what the engine did not (a canvas, a
    /// pointer, a drawn mark).
    pub fn left_edge(self, x: f32, width: f32, box_width: f32) -> f32 {
        match self {
            Direction::Ltr => x,
            Direction::Rtl => box_width - x - width,
        }
    }

    /// `1.0` left to right, `-1.0` right to left: what a distance along the
    /// reading direction is multiplied by to be a distance along the screen's
    /// x axis.
    pub const fn sign(self) -> f32 {
        match self {
            Direction::Ltr => 1.0,
            Direction::Rtl => -1.0,
        }
    }

    /// `key` as a control written for a left-to-right interface reads it: in a
    /// right-to-left one Left and Right swap, so the control's own `ArrowRight`
    /// arm is "towards the end" (expand a tree row, the next segment, open a
    /// submenu) whatever the direction. Every other key is returned as it is.
    pub fn reading_key(self, key: &Key) -> Key {
        match (self, key) {
            (Direction::Rtl, Key::Named(NamedKey::ArrowLeft)) => Key::Named(NamedKey::ArrowRight),
            (Direction::Rtl, Key::Named(NamedKey::ArrowRight)) => Key::Named(NamedKey::ArrowLeft),
            _ => key.clone(),
        }
    }

    /// Which way along the reading direction an arrow key goes: `Some(1)` towards
    /// the end, `Some(-1)` towards the start, `None` for any other key (Up and
    /// Down are not along it). The arrows are the screen's: Right is forward in
    /// a left-to-right interface and backward in a right-to-left one, as in a
    /// browser's `<input type=range dir=rtl>`. A control that moves along its
    /// reading direction (a slider's value, a segment, a tree's expansion, a
    /// submenu) reads its horizontal keys here and not as `ArrowLeft` and `ArrowRight`.
    pub fn along(self, key: &Key) -> Option<i32> {
        let screen = match key {
            Key::Named(NamedKey::ArrowRight) => 1,
            Key::Named(NamedKey::ArrowLeft) => -1,
            _ => return None,
        };
        Some(screen * self.sign() as i32)
    }

    /// The direction a language tag (`ar`, `he_IL.UTF-8`, `fa-IR`) reads in.
    pub fn from_language(tag: &str) -> Direction {
        let primary = tag
            .trim()
            .split(['_', '-', '.', '@', ':'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match primary.as_str() {
            "ar" | "he" | "iw" | "fa" | "ur" | "ps" | "sd" | "ug" | "yi" | "dv" | "ckb" | "syr" => {
                Direction::Rtl
            }
            _ => Direction::Ltr,
        }
    }

    /// The direction the person's environment asks for. Read once.
    pub fn system() -> Direction {
        *SYSTEM
    }
}

/// The direction the subtree being rendered reads: the one a [`directed`]
/// ancestor gave it, else the system's.
pub fn use_direction() -> Direction {
    try_consume_context::<Direction>().unwrap_or_else(Direction::system)
}

/// `child` read in `direction`, whatever the system's is: a window that is
/// shown in another script's order, a specimen, a test.
pub fn directed(direction: Direction, child: impl Into<Element>) -> Directed {
    Directed {
        direction,
        child: child.into(),
        fill: false,
    }
}

/// [`directed`] for a window's whole content: the box it makes fills the
/// window. Every window of an app (and every snapshot) is rooted in one, with
/// the system's direction, so the engine knows which way the window reads.
pub fn window_directed(direction: Direction, child: impl Into<Element>) -> Directed {
    Directed {
        direction,
        child: child.into(),
        fill: true,
    }
}

/// A box, and a context, that read in one direction. See [`directed`].
#[derive(PartialEq)]
pub struct Directed {
    direction: Direction,
    child: Element,
    fill: bool,
}

impl Component for Directed {
    fn render(&self) -> impl IntoElement {
        let direction = self.direction;
        use_provide_context(move || direction);
        let root = rect().text_direction(direction.text()).child(self.child.clone());
        if self.fill {
            root.width(Size::fill()).height(Size::fill())
        } else {
            root
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_to_left_languages_read_from_the_right() {
        for tag in ["ar", "ar_EG.UTF-8", "he_IL", "iw", "fa-IR", "ur_PK@arabic", "ckb"] {
            assert_eq!(Direction::from_language(tag), Direction::Rtl, "{tag}");
        }
        for tag in ["en_GB.UTF-8", "de", "C", "", "ja_JP", "es-419", "ru"] {
            assert_eq!(Direction::from_language(tag), Direction::Ltr, "{tag}");
        }
    }

    #[test]
    fn a_distance_along_the_reading_direction_points_the_other_way_on_screen() {
        assert_eq!(Direction::Ltr.sign(), 1.0);
        assert_eq!(Direction::Rtl.sign(), -1.0);
    }
}
