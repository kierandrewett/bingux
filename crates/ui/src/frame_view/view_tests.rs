//! The frame view mounted in a window, headless: what it tells the producer
//! when it is laid out, what it draws when a frame arrives, the input it
//! forwards at the scales a display has, and how it takes part in focus.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use freya::prelude::*;
use freya_core::elements::label::Label;
use freya_testing::prelude::{ImeEventName, KeyboardEventName, PlatformEvent};
use torin::prelude::Area;

use super::frame_view;
use super::geometry::snap;
use super::input::{
    FocusEscape, FrameFit, KeyInput, PointerAction, PointerButtons, PointerInput, TextInput,
    TouchInput, ViewSize, WheelDelta, WheelInput,
};
use super::pattern::PatternSource;
use crate::controls::action_button;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::{DEFAULT_ACCENT, Scheme, SharedTokens, Tokens, freya_theme};
use crate::window::WindowHost;

const SCALES: [f64; 3] = [1.0, 1.25, 2.0];
const SETTLE: Duration = Duration::from_millis(120);

/// What the producer's callbacks heard, in order.
#[derive(Default)]
struct Log {
    sizes: Vec<ViewSize>,
    pointers: Vec<PointerInput>,
    wheels: Vec<WheelInput>,
    keys: Vec<KeyInput>,
    texts: Vec<TextInput>,
    touches: Vec<TouchInput>,
    focus: Vec<bool>,
}

/// A view in a column with a button before it and after it, and what to test.
#[derive(Clone)]
struct Case {
    source: Arc<PatternSource>,
    log: Rc<RefCell<Log>>,
    width: f32,
    height: f32,
    escape: FocusEscape,
    fit: FrameFit,
    /// The producer renders at the size it is told.
    follows: bool,
    own_focus: bool,
    enabled: bool,
    placeholder: bool,
    pointer: bool,
    touch: bool,
    radius: Radius,
}

impl Case {
    fn new(source: Arc<PatternSource>) -> Self {
        Self {
            source,
            log: Rc::default(),
            width: 160.0,
            height: 120.0,
            escape: FocusEscape::Tab,
            fit: FrameFit::Stretch,
            follows: true,
            own_focus: false,
            enabled: true,
            placeholder: false,
            pointer: true,
            touch: false,
            radius: Radius::Mark,
        }
    }
}

impl PartialEq for Case {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source) && Rc::ptr_eq(&self.log, &other.log)
    }
}

#[derive(PartialEq)]
struct Root(Case);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let tokens = Tokens::new(DEFAULT_ACCENT, Scheme::PreferDark, false);
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(Host(self.0.clone()))
    }
}

#[derive(PartialEq)]
struct Host(Case);

impl Component for Host {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let case = self.0.clone();
        let mut wide = use_state(|| false);
        let mut shown = use_state(|| true);
        let width = if wide() {
            case.width + 40.0
        } else {
            case.width
        };
        let height = case.height;

        let log = case.log.clone();
        let source = case.source.clone();
        let follows = case.follows;
        let mut view = frame_view(case.source.clone(), "Test pattern")
            .fit(case.fit)
            .radius(case.radius)
            .focus_escape(case.escape)
            .enabled(case.enabled)
            .on_resize({
                let log = log.clone();
                move |size: ViewSize| {
                    log.borrow_mut().sizes.push(size);
                    if follows {
                        let (width, height) = size.device_size();
                        source.resize(width, height, size.scale as f32);
                        source.step();
                    }
                }
            })
            .on_wheel({
                let log = log.clone();
                move |input: WheelInput| log.borrow_mut().wheels.push(input)
            })
            .on_key({
                let log = log.clone();
                move |input: KeyInput| log.borrow_mut().keys.push(input)
            })
            .on_text({
                let log = log.clone();
                move |input: TextInput| log.borrow_mut().texts.push(input)
            })
            .on_focus({
                let log = log.clone();
                move |focused: bool| log.borrow_mut().focus.push(focused)
            });
        if case.pointer {
            let log = log.clone();
            view =
                view.on_pointer(move |input: PointerInput| log.borrow_mut().pointers.push(input));
        }
        if case.touch {
            let log = log.clone();
            view = view.on_touch(move |input: TouchInput| log.borrow_mut().touches.push(input));
        }
        if case.own_focus {
            view = view.producer_draws_focus();
        }
        if case.placeholder {
            view = view.placeholder("Starting");
        }
        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), p(S4), gap(S2)])
            .child(action_button("Before", |_| {}))
            .maybe(shown(), |column| {
                column.child(
                    rect()
                        .width(Size::px(width))
                        .height(Size::px(height))
                        .child(view),
                )
            })
            .child(action_button("After", |_| {}))
            .child(action_button("Grow", move |_| wide.set(!wide())))
            .child(action_button("Unmount", move |_| shown.set(false)))
    }
}

fn start(case: &Case, scale: f64) -> Snapshot {
    let case = case.clone();
    let mut shot = Snapshot::with_scale(
        move || Root(case.clone()).into_element(),
        (400.0, 360.0),
        scale,
    );
    shot.settle(SETTLE);
    shot
}

/// The view's box in device pixels, as laid out.
fn view_area(shot: &mut Snapshot) -> Area {
    shot.runner()
        .find_many(|node, element| {
            (element.accessibility().builder.class_name() == Some("FrameView"))
                .then(|| node.layout().area)
        })
        .into_iter()
        .next()
        .expect("the view is mounted")
}

/// The window as BGRA pixels.
fn picture(shot: &mut Snapshot) -> (usize, usize, Vec<u8>) {
    let png = shot.runner().render();
    let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("a PNG");
    let (width, height) = (image.width() as usize, image.height() as usize);
    let mut surface =
        raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
    surface.canvas().draw_image(&image, (0, 0), None);
    let info = surface.canvas().image_info();
    let mut out = vec![0u8; width * height * 4];
    assert!(
        surface
            .canvas()
            .read_pixels(&info, &mut out, width * 4, (0, 0))
    );
    (width, height, out)
}

fn pixel(picture: &(usize, usize, Vec<u8>), x: usize, y: usize) -> [u8; 4] {
    let at = (y * picture.0 + x) * 4;
    picture.2[at..at + 4].try_into().unwrap()
}

fn labels(shot: &mut Snapshot) -> Vec<String> {
    shot.runner()
        .find_many(|_, element| match Label::try_downcast(element) {
            Some(label) => Some(label.text.to_string()),
            None => Paragraph::try_downcast(element).map(|paragraph| {
                paragraph
                    .spans
                    .iter()
                    .map(|span| span.text.to_string())
                    .collect()
            }),
        })
}

/// The focus rings that belong to the view, whatever the buttons around it do.
fn view_rings(shot: &mut Snapshot) -> Vec<crate::focus::ring_check::RingProbe> {
    shot.focus_rings()
        .into_iter()
        .filter(|ring| ring.label.contains("FrameView"))
        .collect()
}

fn tab() -> Key {
    Key::Named(NamedKey::Tab)
}

const BORDER: [u8; 4] = [255, 255, 255, 255];
const RED: [u8; 4] = [40, 40, 230, 255];
const GREEN: [u8; 4] = [60, 200, 60, 255];
const BLUE: [u8; 4] = [230, 90, 40, 255];
const YELLOW: [u8; 4] = [40, 220, 230, 255];

// ---- size ---------------------------------------------------------------------

#[test]
fn the_view_reports_its_size_and_scale_at_every_scale() {
    for scale in SCALES {
        let case = Case::new(PatternSource::stepped(64, 64));
        let mut shot = start(&case, scale);
        let sizes = case.log.borrow().sizes.clone();
        assert!(!sizes.is_empty(), "{scale}x: the first size is reported");
        let size = *sizes.last().unwrap();
        assert_eq!(
            (size.width, size.height),
            (160.0, 120.0),
            "{scale}x: in logical pixels"
        );
        assert_eq!(size.scale, scale);
        let area = view_area(&mut shot);
        let (w, h) = size.device_size();
        assert_eq!(
            (w as f32, h as f32),
            (
                (160.0 * scale).round() as f32,
                (120.0 * scale).round() as f32
            )
        );
        assert!((area.width() - w as f32).abs() < 1.0 && (area.height() - h as f32).abs() < 1.0);
        // Once, not once per frame of the settle.
        assert_eq!(sizes.len(), 1, "{scale}x: {sizes:?}");
    }
}

#[test]
fn a_new_size_is_reported_once_and_the_last_frame_stays_until_the_producer_answers() {
    let source = PatternSource::stepped(64, 64);
    let mut case = Case::new(source.clone());
    case.follows = false;
    let mut shot = start(&case, 1.0);
    source.resize(160, 120, 1.0);
    source.step();
    shot.settle(SETTLE);
    let before = picture(&mut shot);
    let area = view_area(&mut shot);
    let (left, top) = (area.min_x() as usize, area.min_y() as usize);
    assert_eq!(
        pixel(&before, left + 80, top),
        BORDER,
        "the first frame is on screen"
    );

    // The page grows the view by 40 px: reported once, with the new size.
    assert!(shot.click_text("Grow"));
    shot.settle(SETTLE);
    let sizes = case.log.borrow().sizes.clone();
    assert_eq!(sizes.last().map(|s| s.width), Some(200.0), "{sizes:?}");
    assert_eq!(sizes.len(), 2, "one report for one change: {sizes:?}");
    // The producer has not answered: the old frame is drawn, stretched, not a blank.
    let after = picture(&mut shot);
    let area = view_area(&mut shot);
    let (left, top) = (area.min_x() as usize, area.min_y() as usize);
    assert_eq!(pixel(&after, left + 100, top), BORDER);
    assert_eq!(
        pixel(&after, area.max_x() as usize - 1, top + 60),
        BORDER,
        "the frame reaches the new edge"
    );
}

// ---- drawing -------------------------------------------------------------------

#[test]
fn before_the_first_frame_there_is_a_placeholder_and_after_it_the_picture_one_to_one() {
    for scale in SCALES {
        let source = PatternSource::stepped(64, 64);
        let mut case = Case::new(source.clone());
        case.follows = false;
        case.placeholder = true;
        let mut shot = start(&case, scale);
        assert!(
            labels(&mut shot).iter().any(|label| label == "Starting"),
            "{scale}x: the placeholder text"
        );
        let area = view_area(&mut shot);
        let before = picture(&mut shot);
        let centre = pixel(
            &before,
            (area.min_x() + area.width() / 2.0) as usize,
            area.min_y() as usize + 4,
        );
        assert_ne!(centre, BORDER);

        // The producer renders at the size it was told and wakes the view.
        let size = *case.log.borrow().sizes.last().unwrap();
        let (w, h) = size.device_size();
        source.resize(w, h, scale as f32);
        source.step();
        shot.settle(SETTLE);
        assert!(
            !labels(&mut shot).iter().any(|label| label == "Starting"),
            "{scale}x: the text goes with the first frame"
        );

        let shown = picture(&mut shot);
        let area = view_area(&mut shot);
        let (left, top) = (snap(area.min_x()) as usize, snap(area.min_y()) as usize);
        let (w, h) = (w as usize, h as usize);
        // The box's own pixels: the border on every edge's middle, nothing
        // shifted outward, and each corner marker where it belongs.
        let (mid_x, mid_y) = (w / 2, h / 2);
        assert_eq!(
            pixel(&shown, left + mid_x, top),
            BORDER,
            "{scale}x: top edge"
        );
        assert_eq!(
            pixel(&shown, left + mid_x, top + h - 1),
            BORDER,
            "{scale}x: bottom edge"
        );
        assert_eq!(
            pixel(&shown, left, top + mid_y),
            BORDER,
            "{scale}x: left edge"
        );
        assert_eq!(
            pixel(&shown, left + w - 1, top + mid_y),
            BORDER,
            "{scale}x: right edge"
        );
        assert_ne!(
            pixel(&shown, left + mid_x, top - 1),
            BORDER,
            "{scale}x: nothing outside the box"
        );
        assert_ne!(pixel(&shown, left - 1, top + mid_y), BORDER);
        assert_ne!(pixel(&shown, left + w, top + mid_y), BORDER);
        assert_ne!(pixel(&shown, left + mid_x, top + h), BORDER);
        assert_eq!(pixel(&shown, left + 4, top + 4), RED, "{scale}x");
        assert_eq!(pixel(&shown, left + w - 5, top + 4), GREEN, "{scale}x");
        assert_eq!(pixel(&shown, left + 4, top + h - 5), BLUE, "{scale}x");
        assert_eq!(pixel(&shown, left + w - 5, top + h - 5), YELLOW, "{scale}x");
    }
}

// ---- pointer -------------------------------------------------------------------

#[test]
fn pointer_events_are_in_the_views_logical_pixels_at_every_scale() {
    for scale in SCALES {
        let case = Case::new(PatternSource::stepped(64, 64));
        let mut shot = start(&case, scale);
        let area = view_area(&mut shot);
        let at = |x: f32, y: f32| {
            (
                area.min_x() + x * scale as f32,
                area.min_y() + y * scale as f32,
            )
        };
        case.log.borrow_mut().pointers.clear();

        let (px, py) = at(40.0, 20.0);
        shot.hover(px, py);
        shot.press((px, py));
        // Drag out of the view to its left, and let go there.
        let (ox, oy) = at(-30.0, 50.0);
        shot.move_to((ox, oy));
        shot.release((ox, oy));

        let pointers = case.log.borrow().pointers.clone();
        let actions: Vec<_> = pointers.iter().map(|p| p.action).collect();
        assert!(
            actions.contains(&PointerAction::Enter),
            "{scale}x: {actions:?}"
        );
        let down = pointers
            .iter()
            .find(|p| p.action == PointerAction::Down)
            .expect("a press");
        assert_eq!(down.button, Some(MouseButton::Left));
        assert_eq!(down.click_count, 1);
        assert_eq!(down.buttons, PointerButtons(PointerButtons::LEFT));
        assert!(
            (down.position.0 - 40.0).abs() < 0.05 && (down.position.1 - 20.0).abs() < 0.05,
            "{scale}x: {down:?}"
        );
        let up = pointers
            .iter()
            .rfind(|p| p.action == PointerAction::Up)
            .expect("the release outside still arrives");
        assert!(
            (up.position.0 + 30.0).abs() < 0.05 && (up.position.1 - 50.0).abs() < 0.05,
            "{scale}x: {up:?}"
        );
        assert_eq!(up.buttons, PointerButtons::default());
        assert_eq!(up.click_count, 1);
        let outside_moves = pointers
            .iter()
            .filter(|p| p.action == PointerAction::Move && p.position.0 < 0.0)
            .count();
        assert!(
            outside_moves >= 1,
            "{scale}x: the drag out of the view keeps reporting: {actions:?}"
        );
    }
}

#[test]
fn fast_clicks_count_and_a_press_outside_never_arrives() {
    let case = Case::new(PatternSource::stepped(64, 64));
    let mut shot = start(&case, 1.0);
    let area = view_area(&mut shot);
    case.log.borrow_mut().pointers.clear();
    shot.click(area.min_x() + 20.0, area.min_y() + 20.0);
    shot.click(area.min_x() + 21.0, area.min_y() + 20.0);
    shot.click(area.max_x() + 30.0, area.min_y() + 20.0);
    let downs: Vec<u32> = case
        .log
        .borrow()
        .pointers
        .iter()
        .filter(|p| p.action == PointerAction::Down)
        .map(|p| p.click_count)
        .collect();
    assert_eq!(downs, vec![1, 2], "the third press was outside");
}

#[test]
fn a_touch_is_a_pointer_press_unless_the_app_takes_touches() {
    let mut case = Case::new(PatternSource::stepped(64, 64));
    let mut shot = start(&case, 1.0);
    let area = view_area(&mut shot);
    case.log.borrow_mut().pointers.clear();
    shot.touch(area.min_x() + 10.0, area.min_y() + 10.0);
    let actions: Vec<_> = case
        .log
        .borrow()
        .pointers
        .iter()
        .map(|p| (p.action, p.button))
        .collect();
    assert!(
        actions.contains(&(PointerAction::Down, Some(MouseButton::Left))),
        "{actions:?}"
    );

    case.touch = true;
    case.log = Rc::default();
    let mut shot = start(&case, 1.0);
    let area = view_area(&mut shot);
    shot.touch(area.min_x() + 10.0, area.min_y() + 10.0);
    let log = case.log.borrow();
    assert!(
        log.pointers.iter().all(|p| p.action != PointerAction::Down),
        "touch is not also a press"
    );
    let phases: Vec<_> = log.touches.iter().map(|t| t.phase).collect();
    assert_eq!(phases, vec![TouchPhase::Started, TouchPhase::Ended]);
    assert!((log.touches[0].position.0 - 10.0).abs() < 0.05);
}

#[test]
fn the_wheel_arrives_with_the_doms_sign_and_is_kept_from_the_page() {
    let case = Case::new(PatternSource::stepped(64, 64));
    let mut shot = start(&case, 1.0);
    let area = view_area(&mut shot);
    // Freya: a negative `dy` moves the content up, which is scrolling down.
    shot.scroll(area.min_x() + 30.0, area.min_y() + 30.0, -120.0);
    let wheels = case.log.borrow().wheels.clone();
    assert_eq!(wheels.len(), 1, "{wheels:?}");
    let (x, y) = match wheels[0].delta {
        WheelDelta::Pixels { x, y } | WheelDelta::Lines { x, y } => (x, y),
    };
    assert!(y > 0.0 && x == 0.0, "{:?}", wheels[0]);
    assert!((wheels[0].position.0 - 30.0).abs() < 0.05);
}

#[test]
fn the_producers_cursor_shows_over_the_view() {
    use super::input::CursorKind;
    let source = PatternSource::stepped(64, 64);
    let case = Case::new(source.clone());
    let mut shot = start(&case, 1.0);
    let area = view_area(&mut shot);
    shot.hover(area.min_x() + 10.0, area.min_y() + 10.0);
    assert_eq!(shot.runner().cursor_icon(), CursorIcon::Default);
    source.set_cursor(CursorKind::Pointer);
    shot.settle(SETTLE);
    assert_eq!(shot.runner().cursor_icon(), CursorIcon::Pointer);
    source.set_cursor(CursorKind::Text);
    shot.settle(SETTLE);
    assert_eq!(shot.runner().cursor_icon(), CursorIcon::Text);
    source.set_cursor(CursorKind::Hidden);
    shot.settle(SETTLE);
    assert!(!shot.runner().cursor_visible());
}

// ---- keyboard, focus, text ---------------------------------------------------------

/// Tab to the view: Before, then the view.
fn focus_the_view(shot: &mut Snapshot) {
    shot.key(tab());
    shot.key(tab());
}

#[test]
fn the_view_is_a_tab_stop_with_one_ring_and_tells_the_producer_about_focus() {
    let case = Case::new(PatternSource::stepped(64, 64));
    let mut shot = start(&case, 1.0);
    assert!(case.log.borrow().focus.is_empty());
    shot.key(tab());
    assert!(
        case.log.borrow().focus.is_empty(),
        "the first stop is the button before it"
    );
    shot.key(tab());
    shot.settle(SETTLE);
    assert_eq!(case.log.borrow().focus, vec![true]);
    shot.settle(SETTLE);
    let rings = view_rings(&mut shot);
    assert_eq!(rings.len(), 1, "one ring, around the view: {rings:?}");
    assert!(rings[0].progress > 0.9, "and it has arrived: {rings:?}");
    assert!(
        shot.ring_mismatches().is_empty(),
        "the ring is around the face and follows its radius: {:?}",
        shot.ring_mismatches()
    );
    shot.key(tab());
    shot.settle(SETTLE);
    assert_eq!(case.log.borrow().focus, vec![true, false]);
    shot.settle(Duration::from_millis(500));
    assert!(
        view_rings(&mut shot).iter().all(|ring| ring.progress < 0.1),
        "the ring leaves with the focus"
    );
}

#[test]
fn a_producer_that_draws_its_own_focus_gets_no_ring_but_is_still_a_stop() {
    let mut case = Case::new(PatternSource::stepped(64, 64));
    case.own_focus = true;
    let mut shot = start(&case, 1.0);
    focus_the_view(&mut shot);
    shot.settle(SETTLE);
    assert_eq!(case.log.borrow().focus, vec![true]);
    assert!(
        view_rings(&mut shot).is_empty(),
        "no ring around a view whose producer draws its own focus"
    );
}

#[test]
fn a_disabled_view_is_not_a_stop_and_hears_nothing() {
    let mut case = Case::new(PatternSource::stepped(64, 64));
    case.enabled = false;
    let mut shot = start(&case, 1.0);
    let area = view_area(&mut shot);
    shot.key(tab());
    shot.key(tab());
    shot.click(area.min_x() + 10.0, area.min_y() + 10.0);
    shot.settle(SETTLE);
    let log = case.log.borrow();
    assert!(log.focus.is_empty() && log.pointers.is_empty() && log.keys.is_empty());
}

#[test]
fn keys_reach_the_producer_with_dom_names_text_and_repeat() {
    let case = Case::new(PatternSource::stepped(64, 64));
    let mut shot = start(&case, 1.0);
    focus_the_view(&mut shot);
    let a = Key::Character("a".into());
    shot.key_event(
        KeyboardEventName::KeyDown,
        a.clone(),
        Code::KeyA,
        Modifiers::empty(),
    );
    shot.key_event(
        KeyboardEventName::KeyDown,
        a.clone(),
        Code::KeyA,
        Modifiers::empty(),
    );
    shot.key_event(
        KeyboardEventName::KeyUp,
        a.clone(),
        Code::KeyA,
        Modifiers::empty(),
    );
    shot.key_event(
        KeyboardEventName::KeyDown,
        Key::Named(NamedKey::ArrowDown),
        Code::ArrowDown,
        Modifiers::SHIFT,
    );
    let keys = case.log.borrow().keys.clone();
    assert_eq!(keys.len(), 4, "{keys:?}");
    assert_eq!(
        (
            keys[0].key_string().as_str(),
            keys[0].code_string().as_str()
        ),
        ("a", "KeyA")
    );
    assert!(keys[0].pressed && !keys[0].repeat && keys[0].text.as_deref() == Some("a"));
    assert!(keys[1].pressed && keys[1].repeat);
    assert!(!keys[2].pressed && keys[2].text.is_none());
    assert_eq!(keys[3].key_string(), "ArrowDown");
    assert_eq!(keys[3].modifiers, Modifiers::SHIFT);
    // The arrow did not also walk the window's focus away.
    assert_eq!(case.log.borrow().focus, vec![true]);
}

#[test]
fn by_default_tab_leaves_the_view_and_the_producer_never_sees_it() {
    let case = Case::new(PatternSource::stepped(64, 64));
    let mut shot = start(&case, 1.0);
    focus_the_view(&mut shot);
    shot.key(tab());
    shot.settle(SETTLE);
    assert_eq!(
        case.log.borrow().focus,
        vec![true, false],
        "focus moved on to the next control"
    );
    assert!(case.log.borrow().keys.iter().all(|key| key.key != tab()));
    // Shift+Tab comes back.
    shot.key_with(tab(), Modifiers::SHIFT);
    shot.settle(SETTLE);
    assert_eq!(case.log.borrow().focus, vec![true, false, true]);
}

#[test]
fn with_focus_escape_producer_tab_goes_to_the_producer_and_focus_stays() {
    let mut case = Case::new(PatternSource::stepped(64, 64));
    case.escape = FocusEscape::Producer;
    let mut shot = start(&case, 1.0);
    focus_the_view(&mut shot);
    shot.key(tab());
    shot.key_with(tab(), Modifiers::SHIFT);
    shot.settle(SETTLE);
    let log = case.log.borrow();
    assert_eq!(log.focus, vec![true], "focus did not leave");
    let tabs: Vec<_> = log
        .keys
        .iter()
        .filter(|key| key.key == tab())
        .map(|key| key.modifiers)
        .collect();
    assert_eq!(tabs, vec![Modifiers::empty(), Modifiers::SHIFT]);
}

#[test]
fn f6_is_never_forwarded() {
    let mut case = Case::new(PatternSource::stepped(64, 64));
    case.escape = FocusEscape::Producer;
    let mut shot = start(&case, 1.0);
    focus_the_view(&mut shot);
    shot.key(Key::Named(NamedKey::F6));
    assert!(
        case.log
            .borrow()
            .keys
            .iter()
            .all(|key| key.key != Key::Named(NamedKey::F6))
    );
}

#[test]
fn input_method_text_arrives_as_preedit_then_commit() {
    let case = Case::new(PatternSource::stepped(64, 64));
    let mut shot = start(&case, 1.0);
    focus_the_view(&mut shot);
    shot.platform_event(PlatformEvent::ImePreedit {
        name: ImeEventName::Preedit,
        text: "にほ".into(),
        cursor: Some((0, 6)),
    });
    shot.key_event(
        KeyboardEventName::KeyDown,
        Key::Character("日本".into()),
        Code::Unidentified,
        Modifiers::empty(),
    );
    shot.platform_event(PlatformEvent::ImePreedit {
        name: ImeEventName::Preedit,
        text: String::new(),
        cursor: None,
    });
    let texts = case.log.borrow().texts.clone();
    assert_eq!(
        texts,
        vec![
            TextInput::Preedit {
                text: "にほ".into(),
                cursor: Some((0, 6))
            },
            TextInput::Commit("日本".into()),
            TextInput::Preedit {
                text: String::new(),
                cursor: None
            },
        ]
    );
    assert!(
        case.log.borrow().keys.is_empty(),
        "a commit is text, not a key"
    );
}
