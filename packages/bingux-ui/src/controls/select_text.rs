//! Text a person can select and copy.
//!
//! [`select_text`] is the one control for text that is worth copying: a value
//! in an info row (a path, an address, a version, a hash, a licence), a block
//! of licence text, the details of an error. It wraps Freya's
//! `SelectableText`, which does the selecting: a drag selects, a double click
//! selects a word, a triple click a line, Shift+arrows extend, Ctrl+A selects
//! all and Ctrl+C copies the selection through the clipboard. This control adds
//! what the toolkit leaves out:
//!
//! * The selection is drawn in [`Paint::Selection`], not Freya's fixed blue.
//! * A right click, or a touch that rests for a long press, offers **Copy**,
//!   which copies the whole text (the selection is copied by Ctrl+C).
//! * The text is never a Tab stop unless asked (`tab_stop(true)`, for a block a
//!   keyboard user is to select from), so a page's Tab order is its controls.
//! * It has an accessibility role and its text as a label.
//! * It never cuts the text: it wraps between words, and a text that declares
//!   [`breaks_anywhere`](SelectText::breaks_anywhere) (a path, a hash, a
//!   device id, a MIME type) breaks wherever the line ends when a single word
//!   is wider than the line. Layout checks read that declaration, so a word
//!   broken in the middle is a finding unless it is declared.
//! * A selection changes no layout: it is a fill behind the glyphs.
//!
//! In a row that opens something on a press ([`crate::rows::Row::on_press`]) the
//! text is plain: a selection would take the press that opens the row.

use freya::prelude::*;

use super::press::{is_touch, use_press};
use crate::component::ComponentName;
use crate::rows::use_in_pressable_row;
use crate::shell::menu::menu_item;
use crate::shell::overlays::Overlays;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, TextStyle};

/// Text that can be selected and copied. See the [module](self).
pub fn select_text(text: impl Into<String>) -> SelectText {
    SelectText {
        text: text.into(),
        style: BODY,
        align: TextAlign::Start,
        fill: true,
        tab_stop: false,
        breaks_anywhere: false,
        copy_label: "Copy",
        key: None,
    }
}

#[derive(Clone, PartialEq)]
pub struct SelectText {
    text: String,
    style: TextStyle,
    align: TextAlign,
    fill: bool,
    tab_stop: bool,
    breaks_anywhere: bool,
    copy_label: &'static str,
    key: Option<String>,
}

impl SelectText {
    /// The type style (body text by default).
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Where the lines sit in the box: `TextAlign::End` for a value at the end
    /// of a row.
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Whether the text takes the width of its parent (the default) or only
    /// the width its longest line needs.
    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    /// Makes the text a Tab stop, so a keyboard user can focus it and use
    /// Ctrl+A and Ctrl+C. Off by default.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Declares that the text may break inside a word when one is wider than
    /// the line, for a path, a hash, an address or an identifier that has no
    /// space to break at.
    pub fn breaks_anywhere(mut self) -> Self {
        self.breaks_anywhere = true;
        self
    }

    /// The label of the context-menu item that copies the text ("Copy path").
    pub fn copy_label(mut self, label: &'static str) -> Self {
        self.copy_label = label;
        self
    }

    /// Keys the control, so a text that is replaced by another is a new one
    /// and keeps no selection.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl From<SelectText> for Element {
    fn from(text: SelectText) -> Element {
        let key = text.key.clone();
        let element = Element::from(SelectTextView(text));
        match key {
            Some(key) => rect().key(key).child(element).into(),
            None => element,
        }
    }
}

/// Copies `text` to the clipboard and says so, or says that it could not.
pub(crate) fn copy_text(overlays: Overlays, text: String) {
    match freya::clipboard::Clipboard::set(text) {
        Ok(()) => overlays.toast("Copied."),
        Err(_) => overlays.toast("The text could not be copied."),
    }
}

#[derive(PartialEq)]
struct SelectTextView(SelectText);

impl Component for SelectTextView {
    fn render(&self) -> impl IntoElement {
        let text = &self.0;
        let tokens = use_tokens();
        let overlays = try_consume_context::<Overlays>();
        let press = use_press();
        let mut area = use_state(|| None::<Area>);
        let pressable = use_in_pressable_row();

        let body: Element = if pressable {
            // Plain: the row's own press must keep working.
            text.style
                .paragraph(&tokens, text.text.clone())
                .text_align(text.align)
                .maybe(text.fill, |paragraph| paragraph.width(Size::fill()))
                .into()
        } else {
            SelectableText::new()
                .span(text.style.span(&tokens, text.text.clone()))
                .line_height(text.style.leading)
                .text_align(text.align)
                .highlight_color(Paint::Selection.color(&tokens))
                .a11y_role(AccessibilityRole::Label)
                .a11y_alt(text.text.clone())
                .a11y_focusable(text.tab_stop)
                .maybe(text.fill, |selectable| selectable.width(Size::fill()))
                .into()
        };

        let copy = text.text.clone();
        let label = text.copy_label;
        let items = move || {
            let copy = copy.clone();
            overlays.map(|overlays| vec![menu_item(label, move |_| copy_text(overlays, copy.clone()))])
        };
        let touch_items = items.clone();
        let right_items = items;
        rect()
            .component("SelectText")
            .maybe(text.fill, |wrapper| wrapper.width(Size::fill()))
            .on_sized(move |event: Event<SizedEventData>| {
                area.set_if_modified(Some(event.area));
            })
            // A right click is not a selection, so it reaches this box. A touch
            // is a selection's own, which stops it here, so the long press
            // watches the window's touches instead and keeps those that land
            // on this box.
            .on_pointer_down(move |event: Event<PointerEventData>| {
                let right = matches!(
                    event.data(),
                    PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Right)
                );
                if let (true, Some(overlays), Some(items)) = (right, overlays, right_items()) {
                    event.stop_propagation();
                    let at = event.global_location();
                    overlays.show_menu((at.x as f32, at.y as f32), items);
                }
            })
            .on_global_pointer_down(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                let (x, y) = (at.x as f32, at.y as f32);
                let inside = area().is_some_and(|area| {
                    x >= area.min_x() && x <= area.max_x() && y >= area.min_y() && y <= area.max_y()
                });
                if !is_touch(&event) || !inside {
                    return;
                }
                let long = overlays.zip(touch_items()).map(|(overlays, items)| {
                    EventHandler::new(move |at: (f32, f32)| overlays.show_menu(at, items.clone()))
                });
                press.begin(&event, long);
            })
            .on_global_pointer_up(move |_| press.release())
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .child(if text.breaks_anywhere {
                // The Layout checks read this box as "a word may break
                // anywhere here".
                rect().component("BreaksAnywhere").width(Size::fill()).child(body).into_element()
            } else {
                body
            })
    }
}

/// The clipboard of a test: memory, because the system's belongs to the
/// person at the desk and a test must not write it.
#[cfg(all(test, feature = "testing"))]
pub(crate) mod test_clipboard {
    use std::sync::{Arc, Mutex};

    use copypasta::ClipboardProvider;
    use freya::clipboard::Clipboard;
    use freya_core::prelude::GlobalContexts;

    type Fail = Box<dyn std::error::Error + Send + Sync + 'static>;

    /// What was copied last.
    #[derive(Clone, Default)]
    pub(crate) struct Memory(Arc<Mutex<String>>);

    impl Memory {
        pub(crate) fn text(&self) -> String {
            self.0.lock().map(|text| text.clone()).unwrap_or_default()
        }
    }

    impl ClipboardProvider for Memory {
        fn get_contents(&mut self) -> Result<String, Fail> {
            Ok(self.text())
        }

        fn set_contents(&mut self, text: String) -> Result<(), Fail> {
            *self.0.lock().map_err(|_| Fail::from("poisoned"))? = text;
            Ok(())
        }
    }

    /// Puts a memory clipboard in the place of the window's, for the rest of
    /// the test, and returns it to read what the app copied.
    pub(crate) fn install() -> Memory {
        let memory = Memory::default();
        GlobalContexts::get().insert_context(Clipboard::create(Some(Box::new(memory.clone()))));
        memory
    }
}

#[cfg(all(test, feature = "testing"))]
mod tests {
    use std::time::Duration;

    use super::test_clipboard;
    use super::*;
    use crate::controls::path_label::text_width;
    use crate::inspector::snapshot::Snapshot as Tree;
    use crate::snapshot::Snapshot;
    use crate::typography::BODY;

    const TEXT: &str = "alpha beta gamma delta epsilon";

    #[derive(PartialEq)]
    struct Scene;

    impl Component for Scene {
        fn render(&self) -> impl IntoElement {
            let tokens = crate::theme::provide_tokens();
            rect().expanded().tw(&tokens, &[p(S4)]).child(crate::shell::OverlayHost::new(
                rect()
                    .tw(&tokens, &[w_full()])
                    .child(Element::from(select_text(TEXT))),
            ))
        }
    }

    /// A window with the text and a memory clipboard; where the text is.
    fn scene() -> (Snapshot, test_clipboard::Memory, [f32; 4]) {
        let mut shot = Snapshot::new(|| Scene.into(), (400.0, 160.0));
        let clipboard = test_clipboard::install();
        shot.settle(Duration::from_millis(400));
        let at = text_area(&mut shot);
        (shot, clipboard, at)
    }

    /// The box the text paragraph has: `[x, y, width, height]`.
    fn text_area(shot: &mut Snapshot) -> [f32; 4] {
        let tree = Tree::capture_headless(shot.runner(), 1.0);
        let node = tree
            .nodes
            .iter()
            .find(|node| node.text.as_deref() == Some(TEXT))
            .expect("the text is in the tree");
        let area = node.area.expect("it has a box");
        [area.min_x(), area.min_y(), area.width(), area.height()]
    }

    fn ctrl(shot: &mut Snapshot, key: &str) {
        shot.key_with(Key::Character(key.to_owned()), Modifiers::CONTROL);
        shot.settle(Duration::from_millis(100));
    }

    #[test]
    fn a_drag_selects_and_ctrl_c_copies_exactly_what_it_covered() {
        let (mut shot, clipboard, [x, y, _, h]) = scene();
        let end = shot.runner().run_in(|| text_width("alpha beta", BODY));
        shot.drag((x + 1.0, y + h / 2.0), (x + end + 1.0, y + h / 2.0));
        shot.settle(Duration::from_millis(100));
        ctrl(&mut shot, "c");
        assert_eq!(clipboard.text(), "alpha beta");
    }

    #[test]
    fn selecting_moves_nothing() {
        let (mut shot, _clipboard, [x, y, w, h]) = scene();
        let before = text_area(&mut shot);
        shot.drag((x + 1.0, y + h / 2.0), (x + w - 1.0, y + h / 2.0));
        shot.settle(Duration::from_millis(100));
        assert_eq!(text_area(&mut shot), before);
    }

    #[test]
    fn the_context_menu_copies_the_whole_text() {
        let (mut shot, clipboard, [x, y, _, h]) = scene();
        shot.right_click(x + 4.0, y + h / 2.0);
        shot.settle(Duration::from_millis(300));
        assert!(shot.click_text("Copy"), "a right click offers Copy");
        shot.settle(Duration::from_millis(200));
        assert_eq!(clipboard.text(), TEXT);
    }

    #[test]
    fn a_long_press_with_a_finger_offers_copy_too() {
        let (mut shot, clipboard, [x, y, _, h]) = scene();
        shot.long_press(x + 4.0, y + h / 2.0);
        shot.settle(Duration::from_millis(300));
        assert!(shot.click_text("Copy"), "a long press offers Copy");
        shot.settle(Duration::from_millis(200));
        assert_eq!(clipboard.text(), TEXT);
    }

    #[test]
    fn the_text_is_not_a_tab_stop_unless_asked() {
        let (mut shot, _clipboard, _) = scene();
        let tree = Tree::capture_headless(shot.runner(), 1.0);
        let node = tree
            .nodes
            .iter()
            .find(|node| node.text.as_deref() == Some(TEXT))
            .expect("the text");
        assert_eq!(node.a11y.focusable, Some(false));
        assert_eq!(node.a11y.role, "Label");
    }
}
