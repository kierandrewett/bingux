//! A single line of text that does not fit is shortened by what matters
//! least, so the full path stays readable:
//! `/home/kieran/…/design/final-report-2026.pdf`, then
//! `/h/k/…/design/final-report-2026.pdf`, then `final-rep…2026.pdf`.
//!
//! [`path_label`] is the control. [`elide_path`], [`elide_name`] and
//! [`elide_middle`] are the pure functions under it, in [`crate::elide`]:
//! they take a width and a [`Measure`] and return the text to draw.
//! [`measure_text`] measures a string with the font a label in a
//! [`TextStyle`] renders with, so the decision agrees with the pixels. A
//! breadcrumb or a list row can use the functions directly when it lays text
//! out itself.
//!
//! The ladder, stopping at the first rung that fits (the reasons are in
//! `design/path-elision.md`):
//!
//! 1. The path whole.
//! 2. Directories from the middle outwards become one `…`; the anchor and as
//!    many trailing directories as fit stay.
//! 3. The directories left are abbreviated to their first letter, longest
//!    first; the directory that holds the file stays whole.
//! 4. The name loses its middle and keeps its extension and the end of its
//!    stem; below a few letters the directories give way to it.
//! 5. The name loses its extension too; at the very least `…`.
//!
//! The text is never empty: the least that is returned is `…`. The full text
//! stays the tooltip, the accessible label and what Copy copies.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

use freya::engine::prelude::{
    FontCollection, FontMgr, FontStyle, ParagraphBuilder, ParagraphStyle, Slant,
    TextStyle as SkTextStyle, Width,
};
use freya::prelude::*;
use freya_core::current_context::CurrentContext;

use super::press::{Press, use_press};
use super::tooltip::hover_tip;
use crate::component::ComponentName;
use crate::elide::isolate_ltr;
pub use crate::elide::{ElideStep, Measure, elide_middle, elide_name, elide_path, elide_path_traced};
use crate::shell::{MenuItem, Overlays, menu_item};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink, TextStyle, mono_family};

// ---------------------------------------------------------------------------
// Measuring
// ---------------------------------------------------------------------------

/// What makes two styles measure differently.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct StyleKey {
    size: u32,
    weight: i32,
    tracking: u32,
    tabular: bool,
    mono: bool,
}

impl StyleKey {
    fn of(style: TextStyle, size: f32) -> Self {
        Self {
            size: size.to_bits(),
            weight: i32::from(style.weight),
            tracking: style.tracking.to_bits(),
            tabular: style.tabular,
            mono: style.mono,
        }
    }
}

/// The font size of `style` at `scale`: what `TextStyle::label_scaled` draws,
/// a quarter pixel step. At scale 1 it is the style's own size.
fn scaled_size(style: TextStyle, scale: f32) -> f32 {
    if scale == 1.0 {
        style.size.px()
    } else {
        (style.size.px() * scale * 4.0).round() / 4.0
    }
}

/// A style ready to lay out, and the widths measured with it.
struct Measured {
    paragraph: ParagraphStyle,
    text: SkTextStyle,
    widths: HashMap<Box<str>, f32>,
}

/// How many strings one style remembers before it starts again. A page of
/// rows repeats a few hundred segments; this is generous.
const MEMORY: usize = 8192;

thread_local! {
    static MEASURED: RefCell<HashMap<StyleKey, Measured>> = RefCell::new(HashMap::new());
    /// The system fonts, for measuring outside a running app.
    static SYSTEM_FONTS: FontCollection = {
        let mut fonts = FontCollection::new();
        fonts.set_default_font_manager(FontMgr::default(), None);
        fonts
    };
}

/// The families a label falls back through: the interface font first, as the
/// app registers it as the default.
static FAMILIES: LazyLock<[Vec<String>; 2]> = LazyLock::new(|| {
    let rest = || freya_core::style::default_fonts::default_fonts().into_iter().map(|font| font.to_string());
    [
        std::iter::once(crate::theme::font_family().to_owned())
            .chain(rest())
            .collect(),
        std::iter::once(mono_family().to_owned()).chain(rest()).collect(),
    ]
});

/// The width in px of `text` on one line in `style`: the width the same text
/// has in a `style.label(..)`, laid out by the same text engine with the same
/// fonts. Call it while a component renders so it finds the app's fonts; with
/// no app running it uses the system's.
///
/// Results are remembered per style, so measuring a hundred rows on every
/// frame costs hash lookups.
pub(crate) fn measure_text(text: &str, style: TextStyle) -> f32 {
    measure_text_scaled(text, style, 1.0)
}

/// The width in px of `text` on one line in `style`, as a path label measures
/// it: for a layout of your own that has to agree with the label.
pub fn text_width(text: &str, style: TextStyle) -> f32 {
    measure_text(text, style)
}

/// What a [`path_label`] in `style` draws for `path` in `width` px, and
/// which form of the ladder it is ([`ElideStep`]). For the taxonomy's
/// width slider and for tests of a layout that shows paths.
pub fn path_label_trace(path: &str, style: TextStyle, width: f32) -> (String, ElideStep) {
    elide_path_traced(path, width, &move |text: &str| measure_text(text, style))
}

/// [`measure_text`] for text drawn with `TextStyle::label_scaled(.., scale)`:
/// the font size is scaled and rounded to a quarter pixel, as the label does.
pub(crate) fn measure_text_scaled(text: &str, style: TextStyle, scale: f32) -> f32 {
    // Measured as drawn, so a pseudo-localised label measures its longer form.
    let text = &*crate::stress::localise(text);
    if text.is_empty() {
        return 0.0;
    }
    let size = scaled_size(style, scale);
    let key = StyleKey::of(style, size);
    if let Some(width) = MEASURED.with_borrow(|all| {
        all.get(&key)
            .and_then(|measured| measured.widths.get(text).copied())
    }) {
        return width;
    }
    let app_fonts = CurrentContext::try_with(|_| try_consume_root_context::<FontCollection>()).flatten();
    let remember = app_fonts.is_some();
    let fonts = app_fonts.unwrap_or_else(|| SYSTEM_FONTS.with(Clone::clone));
    MEASURED.with_borrow_mut(|all| {
        let measured = all.entry(key).or_insert_with(|| measured_style(style, size));
        let mut builder = ParagraphBuilder::new(&measured.paragraph, fonts);
        builder.push_style(&measured.text);
        builder.add_text(text);
        let mut paragraph = builder.build();
        paragraph.layout(f32::MAX);
        let width = paragraph.longest_line();
        if remember {
            if measured.widths.len() >= MEMORY {
                measured.widths.clear();
            }
            measured.widths.insert(text.into(), width);
        }
        width
    })
}

/// The text style a `style.label(..)` has, in the engine's terms: what
/// Freya's label builds for its paragraph, minus what does not change width.
fn measured_style(style: TextStyle, size: f32) -> Measured {
    let mut text = SkTextStyle::new();
    text.set_font_size(size);
    text.set_font_families(&FAMILIES[usize::from(style.mono)]);
    text.set_font_style(FontStyle::new(style.weight.into(), Width::NORMAL, Slant::Upright));
    text.set_letter_spacing(style.tracking);
    if style.tabular {
        text.add_font_feature("tnum", 1);
    }
    let mut paragraph = ParagraphStyle::new();
    paragraph.set_text_style(&text);
    Measured {
        paragraph,
        text,
        widths: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// The control
// ---------------------------------------------------------------------------

/// Which cut a [`PathLabel`] makes.
#[derive(Clone, Copy, PartialEq)]
enum Cut {
    Path,
    Name,
    Middle,
}

/// A path, a file name or other text on one line that is cut in the middle
/// to fit the width it is given. Build it with [`path_label`].
///
/// It fills the width of its parent and is one line tall, so a cut never
/// changes the layout around it. Put it in a box that has a width of its own
/// (a row cell, a column), not in one that is as wide as its content.
#[derive(Clone, PartialEq)]
pub struct PathLabel {
    text: String,
    style: TextStyle,
    scale: f32,
    cut: Cut,
    selectable: bool,
    copyable: bool,
    align: Option<Align>,
    tooltip: bool,
}

/// A path on one line, ellipsed in the middle when it does not fit, as in
/// `~/Documents/…/Reports/final.pdf`. The full path is the tooltip while it
/// is cut.
///
/// ```ignore
/// path_label("/home/kieran/Documents/Reports/final.pdf")
///     .style(CAPTION)
///     .copyable(true)
/// ```
pub fn path_label(path: impl Into<String>) -> PathLabel {
    PathLabel {
        text: path.into(),
        style: BODY,
        scale: 1.0,
        cut: Cut::Path,
        selectable: false,
        copyable: false,
        align: None,
        tooltip: true,
    }
}

impl PathLabel {
    /// The text style. The default is [`BODY`].
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// The colour of the text.
    pub fn ink(mut self, ink: Ink) -> Self {
        self.style = self.style.in_ink(ink);
        self
    }

    /// The text is a file or folder name, not a path: only the name is cut,
    /// keeping its extension (`final-rep…2026.pdf`).
    pub fn name(mut self) -> Self {
        self.cut = Cut::Name;
        self
    }

    /// The text is neither: cut in the middle with no special care.
    pub fn middle(mut self) -> Self {
        self.cut = Cut::Middle;
        self
    }

    /// Lets the pointer select the text and Ctrl+C copy it, for a properties
    /// dialog. The selection is of what is shown, so it is the cut text while
    /// the text is cut; pair it with [`copyable`](Self::copyable) for the
    /// whole. It is not a Tab stop. A list row leaves this off, because a
    /// selection would take the press that opens the row.
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// A right click, or a long press on a touch screen, offers to copy the
    /// whole text, even while it is cut.
    pub fn copyable(mut self, copyable: bool) -> Self {
        self.copyable = copyable;
        self
    }

    /// Draws the text at `scale` times its size, rounded to a quarter pixel,
    /// as `TextStyle::label_scaled` does, and cuts it by the scaled width.
    /// A view that zooms its text uses it. The line height scales too.
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Centres the text in its width, as a grid caption is. Text that is cut
    /// fills the width anyway.
    pub fn centre(mut self) -> Self {
        self.align = Some(Align::Center);
        self
    }

    /// Puts text that fits at the end of the width, as a value at the right
    /// of a row is. Text that is cut fills the width anyway.
    pub fn end(mut self) -> Self {
        self.align = Some(Align::End);
        self
    }

    /// Whether the full text shows as a hover tooltip while it is cut. On by
    /// default; turn it off where the surroundings already show the detail.
    pub fn tooltip(mut self, tooltip: bool) -> Self {
        self.tooltip = tooltip;
        self
    }

    /// The text to draw in `width` px.
    fn elided(&self, width: f32) -> String {
        let (style, scale) = (self.style, self.scale);
        let measure = move |text: &str| measure_text_scaled(text, style, scale);
        match self.cut {
            Cut::Path => elide_path(&self.text, width, &measure),
            Cut::Name => elide_name(&self.text, width, &measure),
            Cut::Middle => elide_middle(&self.text, width, &measure),
        }
    }
}

/// The last cut a label made, so a render with the same text, width and
/// scale does not cut again.
struct Memo {
    text: String,
    width: f32,
    scale: f32,
    cut: Cut,
    style: TextStyle,
    out: String,
}

impl Component for PathLabel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut width = use_state(|| None::<f32>);
        let memo = use_hook(|| Rc::new(RefCell::new(None::<Memo>)));
        // Before the first size is known the whole text is drawn on one line
        // and clipped, so the first frame is never empty. Each size event
        // after that sets the width, and the next frame draws the cut for it.
        let shown = match width() {
            Some(room) => {
                let mut memo = memo.borrow_mut();
                match memo.as_ref() {
                    Some(last)
                        if last.width == room
                            && last.scale == self.scale
                            && last.cut == self.cut
                            && last.style == self.style
                            && last.text == self.text =>
                    {
                        last.out.clone()
                    }
                    _ => {
                        let out = self.elided(room);
                        *memo = Some(Memo {
                            text: self.text.clone(),
                            width: room,
                            scale: self.scale,
                            cut: self.cut,
                            style: self.style,
                            out: out.clone(),
                        });
                        out
                    }
                }
            }
            None => self.text.clone(),
        };
        let cut = shown != self.text;
        // A path reads from its root whatever the language of its names:
        // text with right-to-left letters is drawn as one left-to-right run.
        let drawn = isolate_ltr(&shown);

        let text: Element = if self.selectable {
            SelectableText::new()
                .span(self.style.span(&tokens, drawn))
                .max_lines(1)
                .line_height(self.style.leading)
                .a11y_focusable(false)
                .into()
        } else {
            self.style
                .label_scaled(&tokens, drawn, self.scale)
                .max_lines(1)
                .into()
        };
        // The tooltip is the whole text, and only while it is cut. The
        // `Elided` box is what the Layout checks read as "this text is cut on
        // purpose, and its full text is the tooltip".
        let text = if cut && self.tooltip {
            rect().component("Elided").child(hover_tip(text, self.text.clone())).into()
        } else {
            text
        };

        // One line of the style, so the row does not change height with the
        // text. The label's own height can differ by a fraction of a pixel.
        let line = (scaled_size(self.style, self.scale) * self.style.leading).ceil();
        let body = rect()
            .component("PathLabel")
            .a11y_role(AccessibilityRole::Label)
            .a11y_alt(self.text.clone())
            .width(Size::fill())
            .height(Size::px(line))
            .tw(&tokens, &[clip()])
            .maybe(self.align.is_some() && width().is_some(), |element| {
                element.tw(&tokens, &[flex_col(), items(self.align.unwrap_or(Align::Center))])
            })
            .on_sized(move |event: Event<SizedEventData>| {
                width.set_if_modified(Some(event.area.width()));
            })
            .child(text);
        if self.copyable {
            Element::from(CopyMenu {
                child: body.into(),
                text: self.text.clone(),
                label: match self.cut {
                    Cut::Path => "Copy path",
                    Cut::Name => "Copy name",
                    Cut::Middle => "Copy",
                },
            })
        } else {
            body.into()
        }
    }
}

/// Wraps a label with the right click and long press that offer to copy its
/// whole text. Only labels that ask for it carry the press state.
#[derive(Clone, PartialEq)]
struct CopyMenu {
    child: Element,
    text: String,
    label: &'static str,
}

impl Component for CopyMenu {
    fn render(&self) -> impl IntoElement {
        let overlays = try_consume_context::<Overlays>();
        let press = use_press();
        let items = overlays.map(|overlays| {
            let text = self.text.clone();
            vec![menu_item(self.label, move |_| {
                match freya::clipboard::Clipboard::set(text.clone()) {
                    Ok(()) => overlays.toast("Copied."),
                    Err(_) => overlays.toast("The text could not be copied."),
                }
            })]
        });
        let items = items.unwrap_or_default();
        let touch_items = items.clone();
        rect()
            .width(Size::fill())
            .on_pointer_down(move |event: Event<PointerEventData>| {
                open_menu(overlays, &items, &event);
                begin_touch(press, overlays, &touch_items, &event);
            })
            .on_global_pointer_up(move |_| press.release())
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .child(self.child.clone())
    }
}

/// On a right press opens `items` at the pointer and stops the press there.
fn open_menu(overlays: Option<Overlays>, items: &[MenuItem], event: &Event<PointerEventData>) {
    let right = matches!(
        event.data(),
        PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
    );
    if let Some(overlays) = overlays
        && right
        && !items.is_empty()
    {
        event.stop_propagation();
        let at = event.global_location();
        overlays.show_menu((at.x as f32, at.y as f32), items.to_vec());
    }
}

/// A touch that rests opens the menu where it began.
fn begin_touch(
    press: Press,
    overlays: Option<Overlays>,
    items: &[MenuItem],
    event: &Event<PointerEventData>,
) {
    let long = overlays.map(|overlays| {
        let items = items.to_vec();
        EventHandler::new(move |at: (f32, f32)| overlays.show_menu(at, items.clone()))
    });
    press.begin(event, long);
}
