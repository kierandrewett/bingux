use freya_engine::prelude::TextDirection as SkTextDirection;

/// The direction a line of text, and the layout around it, reads in.
///
/// It is inherited like the font size: set it on one element and every
/// descendant has it, until one sets the other. Text is shaped in its own
/// script's direction either way; this is the paragraph's base direction, which
/// decides where [`TextAlign::Start`](super::text_align::TextAlign::Start) is,
/// which way a trailing `.` or `:` falls and where a line is cut. It also
/// mirrors the layout: the children of an element that reads
/// [`Rtl`](TextDirection::Rtl) are laid out from the right (see `torin::node::Node::rtl`),
/// and its borders, corner radii and shadows are drawn mirrored.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default, Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum TextDirection {
    /// Left to right. This is the default.
    #[default]
    Ltr,
    /// Right to left.
    Rtl,
}

impl TextDirection {
    /// Whether this is [`Rtl`](TextDirection::Rtl).
    pub const fn is_rtl(self) -> bool {
        matches!(self, TextDirection::Rtl)
    }

    pub fn pretty(&self) -> String {
        match self {
            Self::Ltr => "ltr".to_string(),
            Self::Rtl => "rtl".to_string(),
        }
    }
}

impl From<TextDirection> for SkTextDirection {
    fn from(value: TextDirection) -> Self {
        match value {
            TextDirection::Ltr => SkTextDirection::LTR,
            TextDirection::Rtl => SkTextDirection::RTL,
        }
    }
}
