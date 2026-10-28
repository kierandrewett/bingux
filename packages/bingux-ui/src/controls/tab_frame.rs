//! The frame that tabs are attached to: a bar with the tabs in it, and the
//! content the selected tab opens into.
//!
//! This is Chromium's window: the tabs stand on the content, and the selected
//! tab and the content are one shape. The frame owns the two facts that make
//! that true, so a tab strip cannot get them wrong.
//!
//! * **One surface.** The content is painted in the *surface* (the page
//!   colour, [`Paint::Bg`]). The frame tells the strip inside it, so the
//!   selected tab is painted in the same colour, from the same value. The bar
//!   behind the tabs is one step darker ([`Paint::TabStrip`]).
//! * **One seam.** The bar's bottom edge is the content's top edge, with no
//!   gap and no overlap. A hairline runs along the bar's bottom edge, and the
//!   selected tab's outline stops on it, so the line is drawn everywhere
//!   except under the selected tab.
//!
//! The bar is [`Dim::Header`] tall, like a header bar, and takes either: a
//! [`HeaderBar`](crate::shell::HeaderBar) whose fill is transparent and whose
//! title slot holds the strip, so the window controls sit in the same row, or
//! a bare row that holds a [`TabStrip`](super::TabStrip). Pressing the bar
//! where there is no tab drags the window when the bar is a header bar.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;

/// The paint of the content and of the selected tab, unless the frame is
/// given another.
pub(crate) const SURFACE: Paint = Paint::Bg;
/// The paint of the bar behind the tabs.
pub(crate) const STRIP: Paint = Paint::TabStrip;

/// The surface the frame's content is painted in, for the strip inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TabSurface(pub Paint);

/// A bar of tabs with `content` attached under it. The frame fills its
/// container: put it where the window's page goes.
pub fn tab_frame(bar: impl Into<Element>, content: impl Into<Element>) -> TabFrame {
    TabFrame {
        bar: bar.into(),
        content: content.into(),
        surface: SURFACE,
    }
}

#[derive(PartialEq)]
pub struct TabFrame {
    bar: Element,
    content: Element,
    surface: Paint,
}

impl TabFrame {
    /// Paint the content, and so the selected tab, in `paint` instead of the
    /// page colour: a frame inside a card is painted in the card's.
    pub fn surface(mut self, paint: Paint) -> Self {
        self.surface = paint;
        self
    }
}

impl Component for TabFrame {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let surface = self.surface;
        use_provide_context(move || TabSurface(surface));
        // The hairline is the bar's last row, under the bar's children, which
        // are transparent: the selected tab paints over it.
        let seam = Dim::Header.px() - Dim::Hairline.px();
        rect()
            .component("TabFrame")
            .tw(&tokens, &[w_full(), h_full(), flex_col()])
            .child(
                rect()
                    .component("TabBar")
                    .tw(&tokens, &[w_full(), h(Dim::Header), bg(STRIP)])
                    .child(
                        rect()
                            .position(Position::new_absolute().top(seam).left(0.0))
                            .interactive(false)
                            .tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]),
                    )
                    .child(self.bar.clone()),
            )
            .child(
                rect()
                    .component("TabContent")
                    .tw(&tokens, &[w_full(), h_flex(), flex_col(), bg(surface)])
                    .child(self.content.clone()),
            )
    }
}
