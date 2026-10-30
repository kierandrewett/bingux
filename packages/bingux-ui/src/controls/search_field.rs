//! The search field: the one control every filter, find bar and "search for"
//! well in the shell is built on (libadwaita `GtkSearchEntry`, SwiftUI
//! `searchable`).
//!
//! It is a [`TextField`](super::text_field::TextField) with three
//! things fixed so no caller can get them wrong:
//!
//! - **A square slot for the glyph.** The search icon sits in a slot as wide
//!   as the field is tall (the control row at the density, so 36 px regular
//!   and 44 px compact, 24 px for a `dense` field), centred in it on both
//!   axes. The text starts [`S1`](crate::style::S1) past the slot. Nothing
//!   about the icon is measured by eye: the slot is a box, the icon is its
//!   only child and is centred in it by the layout.
//! - **A clear button in the same kind of slot.** While the field holds text, a
//!   clear button sits at the trailing edge, centred on the field's height
//!   and the same distance from its end as the glyph is from its start (the
//!   text field's inline button).
//! - **Search behaviour.** The accessibility role is `SearchInput`. Escape
//!   clears the text while there is some, and leaves the field when there is
//!   none. Enter calls `on_submit`, when there is one. The placeholder, the
//!   hover and disabled fills and the focus ring are the text field's.

use freya::prelude::*;

use crate::controls::text_field::{TextField, text_field};
use crate::icons::Icon;
use crate::style::*;

/// A search field bound to a `State<String>`; `on_change` runs on every edit
/// with the new text, which is empty after the clear button or Escape.
#[derive(PartialEq)]
pub struct SearchField {
    value: State<String>,
    on_change: EventHandler<String>,
    placeholder: String,
    width: Dim,
    fill: bool,
    well: bool,
    dense: bool,
    flat: bool,
    enabled: bool,
    autofocus: bool,
    a11y_id: Option<AccessibilityId>,
    on_submit: Option<EventHandler<String>>,
    on_escape: Option<EventHandler<()>>,
    on_enter_modifiers: Option<EventHandler<Modifiers>>,
    on_arrow: Option<EventHandler<f64>>,
}

/// A search field over `value`. Its placeholder is `Search`.
pub fn search_field(value: State<String>, on_change: impl Into<EventHandler<String>>) -> SearchField {
    SearchField {
        value,
        on_change: on_change.into(),
        placeholder: "Search".to_string(),
        width: Dim::Field,
        fill: false,
        well: false,
        dense: false,
        flat: false,
        enabled: true,
        autofocus: false,
        a11y_id: None,
        on_submit: None,
        on_escape: None,
        on_enter_modifiers: None,
        on_arrow: None,
    }
}

impl SearchField {
    /// What the empty field says it searches, such as `Search pictures`.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn width(mut self, width: Dim) -> Self {
        self.width = width;
        self
    }

    /// Fill the container's width, for a filter above a list.
    pub fn fill_width(mut self) -> Self {
        self.fill = true;
        self
    }

    /// On the elevated well fill, for a field on the page or sidebar. A field
    /// on a card, dialog or popover keeps the lighter control fill.
    pub fn well(mut self) -> Self {
        self.well = true;
        self
    }

    /// The compact row of a property editor or inspector,
    /// [`Dim::DenseControl`] tall. The slot is that wide.
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }

    /// A square, borderless field with no fill, [`Dim::FlatField`] tall, for a
    /// find bar docked in a pane.
    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Take keyboard focus when the field first appears.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }

    /// Give the field an accessibility id, so a window can focus it.
    pub fn a11y_id(mut self, id: AccessibilityId) -> Self {
        self.a11y_id = Some(id);
        self
    }

    /// Called with the text when Enter is pressed.
    pub fn on_submit(mut self, on_submit: impl Into<EventHandler<String>>) -> Self {
        self.on_submit = Some(on_submit.into());
        self
    }

    /// Escape is the owner's: it runs, and the field neither clears nor
    /// gives up focus. The owner of a find bar closes the bar with it.
    pub fn on_escape(mut self, on_escape: impl Into<EventHandler<()>>) -> Self {
        self.on_escape = Some(on_escape.into());
        self
    }

    /// Told which modifiers were held each time Enter is pressed, so a find
    /// bar can tell Enter from Shift+Enter.
    pub fn on_enter_modifiers(mut self, handler: impl Into<EventHandler<Modifiers>>) -> Self {
        self.on_enter_modifiers = Some(handler.into());
        self
    }

    /// Told when Up or Down is pressed in the field, as the text field tells
    /// it: negative for Down, positive for Up. A list under the field moves
    /// its highlight with it.
    pub fn on_arrow(mut self, handler: impl Into<EventHandler<f64>>) -> Self {
        self.on_arrow = Some(handler.into());
        self
    }

    fn field(&self) -> TextField {
        let on_submit = self.on_submit.clone();
        let mut field = text_field(self.value, move |text: String| {
            if let Some(on_submit) = &on_submit {
                on_submit.call(text);
            }
        })
        .search()
        .leading(Icon::Search)
        .clearable()
        .placeholder(self.placeholder.clone())
        .width(self.width)
        .enabled(self.enabled)
        .on_change(self.on_change.clone());
        if self.fill {
            field = field.fill_width();
        }
        if self.well {
            field = field.well();
        }
        if self.dense {
            field = field.dense();
        }
        if self.flat {
            field = field.flat();
        }
        if self.autofocus {
            field = field.autofocus();
        }
        if let Some(id) = self.a11y_id {
            field = field.a11y_id(id);
        }
        if let Some(on_escape) = &self.on_escape {
            field = field.on_escape(on_escape.clone());
        }
        if let Some(handler) = &self.on_enter_modifiers {
            field = field.on_enter_modifiers(handler.clone());
        }
        if let Some(handler) = &self.on_arrow {
            field = field.on_arrow(handler.clone());
        }
        field
    }
}

impl Component for SearchField {
    fn render(&self) -> impl IntoElement {
        self.field()
    }
}

/// The geometry the search field promises, measured on the laid-out tree at
/// 1x and 2x, at regular and compact density (a desktop window and a phone),
/// and the pixels the renderer draws for it.
#[cfg(all(test, feature = "testing"))]
mod tests {
    use std::cell::RefCell;
    use std::time::Duration;

    use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};

    use super::*;
    use crate::inspector::snapshot::{Node, Snapshot as Tree};
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;

    thread_local! {
        static CHANGES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static SUBMITS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Look {
        Plain,
        Well,
        Dense,
        Flat,
        Disabled,
    }

    const LOOKS: [Look; 5] = [Look::Plain, Look::Well, Look::Dense, Look::Flat, Look::Disabled];

    #[derive(PartialEq)]
    struct Host {
        look: Look,
        typed: &'static str,
    }

    impl Component for Host {
        fn render(&self) -> impl IntoElement {
            let tokens = provide_tokens();
            let value = use_state(|| self.typed.to_string());
            let mut field = search_field(value, |text: String| CHANGES.with(|log| log.borrow_mut().push(text)))
                .on_submit(|text: String| SUBMITS.with(|log| log.borrow_mut().push(text)))
                .fill_width();
            field = match self.look {
                Look::Plain => field,
                Look::Well => field.well(),
                Look::Dense => field.dense(),
                Look::Flat => field.flat(),
                Look::Disabled => field.enabled(false),
            };
            rect().tw(&tokens, &[w_full(), h_full(), p(S5), bg(Paint::Surface)]).child(field)
        }
    }

    /// A window at a density and a display scale.
    #[derive(Clone, Copy, Debug)]
    struct Setup {
        class: SizeClass,
        scale: f64,
        look: Look,
        typed: &'static str,
    }

    /// Desktop at 1x and 2x, phone at 1x and 2x: the four places the field is
    /// drawn.
    fn setups(look: Look, typed: &'static str) -> Vec<Setup> {
        [(SizeClass::Regular, 1.0), (SizeClass::Regular, 2.0), (SizeClass::Compact, 1.0), (SizeClass::Compact, 2.0)]
            .into_iter()
            .map(|(class, scale)| Setup { class, scale, look, typed })
            .collect()
    }

    fn window(class: SizeClass) -> (f32, f32) {
        match class {
            SizeClass::Regular => (640.0, 240.0),
            SizeClass::Compact => (390.0, 240.0),
        }
    }

    /// One frame as drawn, in physical pixels.
    struct Frame {
        bytes: Vec<u8>,
        row: usize,
    }

    impl Frame {
        fn of(shot: &mut Snapshot) -> Self {
            let png = shot.runner().render();
            let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
            let mut surface = raster_n32_premul((image.width(), image.height())).expect("a surface");
            surface.canvas().draw_image(&image, (0, 0), None);
            let info = surface.canvas().image_info();
            let row = info.min_row_bytes();
            let mut bytes = vec![0u8; info.compute_byte_size(row)];
            assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
            Self { bytes, row }
        }

        fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
            let at = y * self.row + x * 4;
            [self.bytes[at + 2], self.bytes[at + 1], self.bytes[at]]
        }
    }

    type Box4 = [f32; 4];

    /// What a laid-out field looks like, in logical pixels.
    struct Seen {
        field: Box4,
        slot: Box4,
        icon: Box4,
        text_start: f32,
        clear: Option<Box4>,
        role: String,
    }

    fn box_of(node: &Node) -> Box4 {
        let area = node.used.border.or(node.area).expect("a laid-out node");
        [area.min_x(), area.min_y(), area.width(), area.height()]
    }

    fn open(setup: Setup) -> Snapshot {
        CHANGES.with(|log| log.borrow_mut().clear());
        SUBMITS.with(|log| log.borrow_mut().clear());
        let pointer = if setup.class.is_compact() { PointerKind::Touch } else { PointerKind::Mouse };
        mirror_density(setup.class, pointer);
        let Setup { look, typed, .. } = setup;
        let mut shot = Snapshot::with_scale(move || Host { look, typed }.into_element(), window(setup.class), setup.scale);
        shot.settle(Duration::from_millis(300));
        shot
    }

    fn seen(shot: &mut Snapshot, scale: f64) -> Seen {
        let tree = Tree::capture_headless(shot.runner(), scale as f32);
        let under = |root: &Node, name: &str| -> Option<Node> {
            let mut stack = vec![root.id];
            while let Some(id) = stack.pop() {
                let node = tree.node(id)?;
                if node.a11y.component.as_deref() == Some(name) {
                    return Some(node.clone());
                }
                stack.extend(node.children.iter().copied());
            }
            None
        };
        let field = tree
            .nodes
            .iter()
            .find(|node| node.a11y.role.ends_with("Input"))
            .expect("the field's input node")
            .clone();
        let slot = under(&field, "IconSlot").expect("the leading slot");
        let icon = tree.node(slot.children[0]).expect("the icon in the slot");
        let paragraph = {
            let mut stack = vec![field.id];
            let mut found = None;
            while let Some(id) = stack.pop() {
                let node = tree.node(id).unwrap();
                if node.kind == crate::inspector::snapshot::Kind::Paragraph {
                    found = Some(node.clone());
                    break;
                }
                stack.extend(node.children.iter().rev().copied());
            }
            found.expect("the field's text")
        };
        let padding = paragraph
            .used
            .padding
            .or_else(|| paragraph.props.as_ref().map(|props| props.padding))
            .unwrap_or([0.0; 4]);
        let text_box = box_of(&paragraph);
        Seen {
            field: box_of(&field),
            slot: box_of(&slot),
            icon: box_of(icon),
            text_start: text_box[0] + padding[3],
            clear: under(&field, "IconButton").map(|button| box_of(&button)),
            role: field.a11y.role.clone(),
        }
    }

    fn centre(area: Box4) -> (f32, f32) {
        (area[0] + area[2] / 2.0, area[1] + area[3] / 2.0)
    }

    fn height_of(look: Look, class: SizeClass) -> f32 {
        match look {
            Look::Dense => Dim::DenseControl.px_in(class),
            Look::Flat => Dim::FlatField.px_in(class),
            _ => Dim::Control.px_in(class),
        }
    }

    #[test]
    fn the_slot_is_as_wide_as_the_field_is_tall_at_every_density_and_scale() {
        for look in LOOKS {
            for setup in setups(look, "") {
                let mut shot = open(setup);
                let found = seen(&mut shot, setup.scale);
                let height = height_of(look, setup.class);
                assert!((found.field[3] - height).abs() < 0.01, "{setup:?}: field {:?} is not {height} tall", found.field);
                assert!((found.slot[2] - found.field[3]).abs() < 0.01, "{setup:?}: slot {:?} in field {:?}", found.slot, found.field);
                assert!((found.slot[3] - found.field[3]).abs() < 0.01, "{setup:?}: slot {:?} in field {:?}", found.slot, found.field);
                assert!((found.slot[0] - found.field[0]).abs() < 0.01, "{setup:?}: slot starts at {}, field at {}", found.slot[0], found.field[0]);
                assert!((found.slot[1] - found.field[1]).abs() < 0.01, "{setup:?}: slot {:?} in field {:?}", found.slot, found.field);
            }
        }
    }

    #[test]
    fn the_icon_centre_is_the_slot_centre_on_both_axes() {
        for look in LOOKS {
            for setup in setups(look, "") {
                let mut shot = open(setup);
                let found = seen(&mut shot, setup.scale);
                let (ix, iy) = centre(found.icon);
                let (sx, sy) = centre(found.slot);
                assert!((ix - sx).abs() <= 0.5, "{setup:?}: icon centre x {ix}, slot centre x {sx}");
                assert!((iy - sy).abs() <= 0.5, "{setup:?}: icon centre y {iy}, slot centre y {sy}");
                let size = Dim::IconControl.px_in(setup.class);
                assert!((found.icon[2] - size).abs() < 0.01 && (found.icon[3] - size).abs() < 0.01, "{setup:?}: icon {:?}", found.icon);
            }
        }
    }

    #[test]
    fn the_text_starts_a_token_after_the_slot_and_the_role_is_search() {
        for look in LOOKS {
            for setup in setups(look, "wifi") {
                let mut shot = open(setup);
                let found = seen(&mut shot, setup.scale);
                let gap = S1.px_in(setup.class);
                let from_slot = found.text_start - (found.slot[0] + found.slot[2]);
                assert!((from_slot - gap).abs() <= 0.01, "{setup:?}: text starts {from_slot} after the slot, expected {gap}");
                assert_eq!(found.role, "SearchInput", "{setup:?}");
            }
        }
    }

    #[test]
    fn the_clear_button_is_centred_in_a_square_slot_at_the_trailing_edge() {
        for look in [Look::Plain, Look::Well, Look::Dense, Look::Flat] {
            for setup in setups(look, "wifi") {
                let mut shot = open(setup);
                let found = seen(&mut shot, setup.scale);
                let clear = found.clear.unwrap_or_else(|| panic!("{setup:?}: no clear button"));
                // The trailing slot is the field's height square at its end.
                let slot = [found.field[0] + found.field[2] - found.field[3], found.field[1], found.field[3], found.field[3]];
                let (bx, by) = centre(clear);
                let (sx, sy) = centre(slot);
                assert!((bx - sx).abs() <= 0.5, "{setup:?}: clear centre x {bx}, slot centre x {sx}");
                assert!((by - sy).abs() <= 0.5, "{setup:?}: clear centre y {by}, slot centre y {sy}");
                assert!((clear[2] - clear[3]).abs() < 0.01, "{setup:?}: the button is not square: {clear:?}");
            }
            // Empty, there is nothing to clear.
            let mut shot = open(Setup { class: SizeClass::Regular, scale: 1.0, look, typed: "" });
            assert!(seen(&mut shot, 1.0).clear.is_none(), "{look:?}: a clear button on an empty field");
        }
    }

    /// The bounds of what is drawn around the icon's box (the box and 3 px
    /// beyond it, so the field's rounded corner is out of it), as the box of
    /// the ink: a pixel is ink when it is at least a third of the way from
    /// the background to the strongest pixel there.
    fn ink_centre(frame: &Frame, icon: Box4, scale: f64) -> (f32, f32) {
        let s = scale as f32;
        let margin = 3.0;
        let (x0, y0) = (((icon[0] - margin) * s).floor() as usize, ((icon[1] - margin) * s).floor() as usize);
        let (x1, y1) = (((icon[0] + icon[2] + margin) * s).ceil() as usize, ((icon[1] + icon[3] + margin) * s).ceil() as usize);
        // The background is the region's corner, where the icon does not reach.
        let bg = frame.pixel(x0, y0);
        let distance = |px: [u8; 3]| -> u32 { (0..3).map(|c| (i32::from(px[c]) - i32::from(bg[c])).unsigned_abs()).sum() };
        let strongest = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).map(|(x, y)| distance(frame.pixel(x, y))).max().unwrap_or(0);
        assert!(strongest >= 60, "nothing is drawn around the icon {icon:?} (strongest {strongest})");
        let floor = strongest / 3;
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
        for y in y0..=y1 {
            for x in x0..=x1 {
                if distance(frame.pixel(x, y)) >= floor {
                    left = left.min(x);
                    right = right.max(x);
                    top = top.min(y);
                    bottom = bottom.max(y);
                }
            }
        }
        (((left + right + 1) as f32 / 2.0) / s, ((top + bottom + 1) as f32 / 2.0) / s)
    }

    #[test]
    fn the_ink_of_the_search_glyph_is_centred_in_its_slot_on_the_pixels() {
        let mut worst = 0.0_f32;
        for look in [Look::Plain, Look::Well, Look::Dense, Look::Flat] {
            for setup in setups(look, "") {
                let mut shot = open(setup);
                let found = seen(&mut shot, setup.scale);
                let frame = Frame::of(&mut shot);
                let (ix, iy) = ink_centre(&frame, found.icon, setup.scale);
                let (sx, sy) = centre(found.slot);
                eprintln!("{setup:?}: ink centre ({ix:.2}, {iy:.2}), slot centre ({sx:.2}, {sy:.2})");
                worst = worst.max((ix - sx).abs()).max((iy - sy).abs());
                assert!((ix - sx).abs() <= 0.75, "{setup:?}: ink centre x {ix}, slot centre x {sx}");
                assert!((iy - sy).abs() <= 0.75, "{setup:?}: ink centre y {iy}, slot centre y {sy}");
            }
        }
        eprintln!("worst ink offset {worst:.2} px");
    }

    /// A click at a logical point of a window at `scale`.
    fn click(shot: &mut Snapshot, at: (f32, f32), scale: f64) {
        shot.click(at.0 * scale as f32, at.1 * scale as f32);
        shot.settle(Duration::from_millis(100));
    }

    #[test]
    fn escape_clears_the_text_then_the_next_escape_leaves_the_field() {
        let setup = Setup { class: SizeClass::Regular, scale: 1.0, look: Look::Plain, typed: "wifi" };
        let mut shot = open(setup);
        let found = seen(&mut shot, 1.0);
        click(&mut shot, (found.field[0] + found.field[2] / 2.0, found.field[1] + found.field[3] / 2.0), 1.0);
        shot.key(Key::Named(NamedKey::Escape));
        shot.settle(Duration::from_millis(100));
        assert_eq!(CHANGES.with(|log| log.borrow().clone()), vec![String::new()]);
        assert!(seen(&mut shot, 1.0).clear.is_none(), "the clear button stays on an emptied field");
        // Empty, Escape is the default: no change is reported again.
        shot.key(Key::Named(NamedKey::Escape));
        assert_eq!(CHANGES.with(|log| log.borrow().len()), 1);
    }

    #[test]
    fn the_clear_button_empties_the_field_and_reports_it() {
        let setup = Setup { class: SizeClass::Regular, scale: 2.0, look: Look::Plain, typed: "wifi" };
        let mut shot = open(setup);
        let found = seen(&mut shot, 2.0);
        click(&mut shot, centre(found.clear.expect("a clear button")), 2.0);
        assert_eq!(CHANGES.with(|log| log.borrow().clone()), vec![String::new()]);
        assert!(seen(&mut shot, 2.0).clear.is_none());
    }

    /// Every Rust file of the four apps and libraries that draw search fields.
    fn sources() -> Vec<(std::path::PathBuf, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        out.push((path, text));
                    }
                }
            }
        }
        let packages = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut out = Vec::new();
        for package in ["bingux-ui", "bingux-settings", "bingux-files-app", "bingux-taxonomy"] {
            walk(&packages.join(package).join("src"), &mut out);
        }
        out
    }

    #[test]
    fn no_screen_draws_its_own_search_field() {
        let files = sources();
        assert!(files.len() > 100, "found only {} sources", files.len());
        let mut found = Vec::new();
        for (path, text) in &files {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            // The address field is a path field that turns into a search: it
            // shares the slot through `leading` and asks for the search role
            // with `search()` itself.
            if name == "address_edit.rs" || name == "search_field.rs" || name == "text_field.rs" {
                continue;
            }
            for (number, line) in text.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                // A text field given the search glyph, or the sidebar's old
                // glyph column, is a search field built by hand.
                if (code.contains(".leading(") && code.contains("Icon::Search"))
                    || code.contains("leading_column(")
                {
                    found.push(format!("{}:{}: {code}", path.display(), number + 1));
                }
            }
        }
        assert!(found.is_empty(), "search fields built by hand, use search_field:\n{}", found.join("\n"));
    }

    #[test]
    fn a_field_under_the_pointer_takes_the_hover_layer_and_one_beside_it_does_not() {
        let setup = Setup { class: SizeClass::Regular, scale: 1.0, look: Look::Plain, typed: "" };
        let mut shot = open(setup);
        let found = seen(&mut shot, 1.0);
        // Empty space in the field, right of the placeholder.
        let at = (found.field[0] + found.field[2] - 40.0, found.field[1] + found.field[3] / 2.0);
        let rest = Frame::of(&mut shot).pixel(at.0 as usize, at.1 as usize);
        shot.hover(at.0, at.1);
        shot.settle(Duration::from_millis(100));
        let hovered = Frame::of(&mut shot).pixel(at.0 as usize, at.1 as usize);
        assert_ne!(rest, hovered, "the field looks the same under the pointer");
        shot.hover(found.field[0] + found.field[2] + 10.0, found.field[1] + found.field[3] + 10.0);
        shot.settle(Duration::from_millis(100));
        assert_eq!(Frame::of(&mut shot).pixel(at.0 as usize, at.1 as usize), rest, "the hover layer stays after the pointer leaves");
    }

    #[test]
    fn a_disabled_field_keeps_its_text_and_slot_but_offers_no_clear_button_and_ignores_escape() {
        let setup = Setup { class: SizeClass::Regular, scale: 1.0, look: Look::Disabled, typed: "wifi" };
        let mut shot = open(setup);
        let found = seen(&mut shot, 1.0);
        assert!(found.clear.is_none(), "a disabled field has a clear button");
        assert!((found.slot[2] - found.field[3]).abs() < 0.01);
        click(&mut shot, (found.field[0] + found.field[2] / 2.0, found.field[1] + found.field[3] / 2.0), 1.0);
        shot.key(Key::Named(NamedKey::Escape));
        assert!(CHANGES.with(|log| log.borrow().is_empty()), "a disabled field cleared");
    }

    #[test]
    fn enter_submits_the_text() {
        let setup = Setup { class: SizeClass::Regular, scale: 1.0, look: Look::Plain, typed: "wifi" };
        let mut shot = open(setup);
        let found = seen(&mut shot, 1.0);
        click(&mut shot, (found.field[0] + found.field[2] / 2.0, found.field[1] + found.field[3] / 2.0), 1.0);
        shot.key(Key::Named(NamedKey::Enter));
        assert_eq!(SUBMITS.with(|log| log.borrow().clone()), vec!["wifi".to_string()]);
    }
}
